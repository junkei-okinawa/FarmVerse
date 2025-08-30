"""
Streaming Serial Protocol Handler

This module implements a streaming-aware serial protocol handler that
integrates with StreamingImageProcessor for real-time image processing.
"""

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

# 絶対インポートを使用
import sys
import os
sys.path.append(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from processors.streaming_image_processor import StreamingImageProcessor
from processors.voltage_processor import VoltageDataProcessor
from processors.sleep_controller import determine_sleep_duration, format_sleep_command_to_gateway
from storage import influx_client
from utils.data_parser import DataParser
from config import config

logger = logging.getLogger(__name__)


class StreamingSerialProtocol(asyncio.Protocol):
    """
    Streaming対応シリアルプロトコル処理クラス
    
    従来のバッファ蓄積方式ではなく、チャンク受信と同時に
    StreamingImageProcessorで処理を行います。
    """
    
    def __init__(self, connection_lost_future: asyncio.Future, stats: Dict):
        super().__init__()
        self.buffer = bytearray()
        self.transport = None
        self.connection_lost_future = connection_lost_future
        self.frame_start_time = None
        
        # ストリーミング画像プロセッサー
        self.streaming_processor = StreamingImageProcessor(max_concurrent_streams=5)
        
        # 統計情報（下位互換性のため保持）
        self.stats = stats
        
        # 電圧プロセッサー
        self.voltage_processor = VoltageDataProcessor()
        
        # 電圧情報キャッシュ（スリープコマンド送信用）
        self.voltage_cache = {}  # {sender_mac: voltage}
        
        # スリープコマンド送信履歴を追跡（重複送信防止用）
        self.sleep_command_sent = {}  # {sender_mac: timestamp}
        
        # 画像データの有無を記録（HASHフレーム時に設定、EOF時に参照）
        self.has_image_data_cache = {}  # {sender_mac: bool}
        
        # EOF処理済みフラグ（重複EOF処理を防止）
        self.eof_processed = {}  # {sender_mac: timestamp}
        
        # 最後のデータフレーム受信時間
        self.last_data_frame_time = {}  # {sender_mac: timestamp}
        
        # タイムアウトチェックタスク
        self.timeout_check_task = None
        
        logger.info("StreamingSerialProtocol initialized")
    
    def connection_made(self, transport):
        """接続確立時の処理"""
        self.transport = transport
        try:
            transport.serial.dtr = True
            logger.info(f"Serial port {transport.serial.port} opened, DTR set.")
        except IOError as e:
            logger.warning(f"Could not set DTR on {transport.serial.port}: {e}")
        
        # タイムアウトチェックタスクを開始
        if not self.timeout_check_task:
            self.timeout_check_task = asyncio.create_task(self._timeout_check_loop())
    
    def data_received(self, data):
        """データ受信時の処理"""
        if config.DEBUG_FRAME_PARSING:
            logger.debug(f"Received {len(data)} bytes: {data.hex() if len(data) < 100 else data[:50].hex() + '...'}")
            logger.debug(f"Buffer size before: {len(self.buffer)}, after: {len(self.buffer) + len(data)}")
        
        self.buffer.extend(data)
        asyncio.create_task(self._process_buffer_async())
    
    async def _process_buffer_async(self):
        """非同期バッファ処理"""
        try:
            await self._process_streaming_buffer()
        except Exception as e:
            logger.error(f"Error in async buffer processing: {e}")
    
    async def _process_streaming_buffer(self):
        """ストリーミング対応バッファ処理"""
        while True:
            # フレームタイムアウトチェック
            await self._check_frame_timeout()
            
            if config.DEBUG_FRAME_PARSING:
                logger.debug(f"Processing buffer, size: {len(self.buffer)}")
            
            # 開始マーカーを探す
            start_index = self.buffer.find(START_MARKER)
            if start_index == -1:
                if config.DEBUG_FRAME_PARSING and len(self.buffer) > 0:
                    logger.debug(f"No START_MARKER found in buffer of {len(self.buffer)} bytes")
                    if len(self.buffer) < 50:
                        logger.debug(f"Buffer content: {self.buffer.hex()}")
                if len(self.buffer) >= len(START_MARKER):
                    self.buffer = self.buffer[-(len(START_MARKER) - 1):]
                break
            
            if start_index > 0:
                discarded_data = self.buffer[:start_index]
                # SUPPRESS_DISCARD_LOGSの設定に従ってログレベルを調整
                if config.SUPPRESS_DISCARD_LOGS:
                    logger.debug(f"Discarding {start_index} bytes: {discarded_data.hex()}")
                else:
                    logger.warning(f"Discarding {start_index} bytes: {discarded_data.hex()}")
                self.buffer = self.buffer[start_index:]
                self.frame_start_time = time.monotonic()
                continue
            
            # フレーム受信開始時間を記録
            if self.frame_start_time is None:
                self.frame_start_time = time.monotonic()
            
            if config.DEBUG_FRAME_PARSING:
                logger.debug(f"Found START_MARKER at index 0, buffer size: {len(self.buffer)}")
            
            # ヘッダー解析に必要なデータ長チェック
            header_length = (len(START_MARKER) + MAC_ADDRESS_LENGTH + 
                           FRAME_TYPE_LENGTH + SEQUENCE_NUM_LENGTH + LENGTH_FIELD_BYTES)
            
            if len(self.buffer) < header_length:
                if config.DEBUG_FRAME_PARSING:
                    logger.debug(f"Insufficient data for header: {len(self.buffer)} < {header_length}")
                break
            
            try:
                # ヘッダーを解析
                sender_mac, frame_type, seq_num, data_len = FrameParser.parse_header(self.buffer, 0)
                
                if config.DEBUG_FRAME_PARSING:
                    frame_type_name = self._get_frame_type_name(frame_type)
                    logger.debug(f"Parsed frame header: {frame_type_name} from {sender_mac}, seq: {seq_num}, data_len: {data_len}")
                
                # フレーム検証
                header_start = len(START_MARKER)
                mac_bytes = self.buffer[header_start:header_start + MAC_ADDRESS_LENGTH]
                FrameParser.validate_frame_data(data_len, mac_bytes, config.MAX_DATA_LEN)
                
            except (ValueError, IndexError) as e:
                # FrameSyncErrorの場合はSUPPRESS_SYNC_ERRORSの設定に従う
                if "exceeds physical limit" in str(e):
                    if config.SUPPRESS_SYNC_ERRORS:
                        logger.debug(f"Frame decode error: {e}")
                    else:
                        logger.error(f"Frame decode error: {e}")
                else:
                    if config.SUPPRESS_SYNC_ERRORS:
                        logger.debug(f"Frame decode error: {e}")
                    else:
                        logger.error(f"Frame decode error: {e}")
                if config.DEBUG_FRAME_PARSING:
                    logger.debug(f"Buffer content around error: {self.buffer[:50].hex()}")
                await self._handle_frame_error()
                continue
            
            # フレーム全体の長さを計算
            frame_end_index = (header_length + data_len + 
                             CHECKSUM_LENGTH + len(END_MARKER))
            
            if config.DEBUG_FRAME_PARSING:
                logger.debug(f"Frame calculation: header_len={header_length}, data_len={data_len}, "
                           f"checksum_len={CHECKSUM_LENGTH}, end_marker_len={len(END_MARKER)}, "
                           f"total_frame_len={frame_end_index}, buffer_len={len(self.buffer)}")
            
            if len(self.buffer) < frame_end_index:
                if config.DEBUG_FRAME_PARSING:
                    logger.debug(f"Waiting for complete frame: {len(self.buffer)} < {frame_end_index}")
                break  # 完全なフレームを待つ
            
            # データ部分を抽出
            data_start_index = header_length
            chunk_data = self.buffer[data_start_index:data_start_index + data_len]
            
            # エンドマーカーを確認
            end_marker_start = data_start_index + data_len + CHECKSUM_LENGTH
            footer = self.buffer[end_marker_start:frame_end_index]
            
            if footer != END_MARKER:
                logger.warning(f"Invalid end marker for {sender_mac}, expected: {END_MARKER.hex()}, got: {footer.hex()}")
                if config.DEBUG_FRAME_PARSING:
                    logger.debug(f"Frame details - sender_mac: {sender_mac}, frame_type: {frame_type}, seq_num: {seq_num}, data_len: {data_len}")
                    logger.debug(f"Buffer dump around end marker (±20 bytes): {self.buffer[max(0, end_marker_start-20):end_marker_start+20].hex()}")
                await self._handle_frame_error()
                continue
            
            # フレームタイプ別処理
            await self._process_frame_by_type(
                sender_mac, frame_type, seq_num, chunk_data
            )
            
            # フレーム処理完了、バッファから削除
            self.buffer = self.buffer[frame_end_index:]
            self.frame_start_time = None
            
            if config.DEBUG_FRAME_PARSING:
                frame_type_name = self._get_frame_type_name(frame_type)
                logger.debug(f"Processed {frame_type_name} frame from {sender_mac} (seq: {seq_num}, data_len: {data_len})")
                if frame_type == FRAME_TYPE_EOF:
                    logger.info(f"✓ EOF frame successfully processed for {sender_mac}")
    
    async def _process_frame_by_type(self, sender_mac: str, frame_type: int, 
                                   seq_num: int, chunk_data: bytes):
        """フレームタイプ別処理"""
        if config.DEBUG_FRAME_PARSING:
            frame_type_name = self._get_frame_type_name(frame_type)
            logger.debug(f"Processing {frame_type_name} frame from {sender_mac} (seq: {seq_num}, data_len: {len(chunk_data)})")
        
        if frame_type == FRAME_TYPE_HASH:
            await self._process_streaming_hash_frame(sender_mac, chunk_data)
            
        elif frame_type == FRAME_TYPE_DATA:
            await self._process_streaming_data_frame(
                sender_mac, chunk_data, seq_num
            )
            
        elif frame_type == FRAME_TYPE_EOF:
            logger.info(f"Received EOF frame for {sender_mac}")
            await self._process_streaming_eof_frame(sender_mac)
            
        else:
            logger.warning(f"Unknown frame type {frame_type} from {sender_mac}")
    
    async def _process_streaming_hash_frame(self, sender_mac: str, chunk_data: bytes):
        """HASHフレーム処理（ストリーミング対応）"""
        try:
            payload_str = chunk_data[5:].decode('ascii')  # 'HASH:' をスキップ
        except UnicodeDecodeError:
            logger.warning(f"Could not decode HASH payload from {sender_mac}")
            return
        
        logger.info(f"Received HASH frame from {sender_mac}: {payload_str}")
        payload_split = payload_str.split(",")
        
        if len(payload_split) < 2:
            logger.warning(f"Invalid HASH payload format: {payload_str}")
            return
        
        hash_value = payload_split[0]
        volt_log_entry = payload_split[1]
        temp_log_entry = payload_split[2] if len(payload_split) > 2 else ""
        
        # 電圧・温度情報を抽出
        voltage = DataParser.extract_voltage_with_validation(volt_log_entry, sender_mac)
        temperature = DataParser.extract_temperature_with_validation(temp_log_entry, sender_mac)
        
        logger.info(f"Extracted voltage for {sender_mac}: {voltage}% from '{volt_log_entry}'")
        
        # 電圧情報をキャッシュ
        self.voltage_cache[sender_mac] = voltage
        
        # ダミーハッシュを検出して画像データの有無を判定
        DUMMY_HASH = "0000000000000000000000000000000000000000000000000000000000000000"
        has_image_data = hash_value != DUMMY_HASH
        
        # 画像データの有無をキャッシュに記録（EOF時の判定用）
        self.has_image_data_cache[sender_mac] = has_image_data
        
        if has_image_data:
            logger.info(f"Image data expected for {sender_mac} (hash: {hash_value[:16]}...)")
        else:
            logger.info(f"No image data expected for {sender_mac} (dummy hash detected)")
        
        # InfluxDBに書き込み
        try:
            influx_client.write_sensor_data(sender_mac, voltage, temperature)
            logger.info(f"Initiated InfluxDB write for {sender_mac}")
        except Exception as e:
            logger.error(f"InfluxDB write error for {sender_mac}: {e}")
        
        # 画像データがある場合のみストリーム管理を行う
        # スリープコマンドはEOF処理後に送信（xiaの受信体制が整ってから）
        if has_image_data:
            # HASHフレーム受信時は既存ストリームを強制終了しない
            # 代わりに、画像データが既に受信されている場合はそれを保持
            if sender_mac not in self.streaming_processor.active_streams:
                # 新規ストリーム開始
                await self.streaming_processor.start_image_stream(
                    sender_mac, hash_data=payload_str
                )
                logger.debug(f"Started new image stream for {sender_mac} after HASH")
            else:
                # 既存ストリームのメタデータを更新
                stream_meta = self.streaming_processor.active_streams[sender_mac]
                stream_meta.hash_data = payload_str
                logger.debug(f"Updated existing stream metadata for {sender_mac}")
            
            logger.info(f"Will send sleep command after EOF for {sender_mac} (image data expected)")
        else:
            logger.info(f"Will send sleep command after EOF for {sender_mac} (no image data - dummy hash)")
    
    async def _process_streaming_data_frame(self, sender_mac: str, 
                                          chunk_data: bytes, seq_num: int):
        """DATAフレーム処理（ストリーミング対応）"""
        # ストリーミングプロセッサーでチャンク処理
        success = await self.streaming_processor.process_chunk(
            sender_mac, chunk_data, seq_num, 
            callback=self._chunk_processed_callback
        )
        
        if success:
            current_time = time.monotonic()
            self.last_data_frame_time[sender_mac] = current_time
            
            # 統計更新（下位互換性）
            self.stats["total_bytes"] = self.stats.get("total_bytes", 0) + len(chunk_data)
        else:
            logger.warning(f"Failed to process chunk for {sender_mac}")
    
    async def _process_streaming_eof_frame(self, sender_mac: str):
        """EOFフレーム処理（ストリーミング対応）"""
        current_time = time.time()
        
        # 重複EOF処理チェック（5秒以内の重複を防止）
        if sender_mac in self.eof_processed:
            last_processed_time = self.eof_processed[sender_mac]
            time_diff = current_time - last_processed_time
            if time_diff < 5.0:
                logger.info(f"EOF already processed for {sender_mac} {time_diff:.1f}s ago, skipping duplicate")
                return
        
        logger.info(f"Processing EOF frame for {sender_mac}")
        
        # EOF処理済みとしてマーク
        self.eof_processed[sender_mac] = current_time
        
        # ストリーミング画像を完成・保存
        final_path = await self.streaming_processor.finalize_image_stream(
            sender_mac, self.stats
        )
        
        if final_path:
            # 統計更新
            self.stats["received_images"] = self.stats.get("received_images", 0) + 1
            logger.info(f"✓ Streaming image saved: {final_path}")
        else:
            logger.error(f"Failed to finalize streaming image for {sender_mac}")
        
        # EOFフレーム処理後にスリープコマンド送信
        await self._send_sleep_command_after_eof(sender_mac)
    
    async def _chunk_processed_callback(self, sender_mac: str, chunk_data: bytes, 
                                      seq_num: int):
        """チャンク処理完了時のコールバック"""
        # 必要に応じて追加の処理を実装
        pass
    
    async def _send_sleep_command(self, sender_mac: str, voltage: float):
        """スリープコマンド送信（重複送信防止機能付き）"""
        current_time = time.time()
        
        # 古い送信履歴をクリーンアップ（1時間以上前のものを削除）
        cleanup_threshold = current_time - 3600  # 1時間
        to_remove = [mac for mac, timestamp in self.sleep_command_sent.items() if timestamp < cleanup_threshold]
        for mac in to_remove:
            del self.sleep_command_sent[mac]
        
        # EOF処理済みフラグもクリーンアップ（1時間以上前のものを削除）
        eof_to_remove = [mac for mac, timestamp in self.eof_processed.items() if timestamp < cleanup_threshold]
        for mac in eof_to_remove:
            del self.eof_processed[mac]
        
        # 重複送信チェック（10秒以内の重複を防止）
        if sender_mac in self.sleep_command_sent:
            last_sent_time = self.sleep_command_sent[sender_mac]
            time_diff = current_time - last_sent_time
            if time_diff < 10.0:
                logger.info(f"Sleep command already sent to {sender_mac} {time_diff:.1f}s ago, skipping duplicate")
                return
        
        sleep_duration_s = determine_sleep_duration(voltage)
        command_to_gateway = format_sleep_command_to_gateway(sender_mac, sleep_duration_s)
        command_bytes = command_to_gateway.encode('utf-8')
        
        logger.info(f"Sending sleep command for {sender_mac} with voltage {voltage}% -> {sleep_duration_s}s sleep")
        
        if self.transport:
            try:
                self.transport.write(command_bytes)
                logger.info(f"Sent sleep command for {sender_mac}: {command_to_gateway.strip()}")
                # 送信履歴を記録
                self.sleep_command_sent[sender_mac] = current_time
            except Exception as e:
                logger.error(f"Error sending sleep command for {sender_mac}: {e}")
        else:
            logger.warning(f"No transport available for sleep command to {sender_mac}")
    
    async def _send_sleep_command_after_eof(self, sender_mac: str):
        """EOF処理後のスリープコマンド送信（xiaの受信体制が整った後）"""
        if sender_mac not in self.voltage_cache:
            logger.warning(f"No voltage cache for {sender_mac}, cannot send sleep command")
            return
            
        voltage = self.voltage_cache[sender_mac]
        if not isinstance(voltage, float):
            logger.warning(f"Invalid voltage for {sender_mac}: {voltage}")
            return
        
        logger.info(f"Retrieved voltage from cache for {sender_mac}: {voltage}% (type: {type(voltage)})")
        
        # HASHフレーム時にキャッシュされた画像データの有無を確認（ログ用）
        has_image_data = self.has_image_data_cache.get(sender_mac, True)
        
        if has_image_data:
            logger.info(f"Will send sleep command after EOF for {sender_mac} (with image data, voltage: {voltage}%)")
        else:
            logger.info(f"Will send sleep command after EOF for {sender_mac} (no image data - dummy hash, voltage: {voltage}%)")
        
        # xiaがスリープコマンド待機状態に入るまで待機
        delay_seconds = 2
        logger.info(f"Waiting {delay_seconds} seconds for {sender_mac} to enter sleep command reception mode...")
        await asyncio.sleep(delay_seconds)
        
        # xiaの受信体制が整ったタイミングでスリープコマンドを送信
        await self._send_sleep_command(sender_mac, voltage)
        
        # キャッシュから削除
        try:
            del self.voltage_cache[sender_mac]
            if sender_mac in self.has_image_data_cache:
                del self.has_image_data_cache[sender_mac]
        except KeyError as e:
            logger.warning(f"Cache cleanup error for {sender_mac}: {e}")
    
    async def _check_frame_timeout(self):
        """フレームタイムアウトチェック"""
        if not self.frame_start_time:
            return
        
        # アクティブなストリームがある場合は長めのタイムアウト
        active_streams = len(self.streaming_processor.active_streams)
        timeout_duration = 30.0 if active_streams > 0 else 2.0
        
        elapsed = time.monotonic() - self.frame_start_time
        if elapsed > timeout_duration:
            logger.warning(f"Frame timeout ({elapsed:.1f}s), clearing buffer")
            await self._handle_frame_timeout()
    
    async def _handle_frame_timeout(self):
        """フレームタイムアウト処理"""
        # バッファをクリアして次のSTART_MARKERを探す
        next_start = self.buffer.find(START_MARKER, 1)
        if next_start != -1:
            self.buffer = self.buffer[next_start:]
        else:
            self.buffer.clear()
        
        self.frame_start_time = None
    
    async def _handle_frame_error(self):
        """フレームエラー処理"""
        next_start = self.buffer.find(START_MARKER, 1)
        if next_start != -1:
            self.buffer = self.buffer[next_start:]
        else:
            self.buffer.clear()
        
        self.frame_start_time = None
    
    async def _timeout_check_loop(self):
        """定期的なタイムアウトチェック"""
        while True:
            try:
                await asyncio.sleep(5.0)  # 5秒間隔でチェック
                await self.streaming_processor.check_stream_timeouts()
            except asyncio.CancelledError:
                logger.info("Timeout check loop cancelled")
                break
            except Exception as e:
                logger.error(f"Error in timeout check loop: {e}")
    
    def _get_frame_type_name(self, frame_type: int) -> str:
        """フレームタイプ名を取得"""
        type_map = {
            FRAME_TYPE_HASH: "HASH",
            FRAME_TYPE_DATA: "DATA", 
            FRAME_TYPE_EOF: "EOF"
        }
        return type_map.get(frame_type, f"UNKNOWN({frame_type})")
    
    def connection_lost(self, exc):
        """接続切断時の処理"""
        logger.info(f"Connection lost: {exc}")
        self.transport = None
        
        # タイムアウトチェックタスクをキャンセル
        if self.timeout_check_task and not self.timeout_check_task.done():
            self.timeout_check_task.cancel()
        
        # ストリーミングプロセッサーをクリーンアップ
        asyncio.create_task(self.streaming_processor.cleanup_all_streams())
        
        # 接続切断通知
        if self.connection_lost_future and not self.connection_lost_future.done():
            if exc:
                try:
                    self.connection_lost_future.set_exception(exc)
                except asyncio.InvalidStateError:
                    pass
            else:
                try:
                    self.connection_lost_future.set_result(True)
                except asyncio.InvalidStateError:
                    pass
