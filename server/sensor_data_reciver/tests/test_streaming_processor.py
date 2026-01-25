"""
Test cases for Streaming Image Processor

This module tests the core functionality of the streaming-based
image processing implementation.
"""

import asyncio
import os
import tempfile
import unittest
from unittest.mock import MagicMock, patch
import shutil

import sys
sys.path.append(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from processors.streaming_image_processor import StreamingImageProcessor
from config import config


class TestStreamingImageProcessor(unittest.TestCase):
    """StreamingImageProcessor のテストケース"""

    def setUp(self):
        """テストセットアップ"""
        # 一時ディレクトリを作成
        self.temp_dir = tempfile.mkdtemp()
        
        # Configのパッチ
        self.config_patch = patch.object(config, 'IMAGE_DIR', self.temp_dir)
        self.config_patch.start()
        
        # プロセッサーを初期化
        self.processor = StreamingImageProcessor(max_concurrent_streams=3)

    def tearDown(self):
        """テストクリーンアップ"""
        # パッチを停止
        self.config_patch.stop()
        
        # 一時ディレクトリを削除
        if os.path.exists(self.temp_dir):
            shutil.rmtree(self.temp_dir)

    async def test_start_image_stream(self):
        """画像ストリーム開始のテスト"""
        sender_mac = "aa:bb:cc:dd:ee:ff"
        hash_data = "test_hash_data"
        
        # ストリーム開始
        result = await self.processor.start_image_stream(sender_mac, hash_data)
        
        self.assertTrue(result)
        self.assertIn(sender_mac, self.processor.active_streams)
        
        stream_meta = self.processor.active_streams[sender_mac]
        self.assertEqual(stream_meta.sender_mac, sender_mac)
        self.assertEqual(stream_meta.hash_data, hash_data)
        self.assertEqual(stream_meta.total_chunks_received, 0)

    async def test_process_chunk(self):
        """チャンク処理のテスト"""
        sender_mac = "aa:bb:cc:dd:ee:ff"
        chunk_data = b'\xff\xd8' + b'test_jpeg_data' * 10  # JPEGヘッダー + データ
        sequence_number = 1
        
        # ストリーム開始
        await self.processor.start_image_stream(sender_mac)
        
        # チャンク処理
        result = await self.processor.process_chunk(
            sender_mac, chunk_data, sequence_number
        )
        
        self.assertTrue(result)
        
        # メタデータの確認
        stream_meta = self.processor.active_streams[sender_mac]
        self.assertEqual(stream_meta.total_chunks_received, 1)
        self.assertEqual(stream_meta.total_bytes_received, len(chunk_data))
        self.assertIn(sequence_number, stream_meta.sequence_numbers)
        
        # 一時ファイルの確認
        temp_file_path = self.processor._get_temp_file_path(sender_mac)
        self.assertTrue(os.path.exists(temp_file_path))
        
        with open(temp_file_path, 'rb') as f:
            file_content = f.read()
        self.assertEqual(file_content, chunk_data)

    async def test_multiple_chunks(self):
        """複数チャンク処理のテスト"""
        sender_mac = "aa:bb:cc:dd:ee:ff"
        chunks = [
            b'\xff\xd8' + b'chunk1_data',  # JPEGヘッダー + データ
            b'chunk2_data',
            b'chunk3_data' + b'\xff\xd9'   # 最後にJPEGフッター
        ]
        
        await self.processor.start_image_stream(sender_mac)
        
        # 複数チャンクを処理
        for i, chunk in enumerate(chunks):
            result = await self.processor.process_chunk(
                sender_mac, chunk, i + 1
            )
            self.assertTrue(result)
        
        # メタデータの確認
        stream_meta = self.processor.active_streams[sender_mac]
        self.assertEqual(stream_meta.total_chunks_received, 3)
        expected_size = sum(len(chunk) for chunk in chunks)
        self.assertEqual(stream_meta.total_bytes_received, expected_size)
        
        # 一時ファイルの内容確認
        temp_file_path = self.processor._get_temp_file_path(sender_mac)
        with open(temp_file_path, 'rb') as f:
            file_content = f.read()
        
        expected_content = b''.join(chunks)
        self.assertEqual(file_content, expected_content)

    @patch('processors.streaming_image_processor.Image')
    async def test_finalize_image_stream(self, mock_image):
        """画像ストリーム完成のテスト"""
        sender_mac = "aa:bb:cc:dd:ee:ff"
        
        # PIL Image のモック設定
        mock_img = MagicMock()
        mock_rotated = MagicMock()
        mock_img.rotate.return_value = mock_rotated
        mock_image.open.return_value = mock_img
        
        # テストデータを準備
        test_image_data = b'\xff\xd8' + b'test_jpeg_data' * 100 + b'\xff\xd9'  # > 1000 bytes
        
        await self.processor.start_image_stream(sender_mac)
        await self.processor.process_chunk(sender_mac, test_image_data, 1)
        
        # 統計情報
        stats = {"received_images": 0, "total_bytes": 0}
        
        # ストリーム完成
        final_path = await self.processor.finalize_image_stream(sender_mac, stats)
        
        self.assertIsNotNone(final_path)
        self.assertTrue(os.path.exists(final_path))
        
        # 統計更新の確認
        self.assertEqual(stats["received_images"], 1)
        self.assertEqual(stats["total_bytes"], len(test_image_data))
        
        # ストリームがクリーンアップされている確認
        self.assertNotIn(sender_mac, self.processor.active_streams)
        
        # 一時ファイルが削除されている確認
        temp_file_path = self.processor._get_temp_file_path(sender_mac)
        self.assertFalse(os.path.exists(temp_file_path))

    async def test_abort_stream(self):
        """ストリーム中断のテスト"""
        sender_mac = "aa:bb:cc:dd:ee:ff"
        
        await self.processor.start_image_stream(sender_mac)
        await self.processor.process_chunk(sender_mac, b'\xff\xd8test_data', 1)
        
        # ストリームが存在することを確認
        self.assertIn(sender_mac, self.processor.active_streams)
        temp_file_path = self.processor._get_temp_file_path(sender_mac)
        self.assertTrue(os.path.exists(temp_file_path))
        
        # ストリーム中断
        await self.processor.abort_stream(sender_mac, "Test abort")
        
        # クリーンアップの確認
        self.assertNotIn(sender_mac, self.processor.active_streams)
        self.assertFalse(os.path.exists(temp_file_path))

    async def test_max_concurrent_streams(self):
        """最大同時ストリーム数制限のテスト"""
        max_streams = 2
        processor = StreamingImageProcessor(max_concurrent_streams=max_streams)
        
        # 最大数まで開始
        for i in range(max_streams):
            sender_mac = f"aa:bb:cc:dd:ee:f{i}"
            result = await processor.start_image_stream(sender_mac)
            self.assertTrue(result)
        
        self.assertEqual(len(processor.active_streams), max_streams)
        
        # 最大数を超えて開始（最も古いストリームが削除される）
        new_sender_mac = f"aa:bb:cc:dd:ee:f{max_streams}"
        result = await processor.start_image_stream(new_sender_mac)
        self.assertTrue(result)
        
        # ストリーム数は最大数を維持
        self.assertEqual(len(processor.active_streams), max_streams)
        self.assertIn(new_sender_mac, processor.active_streams)

    async def test_invalid_jpeg_header(self):
        """無効なJPEGヘッダーのテスト"""
        sender_mac = "aa:bb:cc:dd:ee:ff"
        invalid_chunk = b'invalid_jpeg_data'  # JPEGヘッダーなし
        
        await self.processor.start_image_stream(sender_mac)
        
        # 無効なヘッダーでチャンク処理
        result = await self.processor.process_chunk(sender_mac, invalid_chunk, 1)
        
        # 現在の実装では処理を続行するためTrueが返される
        self.assertTrue(result)
        self.assertIn(sender_mac, self.processor.active_streams)

    async def test_statistics_update(self):
        """統計情報更新のテスト"""
        sender_mac = "aa:bb:cc:dd:ee:ff"
        # ファイルサイズ制限(1000bytes)を超えるようにデータを増やす
        chunk_data = b'\xff\xd8' + b'test_data' * 200
        
        initial_stats = self.processor.streaming_stats
        initial_images = initial_stats.total_images_processed
        initial_bytes = initial_stats.total_bytes_processed
        
        await self.processor.start_image_stream(sender_mac)
        await self.processor.process_chunk(sender_mac, chunk_data, 1)
        
        # チャンク統計の更新確認
        self.assertEqual(
            initial_stats.total_bytes_processed, 
            initial_bytes + len(chunk_data)
        )
        
        # 画像完成統計の確認（finalize後）
        await self.processor.finalize_image_stream(sender_mac)
        self.assertEqual(
            initial_stats.total_images_processed, 
            initial_images + 1
        )

    def test_get_stream_status(self):
        """ストリーム状態取得のテスト"""
        sender_mac = "aa:bb:cc:dd:ee:ff"
        
        # ストリームが存在しない場合
        status = self.processor.get_stream_status(sender_mac)
        self.assertIsNone(status)

    async def test_get_stream_status_active(self):
        """アクティブストリーム状態取得のテスト"""
        sender_mac = "aa:bb:cc:dd:ee:ff"
        
        await self.processor.start_image_stream(sender_mac)
        await self.processor.process_chunk(sender_mac, b'\xff\xd8test', 1)
        
        # ストリーム状態を取得
        status = self.processor.get_stream_status(sender_mac)
        
        self.assertIsNotNone(status)
        self.assertEqual(status["sender_mac"], sender_mac)
        self.assertEqual(status["chunks_received"], 1)
        self.assertGreater(status["bytes_received"], 0)
        self.assertFalse(status["is_completed"])

    def test_get_overall_stats(self):
        """全体統計取得のテスト"""
        stats = self.processor.get_overall_stats()
        
        expected_keys = [
            "active_streams", "total_images_processed", 
            "total_bytes_processed", "average_chunk_size",
            "average_processing_time", "uptime"
        ]
        
        for key in expected_keys:
            self.assertIn(key, stats)
        
        self.assertIsInstance(stats["active_streams"], int)
        self.assertGreaterEqual(stats["uptime"], 0)


class TestStreamingIntegration(unittest.TestCase):
    """統合テストケース"""

    def setUp(self):
        """統合テストセットアップ"""
        self.temp_dir = tempfile.mkdtemp()
        self.config_patch = patch.object(config, 'IMAGE_DIR', self.temp_dir)
        self.config_patch.start()

    def tearDown(self):
        """統合テストクリーンアップ"""
        self.config_patch.stop()
        if os.path.exists(self.temp_dir):
            shutil.rmtree(self.temp_dir)

    @patch('processors.streaming_image_processor.Image')
    async def test_complete_image_streaming_workflow(self, mock_image):
        """完全な画像ストリーミングワークフローのテスト"""
        # PIL Image のモック設定
        mock_img = MagicMock()
        mock_rotated = MagicMock()
        mock_img.rotate.return_value = mock_rotated
        mock_image.open.return_value = mock_img
        
        processor = StreamingImageProcessor()
        sender_mac = "aa:bb:cc:dd:ee:ff"
        
        # 模擬画像データ（複数チャンク）
        jpeg_header = b'\xff\xd8'
        jpeg_footer = b'\xff\xd9'
        chunk_size = 250
        
        chunks = [
            jpeg_header + b'x' * (chunk_size - 2),  # 最初のチャンク
        ]
        
        # 中間チャンクを追加
        for i in range(10):
            chunks.append(b'y' * chunk_size)
        
        # 最後のチャンク
        chunks.append(b'z' * (chunk_size - 2) + jpeg_footer)
        
        # 1. ストリーム開始
        await processor.start_image_stream(sender_mac, "test_hash")
        
        # 2. チャンクを順次処理
        for i, chunk in enumerate(chunks):
            result = await processor.process_chunk(sender_mac, chunk, i + 1)
            self.assertTrue(result, f"Failed to process chunk {i + 1}")
        
        # 3. ストリーム完成
        stats = {"received_images": 0, "total_bytes": 0}
        final_path = await processor.finalize_image_stream(sender_mac, stats)
        
        # 4. 結果検証
        self.assertIsNotNone(final_path)
        self.assertTrue(os.path.exists(final_path))
        
        # ファイルサイズ確認
        expected_size = sum(len(chunk) for chunk in chunks)
        actual_size = os.path.getsize(final_path)
        self.assertEqual(actual_size, expected_size)
        
        # 統計確認
        self.assertEqual(stats["received_images"], 1)
        self.assertEqual(stats["total_bytes"], expected_size)
        
        # クリーンアップ確認
        self.assertEqual(len(processor.active_streams), 0)


# 非同期テスト実行のためのヘルパー
def async_test(coro):
    """非同期テストケースのデコレータ"""
    def wrapper(self):
        loop = asyncio.new_event_loop()
        asyncio.set_event_loop(loop)
        try:
            return loop.run_until_complete(coro(self))
        finally:
            loop.close()
    return wrapper


# 非同期テストメソッドにデコレータを適用
for name, method in TestStreamingImageProcessor.__dict__.items():
    if name.startswith('test_') and asyncio.iscoroutinefunction(method):
        setattr(TestStreamingImageProcessor, name, async_test(method))

for name, method in TestStreamingIntegration.__dict__.items():
    if name.startswith('test_') and asyncio.iscoroutinefunction(method):
        setattr(TestStreamingIntegration, name, async_test(method))


if __name__ == '__main__':
    unittest.main()
