import asyncio
import unittest
from unittest.mock import AsyncMock, MagicMock, patch
import sys
import os

# テスト対象へのパスを通す
# Use absolute path to project root to allow running from any directory
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', '..')) # To tests/.. (server/sensor_data_reciver)
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', '..', '..')) # To server/

# Also ensure 'server/sensor_data_reciver' is in path to find 'protocol' if running from root
sensor_data_receiver_path = os.path.abspath(os.path.join(os.path.dirname(__file__), '..', '..'))
if sensor_data_receiver_path not in sys.path:
    sys.path.insert(0, sensor_data_receiver_path)

from protocol.streaming_handler import StreamingSerialProtocol
from protocol.constants import (
    START_MARKER, SEQUENCE_NUM_LENGTH, 
    LENGTH_FIELD_BYTES, CHECKSUM_LENGTH, END_MARKER,
    FRAME_TYPE_HASH
)

class TestStreamingHandler(unittest.IsolatedAsyncioTestCase):
    
    async def asyncSetUp(self):
        # IsolatedAsyncioTestCase が管理するイベントループを使用する
        self.loop = asyncio.get_running_loop()
        
        self.mock_future = self.loop.create_future()
        self.stats = {}
        
        # モックの作成
        self.patchers = [
            patch('protocol.streaming_handler.StreamingImageProcessor'),
            patch('protocol.streaming_handler.VoltageDataProcessor'),
            patch('protocol.streaming_handler.influx_client')
        ]
        
        for p in self.patchers:
            p.start()
            
        self.protocol = StreamingSerialProtocol(self.mock_future, self.stats)
        
        # _process_frame_by_type をモック化して呼び出しを検知
        self.protocol._process_frame_by_type = AsyncMock()
        
        # process_chunk もモック化（通常のデータ処理用）
        self.protocol.streaming_processor.process_chunk = AsyncMock(return_value=True)

    async def asyncTearDown(self):
        for p in self.patchers:
            p.stop()

    def create_frame_bytes(self, frame_type, payload, seq_num=1):
        """フレームのバイト列を作成するヘルパー"""
        mac_bytes = b'\x01\x02\x03\x04\x05\x06'
        
        frame = (
            START_MARKER +
            mac_bytes +
            bytes([frame_type]) +
            seq_num.to_bytes(SEQUENCE_NUM_LENGTH, byteorder='little') +
            len(payload).to_bytes(LENGTH_FIELD_BYTES, byteorder='little') +
            payload +
            b'\x00' * CHECKSUM_LENGTH +
            END_MARKER
        )
        return frame

    def create_raw_header_and_payload(self, frame_type, payload, seq_num=1):
        """ヘッダー＋ペイロードのみを作成（チェックサム・ENDマーカーなし）"""
        # これはDATAフレームのペイロードとして使われる「内部フレーム」用
        # 実際には完全なフレーム構造を持っているはずだが、テスト対象のロジックは
        # parse_header でヘッダーを解析し、data_len 分だけ切り出す
        
        # ただし、現状の実装では parse_header は完全なフレームの一部であることを前提としているわけではないが
        # ネストされたフレームは「Senderが送ったフレームそのもの」がDATAフレームに入っている想定
        return self.create_frame_bytes(frame_type, payload, seq_num)

    async def test_nested_frame_unpacking(self):
        """二重カプセル化されたフレームが正しく解凍されることをテスト"""
        sender_mac = "01:02:03:04:05:06"
        seq_num = 100
        
        # 内部フレーム（本来送りたかったHASHフレーム）を作成
        # フッター検証のために完全なフレーム構造が必要
        inner_payload = b"HASH:dummy_hash,VOLT:100"
        inner_frame = self.create_frame_bytes(FRAME_TYPE_HASH, inner_payload, seq_num=200)
        
        # これをDATAフレームのペイロードとして渡す
        chunk_data = inner_frame
        
        # テスト実行
        await self.protocol._process_streaming_data_frame(sender_mac, chunk_data, seq_num)
        
        # 検証
        self.protocol._process_frame_by_type.assert_called_once()
        
        args = self.protocol._process_frame_by_type.call_args[0]
        called_mac, called_type, called_seq, called_payload = args
        
        self.assertEqual(called_mac, "01:02:03:04:05:06")
        self.assertEqual(called_type, FRAME_TYPE_HASH)
        self.assertEqual(called_seq, 200)
        self.assertEqual(called_payload, inner_payload)
        
        self.protocol.streaming_processor.process_chunk.assert_not_called()

    async def test_raw_data_processing(self):
        """通常のデータ（ネストされていない）がそのまま処理されることをテスト"""
        sender_mac = "01:02:03:04:05:06"
        seq_num = 101
        
        # START_MARKER で始まらない通常のデータ
        chunk_data = b"\x00\x01\x02\x03" * 10
        
        # テスト実行
        await self.protocol._process_streaming_data_frame(sender_mac, chunk_data, seq_num)
        
        # 検証
        # 1. _process_frame_by_type は呼び出されない
        self.protocol._process_frame_by_type.assert_not_called()
        
        # 2. process_chunk が呼び出される
        self.protocol.streaming_processor.process_chunk.assert_called_once()

    async def test_false_positive_nested_frame(self):
        """START_MARKERで始まるが有効なフレームでない場合、生データとして処理されることをテスト"""
        sender_mac = "01:02:03:04:05:06"
        seq_num = 102
        
        # START_MARKER で始まるが、その後の構造が不正なデータ
        chunk_data = START_MARKER + b"invalid_header_structure"
        
        # テスト実行
        await self.protocol._process_streaming_data_frame(sender_mac, chunk_data, seq_num)
        
        # 検証
        # 1. _process_frame_by_type は呼び出されない（パース失敗）
        self.protocol._process_frame_by_type.assert_not_called()
        
        # 2. process_chunk が呼び出される（フォールバック）
        self.protocol.streaming_processor.process_chunk.assert_called_once()

    async def test_eof_without_hash_emits_cycle_warning(self):
        """HASHなしEOFでサイクル警告が出ることをテスト"""
        sender_mac = "01:02:03:04:05:06"
        seq_num = 103

        self.protocol.streaming_processor.finalize_image_stream = AsyncMock(return_value=None)
        self.protocol._send_sleep_command_after_eof = AsyncMock()

        with self.assertLogs("protocol.cycle_tracker", level="WARNING") as logs:
            await self.protocol._process_streaming_eof_frame(sender_mac, seq_num)

        self.assertTrue(
            any("EOF received before DATA/HASH" in message for message in logs.output)
        )
        self.assertEqual(len(logs.output), 1)
        self.assertEqual(self.protocol.cycle_tracker.get_state(sender_mac).cycle_state, "Completed")
        self.protocol._send_sleep_command_after_eof.assert_awaited_once_with(sender_mac)

    async def test_invalid_hash_payload_does_not_mark_cycle_received(self):
        """無効なHASHペイロードでcycle状態が汚染されないことをテスト"""
        sender_mac = "01:02:03:04:05:06"
        seq_num = 104
        chunk_data = b"HASH:broken"

        await self.protocol._process_streaming_hash_frame(sender_mac, chunk_data, seq_num)

        self.assertIsNone(self.protocol.cycle_tracker.get_state(sender_mac))

    async def test_dry_run_skips_finalize_image_stream(self):
        """DRY_RUN モードでは finalize_image_stream が呼ばれないことをテスト"""
        sender_mac = "01:02:03:04:05:06"
        seq_num = 110

        self.protocol.streaming_processor.finalize_image_stream = AsyncMock(return_value="/tmp/img.jpg")
        self.protocol._send_sleep_command_after_eof = AsyncMock()

        with patch('protocol.streaming_handler.config') as mock_config:
            mock_config.DRY_RUN = True
            mock_config.DEBUG_FRAME_PARSING = False
            mock_config.SUPPRESS_SYNC_ERRORS = False
            mock_config.SUPPRESS_DISCARD_LOGS = False
            mock_config.MAX_DATA_LEN = 250 * 1024

            await self.protocol._process_streaming_eof_frame(sender_mac, seq_num)

        # DRY_RUN なので finalize_image_stream は呼ばれない
        self.protocol.streaming_processor.finalize_image_stream.assert_not_called()
        # スリープコマンド処理は呼ばれる
        self.protocol._send_sleep_command_after_eof.assert_awaited_once_with(sender_mac)

    async def test_dry_run_send_sleep_command_skips_write(self):
        """DRY_RUN モードでは _send_sleep_command が transport.write を呼ばないことをテスト"""
        sender_mac = "01:02:03:04:05:06"

        mock_transport = MagicMock()
        self.protocol.transport = mock_transport

        with patch('protocol.streaming_handler.config') as mock_config:
            mock_config.DRY_RUN = True
            mock_config.DEBUG_FRAME_PARSING = False

            await self.protocol._send_sleep_command(sender_mac, 85.0)

        # DRY_RUN なので transport.write は呼ばれない
        mock_transport.write.assert_not_called()
        # 重複送信抑止の内部状態は更新される
        self.assertIn(sender_mac, self.protocol.sleep_command_sent)

    async def test_buffer_processing_is_serialized(self):
        """複数の buffer processing task が同時に実行されないことをテスト"""
        first_entered = asyncio.Event()
        release_first = asyncio.Event()
        entered = []

        async def fake_process_streaming_buffer():
            entered.append(len(entered) + 1)
            first_entered.set()
            if len(entered) == 1:
                await release_first.wait()

        self.protocol._process_streaming_buffer = fake_process_streaming_buffer

        task1 = asyncio.create_task(self.protocol._process_buffer_async())
        await first_entered.wait()

        task2 = asyncio.create_task(self.protocol._process_buffer_async())
        await asyncio.sleep(0)

        self.assertEqual(entered, [1])

        release_first.set()
        await asyncio.gather(task1, task2)
        self.assertEqual(entered, [1, 2])

    async def test_data_received_coalesces_buffer_tasks(self):
        """data_received が buffer processing task を増殖させないことをテスト"""
        mock_task = MagicMock()
        mock_task.done.return_value = False

        def fake_create_task(coro):
            coro.close()
            return mock_task

        with patch("protocol.streaming_handler.asyncio.create_task", side_effect=fake_create_task) as create_task:
            self.protocol.data_received(b"abc")
            self.protocol.data_received(b"def")

        self.assertEqual(create_task.call_count, 1)
        self.assertEqual(self.protocol.buffer, bytearray(b"abcdef"))

    async def test_process_buffer_async_does_not_self_reschedule(self):
        """_process_buffer_async が自分自身を再スケジュールしないことをテスト"""
        self.protocol.buffer.extend(b"partial")
        self.protocol._process_streaming_buffer = AsyncMock()

        with patch("protocol.streaming_handler.asyncio.create_task") as create_task:
            await self.protocol._process_buffer_async()

        create_task.assert_not_called()
        self.assertIsNone(self.protocol._buffer_processing_task)

if __name__ == '__main__':
    unittest.main()
