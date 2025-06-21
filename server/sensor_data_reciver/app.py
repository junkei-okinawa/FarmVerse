import argparse
import asyncio
import io
import logging
import os
import re
import time
from dataclasses import dataclass
from datetime import datetime
from typing import Dict, Optional, Tuple

import influxdb_client
import serial
import serial_asyncio
from dotenv import load_dotenv
from influxdb_client import Point
from influxdb_client.client.write_api import SYNCHRONOUS
from PIL import Image

load_dotenv()

# --- Configuration ---
@dataclass
class Config:
    """アプリケーション設定"""
    SERIAL_PORT: str = "/dev/ttyACM0"
    BAUD_RATE: int = 115200
    IMAGE_DIR: str = "images"
    IMAGE_TIMEOUT: float = 20.0
    MAX_BUFFER_SIZE: int = 10 * 1024 * 1024  # 10MB
    MAX_DATA_LEN: int = 512
    INFLUXDB_URL: str = "http://localhost:8086"
    INFLUXDB_ORG: str = "agri"
    INFLUXDB_BUCKET: str = "balcony"
    DEBUG_FRAME_PARSING: bool = False

# グローバル設定インスタンス
config = Config()

# --- Logging Setup ---
logging.basicConfig(
    level=logging.INFO, format="%(asctime)s - %(levelname)s - %(message)s"
)
logger = logging.getLogger(__name__)

# influxDB
token = os.environ.get("INFLUXDB_TOKEN")
client = influxdb_client.InfluxDBClient(url=config.INFLUXDB_URL, token=token, org=config.INFLUXDB_ORG)
write_api = client.write_api(write_options=SYNCHRONOUS)

# --- Protocol Constants ---
MAC_ADDRESS_LENGTH = 6
FRAME_TYPE_LENGTH = 1
SEQUENCE_NUM_LENGTH = 4
LENGTH_FIELD_BYTES = 4
CHECKSUM_LENGTH = 4

# 新しい強化されたフレームマーカー（4バイト）
START_MARKER = b"\xfa\xce\xaa\xbb"
END_MARKER = b"\xcd\xef\x56\x78"

# フレームタイプ定義
FRAME_TYPE_HASH = 1
FRAME_TYPE_DATA = 2
FRAME_TYPE_EOF = 3

# フレームヘッダー長（開始マーカー + MACアドレス + フレームタイプ + シーケンス + データ長）
HEADER_LENGTH = len(START_MARKER) + MAC_ADDRESS_LENGTH + FRAME_TYPE_LENGTH + SEQUENCE_NUM_LENGTH + LENGTH_FIELD_BYTES

# フレームフッター長（チェックサム + 終了マーカー）
FOOTER_LENGTH = CHECKSUM_LENGTH + len(END_MARKER)

# 画像タイムアウト設定（長くして複数カメラの同時送信でも完了できるように）
IMAGE_TIMEOUT = 20.0  # Timeout for receiving chunks for one image (seconds)

# デバッグフラグ
DEBUG_FRAME_PARSING = False  # フレーム解析の詳細をログ出力するか

# --- Global State ---
# グローバル変数をImageReceiverクラスに移行
image_buffers = {}
last_receive_time = {}
stats = {"received_images": 0, "total_bytes": 0, "start_time": time.time()}


def ensure_dir_exists():
    if not os.path.exists(config.IMAGE_DIR):
        os.makedirs(config.IMAGE_DIR)
        logger.info(f"Created directory: {config.IMAGE_DIR}")


async def save_image(sender_mac_str: str, image_data: bytes) -> None:
    """Saves the received complete image data (async for potential I/O)."""
    try:
        timestamp = datetime.now().strftime("%Y%m%d_%H%M%S_%f")
        filename = f"{config.IMAGE_DIR}/{sender_mac_str.replace(':', '')}_{timestamp}.jpg"
        loop = asyncio.get_running_loop()
        await loop.run_in_executor(None, write_file_sync, filename, image_data)

        file_size = len(image_data)
        stats["received_images"] += 1
        stats["total_bytes"] += file_size
        logger.info(
            f"Saved image from {sender_mac_str}, size: {file_size} bytes as: {filename}"
        )

        if stats["received_images"] > 0 and stats["received_images"] % 10 == 0:
            elapsed = time.time() - stats["start_time"]
            try:
                avg_size = stats["total_bytes"] / stats["received_images"]
                logger.info(
                    f"Stats: {stats['received_images']} images, avg size: {avg_size:.1f} bytes, elapsed: {elapsed:.1f}s"
                )
            except ZeroDivisionError:
                logger.info("Stats: 0 images received yet.")

    except Exception as e:
        logger.error(f"Error saving image for MAC {sender_mac_str}: {e}")


def write_file_sync(filename: str, data: bytes) -> None:
    """Synchronous helper function to write file data."""
    with open(filename, "wb") as f:
        f.write(data)

    # 回転画像保存
    try:
        # バイト列から Image オブジェクト生成
        im = Image.open(io.BytesIO(data))
        # 左90度回転
        rotated = im.rotate(90, expand=True)
        # ファイル名から MAC 部分だけ取り出し
        base = os.path.splitext(os.path.basename(filename))[0].split("_")[0]
        rotated_filename = os.path.join(config.IMAGE_DIR, f"{base}.jpg")
        rotated.save(rotated_filename)
        logger.info(f"Saved rotated image: {rotated_filename}")
    except Exception as e:
        logger.error(f"Error saving rotated image: {e}")

# --- Frame Processing Classes ---

class FrameParser:
    """フレーム解析クラス"""
    
    @staticmethod
    def parse_header(buffer: bytearray, start_pos: int) -> Tuple[str, int, int, int]:
        """ヘッダー部分の解析を分離"""
        header_start = start_pos + len(START_MARKER)
        mac_bytes = buffer[header_start : header_start + MAC_ADDRESS_LENGTH]
        sender_mac = ":".join(f"{b:02x}" for b in mac_bytes)
        
        frame_type_pos = header_start + MAC_ADDRESS_LENGTH
        frame_type = buffer[frame_type_pos]
        
        seq_num_pos = frame_type_pos + FRAME_TYPE_LENGTH
        seq_bytes = buffer[seq_num_pos:seq_num_pos + SEQUENCE_NUM_LENGTH]
        seq_num = int.from_bytes(seq_bytes, byteorder="big")
        
        data_len_pos = seq_num_pos + SEQUENCE_NUM_LENGTH
        len_bytes = buffer[data_len_pos:data_len_pos + LENGTH_FIELD_BYTES]
        data_len = int.from_bytes(len_bytes, byteorder="big")
        
        return sender_mac, frame_type, seq_num, data_len

    @staticmethod
    def validate_frame_data(data_len: int, mac_bytes: bytes) -> bool:
        """フレームデータの検証"""
        if data_len > config.MAX_DATA_LEN:
            raise ValueError(f"Data length {data_len} exceeds maximum {config.MAX_DATA_LEN}")
        
        if len(mac_bytes) != MAC_ADDRESS_LENGTH:
            raise ValueError(f"Invalid MAC address length: {len(mac_bytes)}")
        
        return True

    @staticmethod
    def sanitize_filename(sender_mac_str: str, timestamp: str) -> str:
        """ファイル名のサニタイズ"""
        safe_mac = re.sub(r'[^\w\-_]', '', sender_mac_str.replace(':', ''))
        safe_timestamp = re.sub(r'[^\w\-_]', '', timestamp)
        return f"{safe_mac}_{safe_timestamp}.jpg"


class ImageReceiver:
    """画像受信管理クラス"""
    
    def __init__(self):
        self.image_buffers: Dict[str, bytearray] = {}
        self.last_receive_time: Dict[str, float] = {}
        self.stats = {"received_images": 0, "total_bytes": 0, "start_time": time.time()}
    
    def check_memory_usage(self) -> None:
        """メモリ使用量の監視"""
        total_buffer_size = sum(len(buf) for buf in self.image_buffers.values())
        
        if total_buffer_size > config.MAX_BUFFER_SIZE:
            logger.warning(f"Total buffer size {total_buffer_size} exceeds limit")
            # 最も古いバッファを削除
            if self.last_receive_time:
                oldest_mac = min(self.last_receive_time.keys(), 
                               key=lambda x: self.last_receive_time[x])
                self._cleanup_buffer(oldest_mac)
    
    def _cleanup_buffer(self, sender_mac: str) -> None:
        """指定されたMACアドレスのバッファをクリーンアップ"""
        if sender_mac in self.image_buffers:
            del self.image_buffers[sender_mac]
        if sender_mac in self.last_receive_time:
            del self.last_receive_time[sender_mac]
        logger.info(f"Cleaned up buffer for {sender_mac}")
    
    async def cleanup_resources(self) -> None:
        """リソースのクリーンアップ"""
        try:
            if hasattr(self, 'write_api') and self.write_api:
                self.write_api.close()
            if hasattr(self, 'client') and self.client:
                self.client.close()
        except Exception as e:
            logger.error(f"Error during cleanup: {e}")

class VoltageDataProcessor:
    """電圧データ処理クラス"""
    
    @staticmethod
    def parse_voltage_data(payload: str) -> Optional[float]:
        """電圧データの解析"""
        try:
            payload_split = payload.split(",")
            for part in payload_split:
                if part.startswith("VOLT:"):
                    volt_value = part.split(":")[1]
                    return float(volt_value)
            return None
        except (ValueError, IndexError):
            return None
    
    @staticmethod
    def parse_temperature_data(payload: str) -> Optional[float]:
        """温度データの解析"""
        try:
            payload_split = payload.split(",")
            for part in payload_split:
                if part.startswith("TEMP:"):
                    temp_value = part.split(":")[1]
                    return float(temp_value)
            return None
        except (ValueError, IndexError):
            return None

# グローバルな画像受信管理インスタンス
image_receiver = ImageReceiver()

# --- Global State (改善版) ---
# グローバル変数をImageReceiverクラスに移行
image_buffers = image_receiver.image_buffers
last_receive_time = image_receiver.last_receive_time
stats = image_receiver.stats

# 電圧データプロセッサのインスタンス
voltage_processor = VoltageDataProcessor()

# --- Serial Protocol Class ---
class SerialProtocol(asyncio.Protocol):
    """Asyncio protocol to handle serial data."""

    def __init__(self, connection_lost_future: asyncio.Future):
        super().__init__()
        self.buffer = bytearray()
        self.transport = None
        # Store the future passed from the main loop
        self.connection_lost_future = connection_lost_future
        self.frame_start_time = None  # フレーム受信開始時間
        logger.info("Serial Protocol initialized.")

    def connection_made(self, transport):
        self.transport = transport
        # <<<--- [修正2] No longer need to create future here ---
        try:
            # Setting DTR might reset some devices, handle potential issues
            transport.serial.dtr = True
            logger.info(f"Serial port {transport.serial.port} opened, DTR set.")
        except IOError as e:
            logger.warning(f"Could not set DTR on {transport.serial.port}: {e}")
        # Debug log to confirm the future exists
        # logger.debug(f"connection_made: Future object ID = {id(self.connection_lost_future)}")

    def data_received(self, data):
        """Called when data is received from the serial port."""
        self.buffer.extend(data)
        self.process_buffer()  # Process the buffer immediately

    def process_buffer(self):
        """Process the buffer to find and handle complete frames with enhanced frame format."""
        global image_buffers, last_receive_time
        # processed_frame = False  # Flag to indicate if a frame was processed in this call

        while True:  # Process all complete frames in the buffer
            # フレームレベルのタイムアウトチェック - 長めの値に設定
            if self.frame_start_time and (
                time.monotonic() - self.frame_start_time > 2.0
            ):  # 2秒タイムアウト（複数カメラ対応のため延長）
                logger.warning(
                    "Frame timeout detected. Discarding partial frame data."
                )
                start_index_after_timeout = self.buffer.find(
                    START_MARKER, 1
                )  # 次のマーカーを探す
                if start_index_after_timeout != -1:
                    logger.warning(
                        f"Discarding {start_index_after_timeout} bytes due to frame timeout."
                    )
                    self.buffer = self.buffer[start_index_after_timeout:]
                else:
                    logger.warning(
                        "No further start marker found after frame timeout. Clearing buffer."
                    )
                    self.buffer.clear()
                self.frame_start_time = None  # タイムアウト処理後はリセット

            # 開始マーカー（現在は4バイト）を探す
            start_index = self.buffer.find(START_MARKER)
            if start_index == -1:
                # Keep the last potential start marker bytes if buffer is short
                if len(self.buffer) >= len(START_MARKER):
                    # 開始マーカーの一部かもしれないので、末尾を残す
                    self.buffer = self.buffer[-(len(START_MARKER) - 1) :]
                break  # Need more data

            # 開始マーカーが見つかったら、フレーム受信開始時間を記録
            if self.frame_start_time is None:
                self.frame_start_time = time.monotonic()

            if start_index > 0:
                discarded_data = self.buffer[:start_index]
                logger.warning(
                    f"Discarding {start_index} bytes before start marker: {discarded_data.hex()}"
                )
                self.buffer = self.buffer[start_index:]
                self.frame_start_time = time.monotonic()  # マーカーを見つけたので時間リセット
                continue  # バッファを更新したのでループの最初から再試行

            # ヘッダー全体を受信するのに十分なデータがあるか確認
            # ヘッダー = [START_MARKER(4) + MAC(6) + FRAME_TYPE(1) + SEQUENCE(4) + DATA_LEN(4)]
            if len(self.buffer) < len(START_MARKER) + MAC_ADDRESS_LENGTH + FRAME_TYPE_LENGTH + SEQUENCE_NUM_LENGTH + LENGTH_FIELD_BYTES:
                if config.DEBUG_FRAME_PARSING:
                    logger.debug(f"Need more data for header. Buffer len: {len(self.buffer)}")
                break  # Need more data for header

            # FrameParserを使用してヘッダーを解析
            try:
                sender_mac, frame_type, seq_num, data_len = FrameParser.parse_header(self.buffer, 0)
                
                # MACアドレス長の検証
                header_start = len(START_MARKER)
                mac_bytes = self.buffer[header_start : header_start + MAC_ADDRESS_LENGTH]
                FrameParser.validate_frame_data(data_len, mac_bytes)
                
            except (ValueError, IndexError) as e:
                logger.error(f"Frame decode error: {e}. Discarding frame.")
                next_start = self.buffer.find(START_MARKER, 1)
                if next_start != -1:
                    self.buffer = self.buffer[next_start:]
                else:
                    self.buffer.clear()
                self.frame_start_time = None
                continue

            # フレーム全体の長さを計算
            # START_MARKER + MAC + FRAME_TYPE + SEQUENCE + DATA_LEN + DATA + CHECKSUM + END_MARKER
            frame_end_index = (len(START_MARKER) + MAC_ADDRESS_LENGTH + FRAME_TYPE_LENGTH + 
                            SEQUENCE_NUM_LENGTH + LENGTH_FIELD_BYTES + data_len + 
                            CHECKSUM_LENGTH + len(END_MARKER))
                            
            if len(self.buffer) < frame_end_index:
                if DEBUG_FRAME_PARSING:
                    logger.debug(f"Need more data for full frame. Expected: {frame_end_index}, Have: {len(self.buffer)}")
                break  # Need more data for full frame

            # データ部分の位置を計算
            data_start_index = len(START_MARKER) + MAC_ADDRESS_LENGTH + FRAME_TYPE_LENGTH + SEQUENCE_NUM_LENGTH + LENGTH_FIELD_BYTES
            chunk_data = self.buffer[data_start_index : data_start_index + data_len]
            
            # チェックサム部分の位置
            checksum_start = data_start_index + data_len
            # checksum_bytes = self.buffer[checksum_start:checksum_start + CHECKSUM_LENGTH]
            
            # エンドマーカーの位置
            end_marker_start = checksum_start + CHECKSUM_LENGTH
            footer = self.buffer[end_marker_start:frame_end_index]

            # エンドマーカーを確認
            if footer == END_MARKER:
                # processed_frame = True
                self.frame_start_time = None  # 正常にフレームを処理したので時間計測リセット
                
                # チェックサムの検証（オプション）
                # 実際のチェックサム検証コードはここに追加
                
                # フレームタイプに応じた処理
                frame_type_str = "UNKNOWN"
                if frame_type == FRAME_TYPE_HASH:
                    frame_type_str = "HASH"
                    try:
                        # HASHフレームのペイロードの先頭5バイトは 'HASH:' なのでスキップ
                        payload_str = chunk_data[5:].decode('ascii')
                    except UnicodeDecodeError:
                        logger.warning(f"Could not decode HASH payload from {sender_mac}")
                        return

                    point = None  # InfluxDBのポイントを初期化

                    # payload_str の内容 "HASH:<hash>,VOLT:<u8>,TEMP:<f32>,<timestamp>"
                    # <timestamp>は image_sender で画像取得タイミングのRTCタイムスタンプなのでズレている可能性が高い。ログ確認のために受信している。
                    logger.info(f"Received HASH frame from {sender_mac}: {payload_str}")
                    payload_splet = payload_str.split(",")
                    if len(payload_splet) < 2: 
                        logger.warning(f"Invalid HASH payload format from {sender_mac}: {payload_str}")
                        return
                    # hash_value = payload_splet[0]  # HASH値自体はここでは不要
                    volt_log_entry = payload_splet[1]
                    temp_value = payload_splet[2]
                    # timestamp_str = payload_splet[3]  # タイムスタンプは受信しているが、画像取得タイミングのRTCタイムスタンプなのでズレている可能性が高い

                    # 電圧情報を抽出
                    voltage = None
                    if "VOLT:" in volt_log_entry:
                        volt_value = volt_log_entry.replace("VOLT:", "")
                        ### ==========================
                        # ソーラーパネル導入により100%で正常稼働するパターンが出てきたため100%を記録する
                        # TODO: 今後のデバイス側の回路変更により復活する可能性あり
                        # if volt_value != "100": # 100%の時は初回起動またはデバッグ時のため記録しない
                        ### ==========================
                        try:
                            voltage = float(volt_value)
                            point = (
                                Point("data").tag("mac_address", sender_mac).field("voltage", float(voltage))
                            )
                        except ValueError:
                            logger.warning(f"Invalid VOLT value from {sender_mac}: {volt_value}")
                    else:
                        logger.warning(f"VOLT not found in HASH payload from {sender_mac}: {payload_str}")

                    # 温度情報を抽出
                    temperature = None
                    if "TEMP:" in temp_value:
                        temp_value = temp_value.replace("TEMP:", "")
                        if "-999" not in temp_value:
                            try:
                                temperature = float(temp_value)
                                if point is None:
                                    point = Point("data").tag("mac_address", sender_mac)
                                point.field("temperature", float(temperature))
                            except ValueError:
                                logger.warning(f"Invalid TEMP value from {sender_mac}: {temp_value}")
                    else:
                        logger.warning(f"TEMP not found in HASH payload from {sender_mac}: {payload_str}")

                    if point is not None:
                        try:
                            logger.info(f"Writing data to InfluxDB for {sender_mac}: {point}")
                            write_api.write(bucket=config.INFLUXDB_BUCKET, org=config.INFLUXDB_ORG, record=point) 
                        except Exception as e:
                            logger.error(f"Error writing to influxDB: {e}")
                    else:
                        logger.warning(f"No valid data to write for {sender_mac} in HASH frame.")

                elif frame_type == FRAME_TYPE_EOF:
                    frame_type_str = "EOF"
                    if sender_mac in image_buffers:
                        logger.info(f"EOF frame received for {sender_mac}. Assembling image ({len(image_buffers[sender_mac])} bytes).")
                        asyncio.create_task(save_image(sender_mac, bytes(image_buffers[sender_mac])))
                        del image_buffers[sender_mac]
                        if sender_mac in last_receive_time:
                            del last_receive_time[sender_mac]
                    else:
                        logger.warning(f"EOF for {sender_mac} but no buffer found.")
                
                elif frame_type == FRAME_TYPE_DATA:
                    frame_type_str = "DATA"
                    if sender_mac not in image_buffers:
                        image_buffers[sender_mac] = bytearray()
                        logger.info(f"Started receiving new image data from {sender_mac}")
                    image_buffers[sender_mac].extend(chunk_data)
                    last_receive_time[sender_mac] = time.monotonic()
                else:
                    logger.warning(f"Unknown frame type {frame_type} from {sender_mac}")

                if DEBUG_FRAME_PARSING:
                    logger.debug(f"Processed {frame_type_str} frame (seq={seq_num}) from {sender_mac}, {data_len} bytes")

                # フレームを処理したのでバッファから削除
                self.buffer = self.buffer[frame_end_index:]
            else:
                logger.warning(
                    f"Invalid end marker for {sender_mac} (got {footer.hex()}, expected {END_MARKER.hex()}). Discarding frame."
                )
                # 同期回復処理
                next_start = self.buffer.find(START_MARKER, 1)
                if next_start != -1:
                    self.buffer = self.buffer[next_start:]
                else:
                    self.buffer.clear()
                self.frame_start_time = None

        # return processed_frame # この関数の戻り値は現在使われていない

    def connection_lost(self, exc):
        log_prefix = f"connection_lost ({id(self)}):"  # Add instance ID for clarity
        if exc:
            logger.error(f"{log_prefix} Serial port connection lost: {exc}")
        else:
            logger.info(f"{log_prefix} Serial port connection closed normally.")
        self.transport = None

        # <<<--- [修正3] Use the future passed during __init__ ---
        # Check if the future exists and is not already done
        # logger.debug(f"{log_prefix} Future object ID = {id(self.connection_lost_future)}")
        if self.connection_lost_future and not self.connection_lost_future.done():
            logger.info(
                f"{log_prefix} Setting connection_lost_future result/exception."
            )
            if exc:
                try:
                    self.connection_lost_future.set_exception(exc)
                except asyncio.InvalidStateError:
                    logger.warning(
                        f"{log_prefix} Future was already set/cancelled when trying to set exception."
                    )
            else:
                try:
                    self.connection_lost_future.set_result(True)
                except asyncio.InvalidStateError:
                    logger.warning(
                        f"{log_prefix} Future was already set/cancelled when trying to set result."
                    )
        else:
            state = (
                "None"
                if not self.connection_lost_future
                else (
                    "Done"
                    if self.connection_lost_future.done()
                    else "Exists but not done?"
                )
            )
            logger.warning(
                f"{log_prefix} connection_lost called but future state is: {state}."
            )


# --- Timeout Checker Task ---
async def check_timeouts() -> None:
    """Periodically check for timed out image buffers."""
    while True:
        try:
            await asyncio.sleep(config.IMAGE_TIMEOUT)
            current_time = time.monotonic()
            timed_out_macs = [
                mac
                for mac, last_time in list(image_receiver.last_receive_time.items())
                if current_time - last_time > config.IMAGE_TIMEOUT
            ]
            for mac in timed_out_macs:
                logger.warning(
                    f"Timeout waiting for data from {mac}. Discarding buffer ({len(image_receiver.image_buffers.get(mac, b''))} bytes)."
                )
                image_receiver._cleanup_buffer(mac)
                
            # メモリ使用量チェック
            image_receiver.check_memory_usage()
            
        except asyncio.CancelledError:
            logger.info("Timeout checker task cancelled.")
            break
        except Exception as e:
            logger.exception(f"Error in timeout checker: {e}")


# --- Main Application Logic ---
async def main(port: str, baud: int) -> None:
    """Main asynchronous function."""
    ensure_dir_exists()
    logger.info("Starting Async USB CDC Image Receiver")
    logger.info(f"Images will be saved to: {os.path.abspath(config.IMAGE_DIR)}")

    loop = asyncio.get_running_loop()
    timeout_task = loop.create_task(check_timeouts())

    while True:  # Reconnection loop
        transport = None
        active_protocol = None
        # <<<--- [修正4] Create the Future in the main loop ---
        connection_lost_future = loop.create_future()
        # logger.debug(f"main loop: Created Future object ID = {id(connection_lost_future)}")

        try:
            logger.info(f"Attempting to connect to {port} at {baud} baud...")

            # <<<--- [修正5] Pass the created Future via the factory ---
            # The lambda creates a protocol instance and passes the future to its __init__
            def protocol_factory():
                # Create a new instance of SerialProtocol with the future
                return SerialProtocol(connection_lost_future)

            # serial_asyncio creates the protocol instance using the factory
            transport, active_protocol = await serial_asyncio.create_serial_connection(
                loop, protocol_factory, port, baudrate=baud
            )
            logger.info("Connection established.")
            # active_protocol should now hold the instance created by the factory

            # <<<--- [修正6] No need to retrieve the future here, just await the one we created ---
            logger.info("Monitoring connection (awaiting future)...")
            await connection_lost_future
            # Execution continues here after connection_lost sets the future result/exception
            logger.info("Connection lost signaled (future completed).")

        except serial.SerialException as e:
            logger.error(f"Serial connection error: {e}")
            # If connection failed, the future might not be set by connection_lost
            # Set it here to prevent the loop from waiting indefinitely on await sleep(5)
            if not connection_lost_future.done():
                logger.warning(
                    "Setting future exception due to SerialException during connection."
                )
                connection_lost_future.set_exception(e)
        except asyncio.CancelledError:
            logger.info("Main task cancelled during connection/monitoring.")
            # Ensure future is cancelled if await was interrupted
            if connection_lost_future and not connection_lost_future.done():
                connection_lost_future.cancel("Main task cancelled")
            break  # Exit the while loop
        except Exception as e:
            logger.exception(f"Error during connection or monitoring: {e}")
            # Ensure the future is set if an unexpected error occurs,
            # otherwise the loop might hang.
            if connection_lost_future and not connection_lost_future.done():
                try:
                    logger.warning(
                        f"Setting future exception due to unexpected error: {e}"
                    )
                    connection_lost_future.set_exception(e)
                except asyncio.InvalidStateError:
                    pass  # Future was already done/cancelled
        finally:
            # Close transport if it exists and is not already closing
            if transport and not transport.is_closing():
                logger.info("Closing transport in finally block.")
                transport.close()
            # Clear references for the next iteration
            transport = None
            # active_protocol = None

        # Check loop status before sleeping
        if not loop.is_running():
            logger.warning("Event loop is not running. Exiting reconnection loop.")
            break

        # Wait before retrying connection
        logger.info(f"Waiting {5} seconds before retrying connection...")
        try:
            # Log if the previous connection ended with an error
            if connection_lost_future.done() and connection_lost_future.exception():
                logger.info(
                    f"Previous connection ended with error: {connection_lost_future.exception()}"
                )
            await asyncio.sleep(5)
        except asyncio.CancelledError:
            logger.info("Retry delay cancelled. Exiting reconnection loop.")
            break  # Exit the while loop

    # Cleanup
    logger.info("Shutting down timeout task...")
    timeout_task.cancel()
    try:
        await timeout_task
    except asyncio.CancelledError:
        pass  # Expected cancellation
    logger.info("Application finished.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Async receive images via USB CDC.")
    parser.add_argument(
        "-p",
        "--port",
        default=config.SERIAL_PORT,
        help=f"Serial port (default: {config.SERIAL_PORT})",
    )
    parser.add_argument(
        "-b",
        "--baud",
        type=int,
        default=config.BAUD_RATE,
        help=f"Baud rate (default: {config.BAUD_RATE})",
    )
    args = parser.parse_args()

    try:
        asyncio.run(main(args.port, args.baud))
    except KeyboardInterrupt:
        logger.info("Exiting due to KeyboardInterrupt.")
