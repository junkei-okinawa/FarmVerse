"""Serial protocol handler for frame processing."""

import asyncio
import logging
import time
from typing import Dict

from .constants import (
    START_MARKER, END_MARKER, FRAME_TYPE_HASH, FRAME_TYPE_DATA, FRAME_TYPE_EOF,
    MAC_ADDRESS_LENGTH, FRAME_TYPE_LENGTH, SEQUENCE_NUM_LENGTH, LENGTH_FIELD_BYTES,
    CHECKSUM_LENGTH
)
from .frame_parser import FrameParser

# 修正: 絶対インポートまたは動的インポートを使用
import sys
import os
sys.path.append(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from config import config
from processors import save_image, determine_sleep_duration, format_sleep_command_to_gateway
from processors.voltage_processor import VoltageDataProcessor
from storage import influx_client
from utils.data_parser import DataParser

logger = logging.getLogger(__name__)


class SerialProtocol(asyncio.Protocol):
    """Asyncio protocol to handle serial data."""

    def __init__(self, connection_lost_future: asyncio.Future, image_buffers: Dict, 
                 last_receive_time: Dict, stats: Dict):
        super().__init__()
        self.buffer = bytearray()
        self.transport = None
        self.connection_lost_future = connection_lost_future
        self.frame_start_time = None  # フレーム受信開始時間
        
        # 画像バッファの参照
        self.image_buffers = image_buffers
        self.last_receive_time = last_receive_time
        self.stats = stats
        
        # 電圧プロセッサー
        self.voltage_processor = VoltageDataProcessor()
        
        # 電圧情報を一時保存（EOFフレーム時のスリープコマンド送信用）
        self.voltage_cache = {}  # {sender_mac: voltage}
        
        logger.info("Serial Protocol initialized.")

    def connection_made(self, transport):
        self.transport = transport
        try:
            transport.serial.dtr = True
            logger.info(f"Serial port {transport.serial.port} opened, DTR set.")
        except IOError as e:
            logger.warning(f"Could not set DTR on {transport.serial.port}: {e}")

    def data_received(self, data):
        """Called when data is received from the serial port."""
        # 受信データの一部をデバッグ出力（ESP-NOWゲートウェイからの変換確認用）
        if config.DEBUG_FRAME_PARSING and len(data) < 50:  # 短いデータのみ
            logger.debug(f"Raw serial data received: {data.hex()} ('{data.decode('ascii', errors='ignore')}')")
        
        self.buffer.extend(data)
        self.process_buffer()  # Process the buffer immediately

    def process_buffer(self):
        """Process the buffer to find and handle complete frames with enhanced frame format."""
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
                
                # フレーム受信詳細をデバッグ出力
                if config.DEBUG_FRAME_PARSING:
                    logger.debug(f"Frame header: MAC={sender_mac}, Type={frame_type}, Seq={seq_num}, DataLen={data_len}")
                
                # MACアドレス長の検証
                header_start = len(START_MARKER)
                mac_bytes = self.buffer[header_start : header_start + MAC_ADDRESS_LENGTH]
                FrameParser.validate_frame_data(data_len, mac_bytes, config.MAX_DATA_LEN)
                
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
                if config.DEBUG_FRAME_PARSING:
                    logger.debug(f"Need more data for full frame. Expected: {frame_end_index}, Have: {len(self.buffer)}")
                break  # Need more data for full frame

            # データ部分の位置を計算
            data_start_index = len(START_MARKER) + MAC_ADDRESS_LENGTH + FRAME_TYPE_LENGTH + SEQUENCE_NUM_LENGTH + LENGTH_FIELD_BYTES
            chunk_data = self.buffer[data_start_index : data_start_index + data_len]
            
            # チェックサム部分の位置
            checksum_start = data_start_index + data_len
            
            # エンドマーカーの位置
            end_marker_start = checksum_start + CHECKSUM_LENGTH
            footer = self.buffer[end_marker_start:frame_end_index]

            # エンドマーカーを確認
            if footer == END_MARKER:
                self.frame_start_time = None  # 正常にフレームを処理したので時間計測リセット
                
                # フレームタイプに応じた処理
                frame_type_str = "UNKNOWN"
                if frame_type == FRAME_TYPE_HASH:
                    frame_type_str = "HASH"
                    self._process_hash_frame(sender_mac, chunk_data)
                    
                elif frame_type == FRAME_TYPE_EOF:
                    frame_type_str = "EOF"
                    logger.info(f"Processing EOF frame from {sender_mac} (seq={seq_num}, data_len={data_len})")
                    self._process_eof_frame(sender_mac)
                
                elif frame_type == FRAME_TYPE_DATA:
                    frame_type_str = "DATA"
                    self._process_data_frame(sender_mac, chunk_data)
                else:
                    logger.warning(f"Unknown frame type {frame_type} from {sender_mac} (seq={seq_num}, data_len={data_len}, data_preview={chunk_data[:20].hex() if chunk_data else 'empty'})")

                if config.DEBUG_FRAME_PARSING:
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

    def _process_hash_frame(self, sender_mac: str, chunk_data: bytes):
        """HASH フレームの処理"""
        try:
            payload_str = chunk_data[5:].decode('ascii')  # 'HASH:' をスキップ
        except UnicodeDecodeError:
            logger.warning(f"Could not decode HASH payload from {sender_mac}")
            return

        logger.info(f"Received HASH frame from {sender_mac}: {payload_str}")
        payload_split = payload_str.split(",")
        
        if len(payload_split) < 2:
            logger.warning(f"Invalid HASH payload format from {sender_mac}: {payload_str}")
            return

        volt_log_entry = payload_split[1]
        temp_value = payload_split[2] if len(payload_split) > 2 else ""

        # 電圧・温度情報を抽出
        voltage = DataParser.extract_voltage_with_validation(volt_log_entry, sender_mac)
        temperature = DataParser.extract_temperature_with_validation(temp_value, sender_mac)

        # 電圧情報をキャッシュに保存（EOFフレーム時のスリープコマンド送信用）
        self.voltage_cache[sender_mac] = voltage

        # InfluxDBに書き込み（非同期・エラー耐性付き）
        # デバイス検証案件のため、100%電圧も含めて全ての電圧データを記録
        try:
            influx_client.write_sensor_data(sender_mac, voltage, temperature)
            logger.info(f"Initiated InfluxDB write for {sender_mac}")
        except Exception as e:
            logger.error(f"Error initiating InfluxDB write for {sender_mac}: {e} (continuing with other operations)")

        # HASHフレーム受信時にスリープコマンドを送信（EOFフレーム問題の回避策）
        # EOFフレームが正常に受信されない問題があるため、HASHフレーム時点でスリープコマンドを送信
        if voltage is not None:
            logger.info(f"Sending sleep command after HASH frame for {sender_mac} (voltage: {voltage})")
            self._send_sleep_command(sender_mac, voltage)
        else:
            logger.warning(f"No voltage data available for sleep command for {sender_mac}")

    def _send_sleep_command(self, sender_mac: str, voltage: float):
        """スリープコマンドを送信"""
        sleep_duration_s = determine_sleep_duration(voltage)
        command_to_gateway = format_sleep_command_to_gateway(sender_mac, sleep_duration_s)
        command_bytes = command_to_gateway.encode('utf-8')

        if self.transport:
            try:
                self.transport.write(command_bytes)
                logger.info(f"Sent sleep command to gateway for {sender_mac}: {command_to_gateway.strip()}")
            except Exception as e:
                logger.error(f"Error writing sleep command to serial for {sender_mac}: {e}")
        else:
            logger.warning(f"No transport available to send sleep command for {sender_mac}")

    def _process_eof_frame(self, sender_mac: str):
        """EOF フレームの処理"""
        if sender_mac in self.image_buffers:
            logger.info(f"EOF frame received for {sender_mac}. Assembling image ({len(self.image_buffers[sender_mac])} bytes).")
            asyncio.create_task(save_image(sender_mac, bytes(self.image_buffers[sender_mac]), self.stats))
            del self.image_buffers[sender_mac]
            if sender_mac in self.last_receive_time:
                del self.last_receive_time[sender_mac]
        else:
            logger.warning(f"EOF for {sender_mac} but no buffer found.")
        
        # EOFフレーム受信時にスリープコマンドを送信
        # 画像送信完了後にUnit Camがスリープコマンドを待機するため
        if sender_mac in self.voltage_cache:
            voltage = self.voltage_cache[sender_mac]
            if isinstance(voltage, float):
                logger.info(f"Sending sleep command after EOF for {sender_mac} (voltage: {voltage})")
                self._send_sleep_command(sender_mac, voltage)
                # キャッシュから削除
                del self.voltage_cache[sender_mac]
            else:
                logger.warning(f"Invalid voltage value for {sender_mac}: {voltage}. Cannot send sleep command.")
        else:
            logger.warning(f"No voltage cache found for {sender_mac}, cannot send sleep command")

    def _process_data_frame(self, sender_mac: str, chunk_data: bytes):
        """DATA フレームの処理"""
        if sender_mac not in self.image_buffers:
            self.image_buffers[sender_mac] = bytearray()
            logger.info(f"Started receiving new image data from {sender_mac}")
        self.image_buffers[sender_mac].extend(chunk_data)
        self.last_receive_time[sender_mac] = time.monotonic()

    def connection_lost(self, exc):
        log_prefix = f"connection_lost ({id(self)}):"
        if exc:
            logger.error(f"{log_prefix} Serial port connection lost: {exc}")
        else:
            logger.info(f"{log_prefix} Serial port connection closed normally.")
        self.transport = None

        if self.connection_lost_future and not self.connection_lost_future.done():
            logger.info(f"{log_prefix} Setting connection_lost_future result/exception.")
            if exc:
                try:
                    self.connection_lost_future.set_exception(exc)
                except asyncio.InvalidStateError:
                    logger.warning(f"{log_prefix} Future was already set/cancelled when trying to set exception.")
            else:
                try:
                    self.connection_lost_future.set_result(True)
                except asyncio.InvalidStateError:
                    logger.warning(f"{log_prefix} Future was already set/cancelled when trying to set result.")
        else:
            state = (
                "None" if not self.connection_lost_future
                else ("Done" if self.connection_lost_future.done() else "Exists but not done?")
            )
            logger.warning(f"{log_prefix} connection_lost called but future state is: {state}.")
