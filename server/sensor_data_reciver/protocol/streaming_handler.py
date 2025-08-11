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
from ..processors.streaming_image_processor import StreamingImageProcessor
from ..processors.voltage_processor import VoltageDataProcessor
from ..processors.sleep_controller import determine_sleep_duration, format_sleep_command_to_gateway
from ..storage import influx_client
from ..utils.data_parser import DataParser
from ..config import config

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
        if config.DEBUG_FRAME_PARSING and len(data) < 100:
            logger.debug(f"Raw data: {data.hex()}")
        
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
            
            # 開始マーカーを探す
            start_index = self.buffer.find(START_MARKER)
            if start_index == -1:
                if len(self.buffer) >= len(START_MARKER):
                    self.buffer = self.buffer[-(len(START_MARKER) - 1):]
                break
            
            if start_index > 0:
                discarded_data = self.buffer[:start_index]
                logger.warning(f"Discarding {start_index} bytes: {discarded_data.hex()}")
                self.buffer = self.buffer[start_index:]
                self.frame_start_time = time.monotonic()
                continue
            
            # フレーム受信開始時間を記録
            if self.frame_start_time is None:
                self.frame_start_time = time.monotonic()
            
            # ヘッダー解析に必要なデータ長チェック
            header_length = (len(START_MARKER) + MAC_ADDRESS_LENGTH + 
                           FRAME_TYPE_LENGTH + SEQUENCE_NUM_LENGTH + LENGTH_FIELD_BYTES)
            
            if len(self.buffer) < header_length:
                break
            
            try:
                # ヘッダーを解析
                sender_mac, frame_type, seq_num, data_len = FrameParser.parse_header(self.buffer, 0)
                
                # フレーム検証
                header_start = len(START_MARKER)
                mac_bytes = self.buffer[header_start:header_start + MAC_ADDRESS_LENGTH]
                FrameParser.validate_frame_data(data_len, mac_bytes, config.MAX_DATA_LEN)
                
            except (ValueError, IndexError) as e:
                logger.error(f"Frame decode error: {e}")
                await self._handle_frame_error()
                continue
            
            # フレーム全体の長さを計算
            frame_end_index = (header_length + data_len + 
                             CHECKSUM_LENGTH + len(END_MARKER))
            
            if len(self.buffer) < frame_end_index:
                break  # 完全なフレームを待つ
            
            # データ部分を抽出
            data_start_index = header_length
            chunk_data = self.buffer[data_start_index:data_start_index + data_len]
            
            # エンドマーカーを確認
            end_marker_start = data_start_index + data_len + CHECKSUM_LENGTH
            footer = self.buffer[end_marker_start:frame_end_index]
            
            if footer != END_MARKER:
                logger.warning(f"Invalid end marker for {sender_mac}")
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
                logger.debug(f"Processed {frame_type_name} frame from {sender_mac}")
    
    async def _process_frame_by_type(self, sender_mac: str, frame_type: int, 
                                   seq_num: int, chunk_data: bytes):
        """フレームタイプ別処理"""
        if frame_type == FRAME_TYPE_HASH:
            await self._process_streaming_hash_frame(sender_mac, chunk_data)
            
        elif frame_type == FRAME_TYPE_DATA:
            await self._process_streaming_data_frame(
                sender_mac, chunk_data, seq_num
            )
            
        elif frame_type == FRAME_TYPE_EOF:
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
        
        volt_log_entry = payload_split[1]
        temp_log_entry = payload_split[2] if len(payload_split) > 2 else ""
        
        # 電圧・温度情報を抽出
        voltage = DataParser.extract_voltage_with_validation(volt_log_entry, sender_mac)
        temperature = DataParser.extract_temperature_with_validation(temp_log_entry, sender_mac)
        
        # 電圧情報をキャッシュ
        self.voltage_cache[sender_mac] = voltage
        
        # InfluxDBに書き込み
        try:
            influx_client.write_sensor_data(sender_mac, voltage, temperature)
            logger.info(f"Initiated InfluxDB write for {sender_mac}")
        except Exception as e:
            logger.error(f"InfluxDB write error for {sender_mac}: {e}")
        
        # ストリーミング画像処理を開始
        await self.streaming_processor.start_image_stream(
            sender_mac, hash_data=payload_str
        )
        
        # HASHフレーム時点でスリープコマンドを送信
        if voltage is not None:
            await self._send_sleep_command(sender_mac, voltage)
    
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
        logger.info(f"Processing EOF frame for {sender_mac}")
        
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
        """スリープコマンド送信"""
        sleep_duration_s = determine_sleep_duration(voltage)
        command_to_gateway = format_sleep_command_to_gateway(sender_mac, sleep_duration_s)
        command_bytes = command_to_gateway.encode('utf-8')
        
        if self.transport:
            try:
                self.transport.write(command_bytes)
                logger.info(f"Sent sleep command for {sender_mac}: {command_to_gateway.strip()}")
            except Exception as e:
                logger.error(f"Error sending sleep command for {sender_mac}: {e}")
        else:
            logger.warning(f"No transport available for sleep command to {sender_mac}")
    
    async def _send_sleep_command_after_eof(self, sender_mac: str):
        """EOF処理後のスリープコマンド送信"""
        if sender_mac in self.voltage_cache:
            voltage = self.voltage_cache[sender_mac]
            if isinstance(voltage, float):
                await self._send_sleep_command(sender_mac, voltage)
                del self.voltage_cache[sender_mac]
            else:
                logger.warning(f"Invalid voltage for {sender_mac}: {voltage}")
        else:
            logger.warning(f"No voltage cache for {sender_mac}")
    
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
