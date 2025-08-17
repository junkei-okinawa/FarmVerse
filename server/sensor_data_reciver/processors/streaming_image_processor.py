"""
Streaming Image Processor for real-time image processing.

This module implements streaming-based image processing that eliminates
the need for complete image buffering, processing chunks as they arrive.
"""

import asyncio
import io
import logging
import os
import time
from datetime import datetime
from typing import Dict, Optional, Callable
from dataclasses import dataclass, field

# 絶対インポートを使用
import sys
sys.path.append(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from config import config


logger = logging.getLogger(__name__)


@dataclass
class StreamingImageMetadata:
    """画像ストリーミングのメタデータ"""
    sender_mac: str
    started_at: float
    total_chunks_received: int = 0
    total_bytes_received: int = 0
    last_chunk_time: float = field(default_factory=time.time)
    sequence_numbers: list = field(default_factory=list)
    is_completed: bool = False
    hash_data: Optional[str] = None


@dataclass
class StreamingStats:
    """ストリーミング統計情報"""
    total_images_processed: int = 0
    total_bytes_processed: int = 0
    average_chunk_size: float = 0.0
    average_processing_time: float = 0.0
    start_time: float = field(default_factory=time.time)
    
    def update_chunk_stats(self, chunk_size: int):
        """チャンク統計を更新"""
        self.total_bytes_processed += chunk_size
        # 移動平均でチャンクサイズを更新
        if self.total_images_processed > 0:
            self.average_chunk_size = (
                self.average_chunk_size * 0.9 + chunk_size * 0.1
            )
        else:
            self.average_chunk_size = chunk_size


class StreamingImageProcessor:
    """
    リアルタイム画像ストリーミング処理クラス
    
    従来のバッファ蓄積方式とは異なり、チャンク受信と同時に処理を行い、
    メモリ効率を大幅に向上させます。
    """
    
    def __init__(self, max_concurrent_streams: int = 5):
        """
        Args:
            max_concurrent_streams: 同時処理可能なストリーム数
        """
        self.active_streams: Dict[str, StreamingImageMetadata] = {}
        self.streaming_stats = StreamingStats()
        self.max_concurrent_streams = max_concurrent_streams
        
        # チャンク処理用の一時ファイルディレクトリ
        self.temp_dir = os.path.join(config.IMAGE_DIR, "streaming_temp")
        self._ensure_temp_dir()
        
        # 非同期タスク管理
        self.processing_tasks: Dict[str, asyncio.Task] = {}
        
        logger.info(f"StreamingImageProcessor initialized (max_streams={max_concurrent_streams})")
    
    def _ensure_temp_dir(self):
        """一時ディレクトリの作成"""
        if not os.path.exists(self.temp_dir):
            os.makedirs(self.temp_dir)
            logger.debug(f"Created streaming temp directory: {self.temp_dir}")
    
    async def start_image_stream(self, sender_mac: str, hash_data: Optional[str] = None) -> bool:
        """
        新しい画像ストリームを開始
        
        Args:
            sender_mac: 送信元MACアドレス
            hash_data: HASHフレームからのメタデータ
            
        Returns:
            bool: ストリーム開始成功/失敗
        """
        if len(self.active_streams) >= self.max_concurrent_streams:
            logger.warning(f"Maximum concurrent streams ({self.max_concurrent_streams}) reached")
            # 最も古いストリームを強制終了
            oldest_mac = min(
                self.active_streams.keys(),
                key=lambda mac: self.active_streams[mac].started_at
            )
            await self.abort_stream(oldest_mac, "Max streams exceeded")
        
        if sender_mac in self.active_streams:
            logger.warning(f"Stream already active for {sender_mac}, restarting")
            await self.abort_stream(sender_mac, "Restart requested")
        
        self.active_streams[sender_mac] = StreamingImageMetadata(
            sender_mac=sender_mac,
            started_at=time.time(),
            hash_data=hash_data
        )
        
        logger.info(f"Started image stream for {sender_mac}")
        return True
    
    async def process_chunk(
        self, 
        sender_mac: str, 
        chunk_data: bytes, 
        sequence_number: int,
        callback: Optional[Callable] = None
    ) -> bool:
        """
        チャンクデータをリアルタイム処理
        
        Args:
            sender_mac: 送信元MACアドレス
            chunk_data: チャンクデータ
            sequence_number: シーケンス番号
            callback: 処理完了時のコールバック関数
            
        Returns:
            bool: 処理成功/失敗
        """
        if sender_mac not in self.active_streams:
            logger.warning(f"No active stream for {sender_mac}, starting new stream")
            await self.start_image_stream(sender_mac)
        
        stream_meta = self.active_streams[sender_mac]
        
        # チャンク処理統計を更新
        stream_meta.total_chunks_received += 1
        stream_meta.total_bytes_received += len(chunk_data)
        stream_meta.last_chunk_time = time.time()
        stream_meta.sequence_numbers.append(sequence_number)
        
        # ストリーミング統計を更新
        self.streaming_stats.update_chunk_stats(len(chunk_data))
        
        try:
            # チャンクをテンポラリファイルに追記
            temp_file_path = self._get_temp_file_path(sender_mac)
            
            # ファイル書き込み（非同期）
            loop = asyncio.get_running_loop()
            await loop.run_in_executor(
                None, 
                self._append_chunk_to_file, 
                temp_file_path, 
                chunk_data
            )
            
            # 最初のチャンクでJPEGヘッダーを検証
            if stream_meta.total_chunks_received == 1:
                is_valid, error_reason = self._validate_jpeg_header(chunk_data)
                if not is_valid:
                    # 具体的なエラー理由をログに出力
                    logger.warning(f"Invalid JPEG header in first chunk for {sender_mac}: {error_reason}")
                    logger.debug(f"First chunk data: {chunk_data[:20].hex() if len(chunk_data) >= 20 else chunk_data.hex()}")
                    # JPEGヘッダーが無効でも処理を続行（EOF後に検証）
                    logger.info(f"Continuing stream processing for {sender_mac} despite invalid header")
                else:
                    logger.info(f"✓ Valid JPEG stream started for {sender_mac}")
            
            # コールバック実行
            if callback:
                try:
                    await callback(sender_mac, chunk_data, sequence_number)
                except Exception as e:
                    logger.error(f"Callback error for {sender_mac}: {e}")
            
            # 進捗ログ（5KB毎）
            if stream_meta.total_bytes_received % 5000 < len(chunk_data):
                logger.debug(
                    f"Streaming progress for {sender_mac}: "
                    f"{stream_meta.total_bytes_received} bytes "
                    f"({stream_meta.total_chunks_received} chunks)"
                )
            
            return True
            
        except Exception as e:
            logger.error(f"Error processing chunk for {sender_mac}: {e}")
            await self.abort_stream(sender_mac, f"Processing error: {e}")
            return False
    
    async def finalize_image_stream(self, sender_mac: str, stats: Optional[Dict] = None) -> Optional[str]:
        """
        画像ストリームを完成・保存
        
        Args:
            sender_mac: 送信元MACアドレス
            stats: 追加統計情報
            
        Returns:
            Optional[str]: 保存されたファイルパス（失敗時はNone）
        """
        if sender_mac not in self.active_streams:
            logger.error(f"No active stream to finalize for {sender_mac}")
            return None
        
        stream_meta = self.active_streams[sender_mac]
        temp_file_path = self._get_temp_file_path(sender_mac)
        
        try:
            # 一時ファイルの存在確認
            if not os.path.exists(temp_file_path):
                logger.error(f"Temp file not found for {sender_mac}: {temp_file_path}")
                await self.abort_stream(sender_mac, "Temp file missing")
                return None
            
            # ファイルサイズの確認
            file_size = os.path.getsize(temp_file_path)
            if file_size < 1000:  # 1KB未満は不正
                logger.error(f"Image file too small for {sender_mac}: {file_size} bytes")
                await self.abort_stream(sender_mac, "File too small")
                return None
            
            # 最終的な画像ファイルパスを生成
            timestamp = datetime.now().strftime("%Y%m%d_%H%M%S_%f")
            final_filename = f"{sender_mac.replace(':', '')}_{timestamp}.jpg"
            final_file_path = os.path.join(config.IMAGE_DIR, final_filename)
            
            # ファイル移動（非同期）
            loop = asyncio.get_running_loop()
            await loop.run_in_executor(
                None,
                self._move_temp_to_final,
                temp_file_path,
                final_file_path
            )
            
            # 画像回転処理（既存のロジックを維持）
            await self._create_rotated_image(
                final_file_path, 
                sender_mac
            )
            
            # 統計更新
            self.streaming_stats.total_images_processed += 1
            processing_time = time.time() - stream_meta.started_at
            self.streaming_stats.average_processing_time = (
                self.streaming_stats.average_processing_time * 0.9 +
                processing_time * 0.1
            )
            
            # 統計情報をログ出力
            if stats:
                stats["received_images"] = stats.get("received_images", 0) + 1
                stats["total_bytes"] = stats.get("total_bytes", 0) + stream_meta.total_bytes_received
            
            logger.info(
                f"✓ Finalized streaming image for {sender_mac}: "
                f"{final_filename} ({file_size} bytes, "
                f"{stream_meta.total_chunks_received} chunks, "
                f"{processing_time:.2f}s)"
            )
            
            # ストリームを正常終了
            stream_meta.is_completed = True
            await self._cleanup_stream(sender_mac)
            
            return final_file_path
            
        except Exception as e:
            logger.error(f"Error finalizing image stream for {sender_mac}: {e}")
            await self.abort_stream(sender_mac, f"Finalization error: {e}")
            return None
    
    async def abort_stream(self, sender_mac: str, reason: str):
        """
        ストリームを異常終了
        
        Args:
            sender_mac: 送信元MACアドレス
            reason: 終了理由
        """
        if sender_mac in self.active_streams:
            stream_meta = self.active_streams[sender_mac]
            logger.warning(
                f"Aborting stream for {sender_mac}: {reason} "
                f"({stream_meta.total_chunks_received} chunks, "
                f"{stream_meta.total_bytes_received} bytes)"
            )
        
        await self._cleanup_stream(sender_mac)
    
    async def _cleanup_stream(self, sender_mac: str):
        """ストリームのクリーンアップ"""
        # アクティブストリームから削除
        if sender_mac in self.active_streams:
            del self.active_streams[sender_mac]
        
        # 実行中のタスクをキャンセル
        if sender_mac in self.processing_tasks:
            task = self.processing_tasks[sender_mac]
            if not task.done():
                task.cancel()
            del self.processing_tasks[sender_mac]
        
        # 一時ファイルを削除
        temp_file_path = self._get_temp_file_path(sender_mac)
        if os.path.exists(temp_file_path):
            try:
                os.remove(temp_file_path)
                logger.debug(f"Removed temp file: {temp_file_path}")
            except OSError as e:
                logger.warning(f"Failed to remove temp file {temp_file_path}: {e}")
    
    def _get_temp_file_path(self, sender_mac: str) -> str:
        """一時ファイルパスを生成"""
        safe_mac = sender_mac.replace(':', '')
        return os.path.join(self.temp_dir, f"stream_{safe_mac}.tmp")
    
    def _append_chunk_to_file(self, file_path: str, chunk_data: bytes):
        """チャンクデータをファイルに追記（同期処理）"""
        with open(file_path, 'ab') as f:
            f.write(chunk_data)
    
    def _move_temp_to_final(self, temp_path: str, final_path: str):
        """一時ファイルを最終ファイルに移動（同期処理）"""
        import shutil
        shutil.move(temp_path, final_path)
    
    def _validate_jpeg_header(self, chunk_data: bytes) -> tuple[bool, Optional[str]]:
        """JPEGヘッダーを検証し、結果と理由を返します。

        Returns:
            tuple[bool, Optional[str]]: (検証結果, エラー理由)
        """
        if len(chunk_data) < 2:
            return False, f"Header too short. Expected at least 2 bytes, got {len(chunk_data)}."
        if not chunk_data.startswith(b'\xff\xd8'):
            return False, f"Invalid SOI marker. Expected 0xFFD8, got {chunk_data[:2].hex()}."
        return True, None
    
    async def _create_rotated_image(self, image_path: str, sender_mac: str) -> Optional[str]:
        """回転画像を作成（既存ロジックを維持）"""
        try:
            # 非同期で画像回転処理
            loop = asyncio.get_running_loop()
            rotated_path = await loop.run_in_executor(
                None,
                self._rotate_image_sync,
                image_path,
                sender_mac
            )
            return rotated_path
        except Exception as e:
            logger.error(f"Error creating rotated image for {sender_mac}: {e}")
            return None
    
    def _rotate_image_sync(self, image_path: str, sender_mac: str) -> str:
        """同期版画像回転処理"""
        try:
            from PIL import Image
        except ImportError:
            logger.warning("PIL not available, skipping image rotation")
            return image_path
            
        with open(image_path, 'rb') as f:
            image_data = f.read()
        
        # 画像回転処理
        im = Image.open(io.BytesIO(image_data))
        rotated = im.rotate(90, expand=True)
        
        # 回転画像ファイルパス
        base = os.path.splitext(os.path.basename(image_path))[0].split("_")[0]
        rotated_filename = os.path.join(config.IMAGE_DIR, f"{base}.jpg")
        
        rotated.save(rotated_filename)
        logger.info(f"Saved rotated image: {rotated_filename}")
        
        return rotated_filename
    
    def get_stream_status(self, sender_mac: str) -> Optional[Dict]:
        """ストリーム状態を取得"""
        if sender_mac not in self.active_streams:
            return None
        
        stream_meta = self.active_streams[sender_mac]
        return {
            "sender_mac": sender_mac,
            "started_at": stream_meta.started_at,
            "chunks_received": stream_meta.total_chunks_received,
            "bytes_received": stream_meta.total_bytes_received,
            "last_chunk_time": stream_meta.last_chunk_time,
            "duration": time.time() - stream_meta.started_at,
            "is_completed": stream_meta.is_completed
        }
    
    def get_overall_stats(self) -> Dict:
        """全体統計を取得"""
        return {
            "active_streams": len(self.active_streams),
            "total_images_processed": self.streaming_stats.total_images_processed,
            "total_bytes_processed": self.streaming_stats.total_bytes_processed,
            "average_chunk_size": self.streaming_stats.average_chunk_size,
            "average_processing_time": self.streaming_stats.average_processing_time,
            "uptime": time.time() - self.streaming_stats.start_time
        }
    
    async def cleanup_all_streams(self):
        """全ストリームのクリーンアップ"""
        for sender_mac in list(self.active_streams.keys()):
            await self._cleanup_stream(sender_mac)
        
        # 一時ディレクトリのクリーンアップ
        try:
            import shutil
            if os.path.exists(self.temp_dir):
                shutil.rmtree(self.temp_dir)
                logger.info("Cleaned up streaming temp directory")
        except Exception as e:
            logger.warning(f"Failed to cleanup temp directory: {e}")
    
    async def check_stream_timeouts(self, timeout_seconds: float = 30.0):
        """ストリームタイムアウトチェック"""
        current_time = time.time()
        timed_out_streams = []
        
        for sender_mac, stream_meta in self.active_streams.items():
            if current_time - stream_meta.last_chunk_time > timeout_seconds:
                timed_out_streams.append(sender_mac)
        
        for sender_mac in timed_out_streams:
            await self.abort_stream(sender_mac, f"Timeout ({timeout_seconds}s)")
