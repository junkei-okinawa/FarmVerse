"""Image processing and storage."""

import asyncio
import io
import logging
import os
import time
from datetime import datetime
from typing import Dict

from PIL import Image

import sys
sys.path.append(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from config import config

logger = logging.getLogger(__name__)


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
            # InfluxDBクライアントのクリーンアップ
            from storage import influx_client
            if influx_client and hasattr(influx_client, 'close'):
                await influx_client.close()
        except Exception as e:
            logger.error(f"Error during cleanup: {e}")


def ensure_dir_exists():
    """画像保存ディレクトリの作成"""
    if not os.path.exists(config.IMAGE_DIR):
        os.makedirs(config.IMAGE_DIR)
        logger.info(f"Created directory: {config.IMAGE_DIR}")


async def save_image(sender_mac_str: str, image_data: bytes, stats: dict) -> None:
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
