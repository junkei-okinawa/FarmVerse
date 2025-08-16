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
from .frame_parser import FrameParser, FrameSyncError

# 修正: 絶対インポートまたは動的インポートを使用
import sys
import os
sys.path.append(os.path.dirname(os.path.abspath(__file__)))

from config import config
from processors import save_image, determine_sleep_duration, format_sleep_command_to_gateway
from processors.voltage_processor import VoltageDataProcessor
from storage import influx_client
from utils.data_parser import DataParser

logger = logging.getLogger(__name__)


class SerialHandler:
    def __init__(self):
        self.image_buffers: Dict[str, bytearray] = {}
        self.voltage_processor = VoltageDataProcessor()
        self.data_parser = DataParser()
        self.frame_parser = FrameParser()
        self.last_discard_log_time = 0  # レート制限用


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
        
        # シーケンス番号追跡（データフレームの順序確認用）
        self.sequence_tracking = {}  # {sender_mac: last_sequence_number}
        
        # 最後のデータフレーム受信時間（画像受信中の判定用）
        self.last_data_frame_time = {}  # {sender_mac: timestamp}
        
        logger.info("Serial Protocol initialized.")
        
    def _has_running_event_loop(self) -> bool:
        """イベントループが実行中かチェック"""
        try:
            asyncio.get_running_loop()
            return True
        except RuntimeError:
            return False

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
        if config.DEBUG_FRAME_PARSING:
            if len(data) < 50:  # 短いデータのみ詳細出力
                logger.debug(f"Raw serial data received: {data.hex()} ('{data.decode('ascii', errors='ignore')}')")
            else:
                logger.debug(f"Raw serial data received: {len(data)} bytes, start: {data[:20].hex()}")
        
        self.buffer.extend(data)
        self.process_buffer()  # Process the buffer immediately

    def process_buffer(self):
        """Process the buffer to find and handle complete frames with enhanced frame format."""
        # デバッグ: バッファ内にEOFマーカーが含まれているかチェック
        if config.DEBUG_FRAME_PARSING and b'EOF' in self.buffer:
            eof_index = self.buffer.find(b'EOF')
            logger.warning(f"Raw EOF marker found at buffer position {eof_index}: {self.buffer[max(0, eof_index-10):eof_index+20].hex()}")
        
        # 暫定対策: 生のEOFマーカーを検出して処理
        self._handle_raw_eof_markers()
        
        # バッファが空になった場合はここで終了
        if len(self.buffer) == 0:
            return
        
        while True:  # Process all complete frames in the buffer
            # フレームレベルのタイムアウトチェック
            # 画像データ受信中は長めのタイムアウト（30秒）、それ以外は短め（2秒）
            has_active_image_transfer = bool(self.image_buffers)
            timeout_duration = 30.0 if has_active_image_transfer else 2.0
            
            # 画像データ受信中の場合、最後のデータフレーム受信時間から判定
            should_timeout = False
            if self.frame_start_time:
                frame_elapsed = time.monotonic() - self.frame_start_time
                if has_active_image_transfer:
                    # 最後のデータフレーム受信から判定
                    latest_data_time = max(self.last_data_frame_time.values()) if self.last_data_frame_time else 0
                    data_elapsed = time.monotonic() - latest_data_time if latest_data_time > 0 else float('inf')
                    should_timeout = data_elapsed > timeout_duration
                    if should_timeout:
                        logger.warning(f"Data frame timeout: {data_elapsed:.1f}s since last data frame")
                else:
                    should_timeout = frame_elapsed > timeout_duration
            
            if should_timeout:
                logger.warning(
                    f"Frame timeout detected after {timeout_duration}s. Discarding partial frame data."
                )
                
                # 画像データ受信中の場合、より慎重に処理
                if has_active_image_transfer:
                    logger.info(f"Active image transfer detected for {list(self.image_buffers.keys())}, preserving data frames")
                    # データフレームの内容は保持し、不完全なフレーム構造のみを削除
                    start_index_after_timeout = self.buffer.find(START_MARKER, 1)
                    if start_index_after_timeout != -1:
                        logger.warning(f"Removing incomplete frame: {start_index_after_timeout} bytes")
                        self.buffer = self.buffer[start_index_after_timeout:]
                    else:
                        # 最初のSTART_MARKERより前の不完全なデータのみを削除
                        first_start_marker = self.buffer.find(START_MARKER)
                        if first_start_marker > 0:
                            logger.warning(f"Removing {first_start_marker} bytes before first START_MARKER")
                            self.buffer = self.buffer[first_start_marker:]
                        elif first_start_marker == -1:
                            logger.warning("No START_MARKER found, but preserving image buffers")
                            self.buffer.clear()
                        else:
                            logger.info("Buffer starts with START_MARKER, keeping all data")
                else:
                    # 画像データ受信中でない場合は通常のタイムアウト処理
                    start_index_after_timeout = self.buffer.find(START_MARKER, 1)
                    if start_index_after_timeout != -1:
                        logger.warning(f"Discarding {start_index_after_timeout} bytes due to frame timeout.")
                        self.buffer = self.buffer[start_index_after_timeout:]
                    else:
                        first_start_marker = self.buffer.find(START_MARKER)
                        if first_start_marker > 0:
                            logger.warning(f"Removing {first_start_marker} bytes before first START_MARKER")
                            self.buffer = self.buffer[first_start_marker:]
                        elif first_start_marker == -1:
                            logger.warning("No START_MARKER found after frame timeout. Clearing buffer.")
                            self.buffer.clear()
                        else:
                            logger.info("Buffer already starts with START_MARKER, keeping existing data.")
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
                if config.DEBUG_FRAME_PARSING:
                    # 破棄されたデータの詳細解析
                    ascii_data = discarded_data.decode('ascii', errors='ignore')
                    logger.debug(f"Discarded data ASCII: '{ascii_data}'")
                    if b'EOF' in discarded_data:
                        logger.warning(f"!!! EOF marker found in discarded data: {discarded_data.hex()}")
                
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
                    frame_type_name = {
                        FRAME_TYPE_DATA: "DATA",
                        FRAME_TYPE_HASH: "HASH", 
                        FRAME_TYPE_EOF: "EOF"
                    }.get(frame_type, f"UNKNOWN({frame_type})")
                    logger.debug(f"Parsed frame header: {frame_type_name} from {sender_mac}, seq: {seq_num}, data_len: {data_len}")
                
                # ヘッダー長の計算
                header_len = len(START_MARKER) + MAC_ADDRESS_LENGTH + FRAME_TYPE_LENGTH + SEQUENCE_NUM_LENGTH + LENGTH_FIELD_BYTES
                total_frame_len = header_len + data_len + CHECKSUM_LENGTH + len(END_MARKER)
                
                if config.DEBUG_FRAME_PARSING:
                    logger.debug(f"Frame calculation: header_len={header_len}, data_len={data_len}, checksum_len={CHECKSUM_LENGTH}, end_marker_len={len(END_MARKER)}, total_frame_len={total_frame_len}, buffer_len={len(self.buffer)}")
                
                # MACアドレス長の検証
                header_start = len(START_MARKER)
                mac_bytes = self.buffer[header_start : header_start + MAC_ADDRESS_LENGTH]
                FrameParser.validate_frame_data(data_len, mac_bytes, config.MAX_DATA_LEN)
                
            except FrameSyncError as e:
                # フレーム同期エラーの場合はログレベルを調整
                if config.SUPPRESS_SYNC_ERRORS:
                    logger.debug(f"Frame sync adjustment: {e}")
                else:
                    logger.warning(f"Frame sync issue: {e}")
            except (ValueError, IndexError) as e:
                logger.error(f"Frame decode error: {e}")
                    
                if config.DEBUG_FRAME_PARSING:
                    # デバッグ情報を出力
                    buffer_preview_size = min(len(self.buffer), 64)
                    logger.debug(f"Buffer content around error: {self.buffer[:buffer_preview_size].hex()}")
                
                # フレーム境界同期の修正：より保守的な同期回復
                # 1. 現在のSTART_MARKERから最小限のバイトを削除
                skip_bytes = min(4, len(self.buffer))  # 4バイトまたはバッファサイズの小さい方
                logger.debug(f"Frame decode failed, skipping {skip_bytes} bytes for boundary realignment")
                self.buffer = self.buffer[skip_bytes:]
                
                # 2. 次のSTART_MARKERを探す
                next_start = self.buffer.find(START_MARKER)
                if next_start != -1 and next_start > 0:
                    logger.debug(f"Found next START_MARKER at position {next_start}, discarding {next_start} bytes")
                    self.buffer = self.buffer[next_start:]
                elif len(self.buffer) > 1000:  # バッファが大きすぎる場合のみクリア
                    logger.debug("Buffer too large without valid frame, clearing")
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
            
            if config.DEBUG_FRAME_PARSING:
                logger.debug(f"Data extraction: start_index={data_start_index}, data_len={data_len}")
                logger.debug(f"Raw chunk_data (first 20 bytes): {chunk_data[:20].hex()}")
                logger.debug(f"Expected JPEG header check: {chunk_data[:2].hex() if len(chunk_data) >= 2 else 'insufficient data'}")
            
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
                    self._process_data_frame(sender_mac, chunk_data, seq_num)
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
                if config.DEBUG_FRAME_PARSING:
                    # フレーム全体のデバッグ情報を出力
                    logger.debug("Frame debug info:")
                    logger.debug(f"  Header: {self.buffer[:len(START_MARKER) + MAC_ADDRESS_LENGTH + FRAME_TYPE_LENGTH + SEQUENCE_NUM_LENGTH + LENGTH_FIELD_BYTES].hex()}")
                    logger.debug(f"  Data (first 20 bytes): {chunk_data[:20].hex() if chunk_data else 'empty'}")
                    logger.debug(f"  Checksum area: {self.buffer[checksum_start:checksum_start + CHECKSUM_LENGTH].hex()}")
                    logger.debug(f"  End marker area: {footer.hex()}")
                    logger.debug(f"  Expected end marker: {END_MARKER.hex()}")
                
                # より積極的な同期回復処理
                # 1. まず次のスタートマーカーを探す
                next_start = self.buffer.find(START_MARKER, 1)
                if next_start != -1:
                    discarded_bytes = next_start
                    logger.warning(f"Found next start marker at position {next_start}, discarding {discarded_bytes} bytes")
                    self.buffer = self.buffer[next_start:]
                else:
                    # 2. スタートマーカーがない場合、EOFマーカーがあるかチェック
                    eof_in_remaining = self.buffer.find(b'EOF', 1)
                    if eof_in_remaining != -1:
                        logger.warning(f"Found EOF marker at position {eof_in_remaining}, clearing buffer up to EOF")
                        # EOFマーカーを処理してからバッファをクリア
                        self._handle_raw_eof_markers()
                    else:
                        logger.warning("No recovery anchor found, clearing entire buffer")
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
        temp_log_entry = payload_split[2] if len(payload_split) > 2 else ""

        # 電圧・温度情報を抽出
        voltage = DataParser.extract_voltage_with_validation(volt_log_entry, sender_mac)
        temperature = DataParser.extract_temperature_with_validation(temp_log_entry, sender_mac)

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
            image_data = bytes(self.image_buffers[sender_mac])
            image_size = len(image_data)
            
            logger.info(f"EOF frame received for {sender_mac}. Assembling image ({image_size} bytes).")
            
            # テスト環境では画像検証をスキップ
            if not config.IS_TEST_ENV:
                # 画像データの基本検証
                if image_size < 1000:  # 1KB未満は明らかに不正
                    logger.error(f"Image data too small ({image_size} bytes), discarding")
                    self._cleanup_image_buffers(sender_mac)
                    # 画像保存をスキップしてもスリープコマンドは送信
                    self._send_sleep_command_after_eof(sender_mac)
                    return
                    
                # JPEGヘッダーの確認
                if not image_data.startswith(b'\xff\xd8'):
                    logger.error(f"Invalid JPEG header detected for {sender_mac}, discarding corrupted image")
                    self._cleanup_image_buffers(sender_mac)
                    # 画像保存をスキップしてもスリープコマンドは送信
                    self._send_sleep_command_after_eof(sender_mac)
                    return
                    
                # JPEGフッターの確認
                if not image_data.endswith(b'\xff\xd9'):
                    logger.warning(f"JPEG footer missing or corrupted, data ends with: {image_data[-10:].hex()}")
                    logger.warning("Attempting to save image anyway")
            else:
                logger.debug("Test environment detected, skipping image validation")
            
            # イベントループが実行中かチェックしてからタスクを作成
            if self._has_running_event_loop():
                try:
                    asyncio.create_task(save_image(sender_mac, image_data, self.stats))
                except Exception as e:
                    logger.error(f"Error creating save_image task for {sender_mac}: {e}")
            else:
                logger.warning(f"No event loop running, cannot create save_image task for {sender_mac}")
            
            self._cleanup_image_buffers(sender_mac)
        else:
            logger.warning(f"EOF for {sender_mac} but no buffer found.")
        
        # EOF処理完了後、画像保存の成功/失敗に関わらずスリープコマンドを送信
        self._send_sleep_command_after_eof(sender_mac)

    def _send_sleep_command_after_eof(self, sender_mac: str):
        """EOF処理完了後にスリープコマンドを送信"""
        # 電圧キャッシュからデータを取得
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

    def _process_data_frame(self, sender_mac: str, chunk_data: bytes, seq_num: int):
        """DATA フレームの処理"""
        if sender_mac not in self.image_buffers:
            self.image_buffers[sender_mac] = bytearray()
            self.sequence_tracking[sender_mac] = seq_num
            logger.info(f"Started receiving new image data from {sender_mac} (first_seq={seq_num})")
        
        # シーケンス番号の連続性をチェック
        if sender_mac in self.sequence_tracking:
            last_seq = self.sequence_tracking[sender_mac]
            if seq_num != last_seq + 1:
                logger.warning(f"Sequence gap detected for {sender_mac}: expected {last_seq + 1}, got {seq_num}")
        
        self.sequence_tracking[sender_mac] = seq_num
        
        current_size = len(self.image_buffers[sender_mac])
        self.image_buffers[sender_mac].extend(chunk_data)
        new_size = len(self.image_buffers[sender_mac])
        chunk_size = len(chunk_data)
        
        # 最初のチャンクでJPEGヘッダーを確認
        if current_size == 0 and len(chunk_data) > 0:
            if chunk_data.startswith(b'\xff\xd8'):
                logger.info(f"✓ Started receiving valid JPEG image from {sender_mac}")
            else:
                logger.warning(f"✗ First chunk missing JPEG header from {sender_mac}")
        
        # 定期的な進捗ログ（5KB毎）
        if new_size % 5000 < chunk_size:
            logger.debug(f"Image receiving progress for {sender_mac}: {new_size} bytes")
        
        # 受信データのタイムスタンプを更新
        current_time = time.monotonic()
        self.last_receive_time[sender_mac] = current_time
        self.last_data_frame_time[sender_mac] = current_time

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

    def _handle_raw_eof_markers(self):
        """暫定対策: フレーム化されていない生のEOFマーカーを検出・処理"""
        # 複数のEOFパターンを検索（改行文字付きも含む）
        eof_patterns = [b'---EOF---\r\n', b'EOF\r\n', b'EOF\n', b'EOF\r', b'EOF']
        
        for eof_pattern in eof_patterns:
            eof_index = self.buffer.find(eof_pattern)
            
            while eof_index != -1:
                # EOFマーカー周辺のデータを確認
                start_context = max(0, eof_index - 20)
                end_context = min(len(self.buffer), eof_index + 50)  # より広い範囲を確認
                context = self.buffer[start_context:end_context]
                
                # EOFパターンを改行文字をエスケープして表示
                eof_pattern_display = repr(eof_pattern.decode('ascii', errors='ignore'))
                logger.warning(f"Processing raw EOF marker {eof_pattern_display} at position {eof_index}")
                logger.debug(f"EOF context: {context.hex()} ('{context.decode('ascii', errors='ignore')}')")
                
                # 現在進行中の画像受信があるかチェック
                processed_eof = False
                for sender_mac in list(self.image_buffers.keys()):
                    if sender_mac in self.image_buffers and len(self.image_buffers[sender_mac]) > 0:
                        image_size = len(self.image_buffers[sender_mac])
                        logger.info(f"Raw EOF marker detected for {sender_mac} with {image_size} bytes")
                        self._process_eof_frame(sender_mac)
                        processed_eof = True
                        break
                
                # 画像バッファがない場合でも、電圧キャッシュがあればEOF処理を実行
                # 電圧0%の場合、画像データは送信されないがEOFマーカーは送信される
                if not processed_eof and self.voltage_cache:
                    # 電圧キャッシュから最新のsender_macを特定
                    for sender_mac in list(self.voltage_cache.keys()):
                        logger.info(f"Raw EOF marker detected for {sender_mac} (no image data, voltage-only)")
                        # 1秒待機
                        time.sleep(3)
                        self._process_eof_frame(sender_mac)  # 画像バッファがなくてもEOF処理を実行
                        processed_eof = True
                        break
                
                # EOFマーカー検出後、バッファを積極的にクリーンアップ
                if processed_eof:
                    # EOFマーカーから後続のデータを全て削除
                    # END_MARKERまでの間にあるノイズデータを除去
                    end_marker_pos = self.buffer.find(END_MARKER, eof_index)
                    if end_marker_pos != -1:
                        removal_end = end_marker_pos + len(END_MARKER)
                    else:
                        removal_end = min(len(self.buffer), eof_index + 50)
                    
                    removal_start = max(0, eof_index - 5)
                    self.buffer = self.buffer[:removal_start] + self.buffer[removal_end:]
                    logger.debug("EOF marker processed and buffer cleaned")
                else:
                    # 画像バッファがない場合は、EOFマーカー周辺のみ削除
                    removal_start = max(0, eof_index - 10)
                    removal_end = min(len(self.buffer), eof_index + len(eof_pattern) + 10)
                    self.buffer = self.buffer[:removal_start] + self.buffer[removal_end:]
                
                # バッファが大幅に変更されたので、再度検索
                eof_index = self.buffer.find(eof_pattern)                # 最終的なバッファクリーンアップ
        self._cleanup_invalid_frame_data()
        
        # EOFマーカー処理後の追加クリーンアップ
        # 残ったデータが有効なフレーム構造を持っているかチェック
        self._validate_remaining_buffer()

    def _cleanup_invalid_frame_data(self):
        """無効なフレームデータをバッファから除去"""
        # START_MARKERがない小さなノイズデータを除去
        if len(self.buffer) > 0 and len(self.buffer) < len(START_MARKER):
            # バッファが小さすぎて有効なフレームになり得ない場合
            if START_MARKER not in self.buffer:
                logger.debug(f"Clearing small noise buffer: {self.buffer.hex()}")
                self.buffer.clear()
                return
        
        # バッファ内の不正なEND_MARKERパターンを検出して除去
        # END_MARKERが単独で存在する場合（正常なフレーム構造外）
        end_marker_positions = []
        search_start = 0
        while True:
            pos = self.buffer.find(END_MARKER, search_start)
            if pos == -1:
                break
            end_marker_positions.append(pos)
            search_start = pos + 1
        
        # 複数のEND_MARKERがある場合、最初のSTART_MARKER以降の不正なものを除去
        start_marker_pos = self.buffer.find(START_MARKER)
        if start_marker_pos != -1 and end_marker_positions:
            for end_pos in end_marker_positions:
                # START_MARKERより前にあるEND_MARKERは不正
                if end_pos < start_marker_pos:
                    logger.debug(f"Removing invalid END_MARKER at position {end_pos} (before START_MARKER)")
                    # END_MARKERとその周辺を削除
                    removal_start = max(0, end_pos - 5)
                    removal_end = min(len(self.buffer), end_pos + len(END_MARKER) + 5)
                    self.buffer = self.buffer[:removal_start] + self.buffer[removal_end:]
                    logger.debug("Removed invalid END_MARKER and surrounding data")
                    break  # バッファが変更されたので、再度処理が必要
        
        # 複数のスタートマーカーがある場合、最初の一つ以外を除去
        start_positions = []
        search_start = 0
        while True:
            pos = self.buffer.find(START_MARKER, search_start)
            if pos == -1:
                break
            start_positions.append(pos)
            search_start = pos + 1
        
        if len(start_positions) > 1:
            # 最初のスタートマーカー以外を除去
            logger.debug(f"Found multiple start markers at positions: {start_positions}")
            logger.debug("Keeping only the first start marker")
            self.buffer = self.buffer[:start_positions[1]]

    def _validate_remaining_buffer(self):
        """残ったバッファデータが有効なフレーム構造を持っているかチェック"""
        if len(self.buffer) == 0:
            return
            
        # START_MARKERから始まっていない場合、最初のSTART_MARKERまでを削除
        start_marker_pos = self.buffer.find(START_MARKER)
        if start_marker_pos == -1:
            # START_MARKERがない場合、バッファ全体をクリア
            logger.debug(f"No START_MARKER found in remaining buffer, clearing {len(self.buffer)} bytes")
            self.buffer.clear()
            return
        elif start_marker_pos > 0:
            # START_MARKERより前のデータを削除
            logger.debug(f"Removing {start_marker_pos} bytes before START_MARKER")
            self.buffer = self.buffer[start_marker_pos:]
        
        # フレームヘッダーが完全に受信されているかチェック
        header_size = len(START_MARKER) + MAC_ADDRESS_LENGTH + FRAME_TYPE_LENGTH + SEQUENCE_NUM_LENGTH + LENGTH_FIELD_BYTES
        if len(self.buffer) >= header_size:
            try:
                # ヘッダーを解析してデータ長をチェック
                from .frame_parser import FrameParser
                sender_mac, frame_type, seq_num, data_len = FrameParser.parse_header(self.buffer, 0)
                
                # データ長が異常に大きい場合、バッファをクリア
                max_reasonable_data_len = 100000  # 100KB
                
                # 特定の問題のあるデータ長パターンを検出
                problematic_data_lengths = [1529887539, 1833508904]  # 実際に観測された異常値
                
                if data_len > max_reasonable_data_len or data_len in problematic_data_lengths:
                    logger.warning(f"Invalid data length {data_len} detected in remaining buffer, clearing")
                    
                    # 問題のあるデータ長の詳細解析
                    if data_len in problematic_data_lengths:
                        logger.warning(f"Known problematic data length {data_len} detected - this indicates corruption")
                        # ヘッダー部分を詳細にログ出力
                        header_data = self.buffer[:header_size]
                        logger.debug(f"Corrupted header data: {header_data.hex()}")
                    
                    self.buffer.clear()
                    return
                    
                logger.debug(f"Validated remaining buffer: MAC={sender_mac}, Type={frame_type}, DataLen={data_len}")
                
            except FrameSyncError as e:
                # フレーム同期エラーの場合はログレベルを調整
                if config.SUPPRESS_SYNC_ERRORS:
                    logger.debug(f"Frame sync adjustment in remaining buffer: {e}")
                else:
                    logger.warning(f"Frame sync issue in remaining buffer: {e}")
                # バッファの先頭部分をデバッグ出力
                debug_data = self.buffer[:min(50, len(self.buffer))]
                logger.debug(f"Invalid buffer data: {debug_data.hex()}")
                self.buffer.clear()
            except (ValueError, IndexError) as e:
                logger.warning(f"Invalid frame header in remaining buffer: {e}")
                # バッファの先頭部分をデバッグ出力
                debug_data = self.buffer[:min(50, len(self.buffer))]
                logger.debug(f"Invalid buffer data: {debug_data.hex()}")
                self.buffer.clear()

    def _cleanup_image_buffers(self, sender_mac: str):
        """指定されたsender_macの画像関連バッファをクリーンアップ"""
        if sender_mac in self.image_buffers:
            del self.image_buffers[sender_mac]
        if sender_mac in self.last_receive_time:
            del self.last_receive_time[sender_mac]
        if sender_mac in self.sequence_tracking:
            del self.sequence_tracking[sender_mac]
        if sender_mac in self.last_data_frame_time:
            del self.last_data_frame_time[sender_mac]
