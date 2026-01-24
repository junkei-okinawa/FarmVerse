import asyncio
import unittest
from unittest.mock import AsyncMock, patch
import sys
import os

# テスト対象へのパスを通す
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', '..'))

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

if __name__ == '__main__':
    unittest.main()
