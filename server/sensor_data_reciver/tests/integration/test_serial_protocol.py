import asyncio
import os
import shutil
import sys
import time
from unittest.mock import MagicMock, patch

import pytest
import pytest_asyncio

# テストファイルから見た app.py への正しいパス
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', '..'))

from app import (CHECKSUM_LENGTH, END_MARKER, FRAME_TYPE_DATA, FRAME_TYPE_EOF,
                 FRAME_TYPE_HASH, LENGTH_FIELD_BYTES, SEQUENCE_NUM_LENGTH,
                 START_MARKER, SerialProtocol, config, image_receiver)


@pytest_asyncio.fixture
async def setup_test_environment():
    """テスト環境のセットアップ"""
    # テスト用に設定を調整
    original_config = {
        'MAX_BUFFER_SIZE': getattr(config, 'MAX_BUFFER_SIZE', None),
        'IMAGE_TIMEOUT': getattr(config, 'IMAGE_TIMEOUT', None),
        'IMAGE_DIR': getattr(config, 'IMAGE_DIR', None)
    }
    
    config.MAX_BUFFER_SIZE = 1024  # 1KB
    config.IMAGE_TIMEOUT = 1  # 1秒
    config.IMAGE_DIR = "test_images"  # テスト用ディレクトリ
    
    # グローバル状態をクリア
    image_receiver.image_buffers.clear()
    image_receiver.last_receive_time.clear()
    image_receiver.stats = {"received_images": 0, "total_bytes": 0, "start_time": time.time()}
    
    # テスト用ディレクトリの作成
    os.makedirs(config.IMAGE_DIR, exist_ok=True)
    
    yield
    
    # クリーンアップ
    await image_receiver.cleanup_resources()
    # テスト用ディレクトリの削除
    shutil.rmtree(config.IMAGE_DIR, ignore_errors=True)
    
    # 設定を元に戻す
    for key, value in original_config.items():
        if value is not None:
            setattr(config, key, value)


@patch('protocol.serial_handler.influx_client.write_sensor_data')  # InfluxDB write操作を直接パッチ
@patch('app.serial_asyncio.create_serial_connection')
@patch('app.influxdb_client.InfluxDBClient')
@patch('app.Image')  # PIL Imageもモック化
@patch('protocol.serial_handler.save_image')  # save_image関数を正しいパスでパッチ
class TestSerialProtocolIntegration:

    @pytest.mark.asyncio
    async def test_receive_hash_frame(self, mock_save_image, mock_image, mock_influx_client, mock_serial_connection, mock_write_sensor_data, setup_test_environment):
        # モックオブジェクトの準備
        mock_transport = MagicMock()
        mock_protocol = MagicMock()
        mock_transport.serial = MagicMock(port="test_port")  # serialオブジェクトを設定

        mock_serial_connection.return_value = (mock_transport, mock_protocol)

        # HASHフレームの作成
        mac_bytes = b"\x01\x02\x03\x04\x05\x06"
        sender_mac = "01:02:03:04:05:06"
        seq_num = 1
        payload_str = "HASH:abcdef123456,VOLT:12.3,TEMP:25.5,1678886400"  # タイムスタンプはdummyでOK
        payload_bytes = payload_str.encode('ascii')
        data_len = len(payload_bytes)
        
        frame_bytes = (
            START_MARKER +
            mac_bytes +
            bytes([FRAME_TYPE_HASH]) +
            seq_num.to_bytes(SEQUENCE_NUM_LENGTH, byteorder="big") +
            data_len.to_bytes(LENGTH_FIELD_BYTES, byteorder="big") +
            payload_bytes +
            b'\x00' * CHECKSUM_LENGTH +  # チェックサムはdummyでOK
            END_MARKER
        )

        # プロトコルインスタンス作成
        loop = asyncio.get_running_loop()
        connection_lost_future = loop.create_future()
        image_buffers = {}
        last_receive_time = {}
        stats = {"received_images": 0, "total_bytes": 0, "start_time": 0}
        protocol = SerialProtocol(connection_lost_future, image_buffers, last_receive_time, stats)
        protocol.connection_made(mock_transport)

        # データ受信
        protocol.data_received(frame_bytes)

        # InfluxDBへの書き込みが呼ばれたか確認
        mock_write_sensor_data.assert_called_once()
        args, kwargs = mock_write_sensor_data.call_args
        assert args[0] == sender_mac  # MAC address
        assert args[1] == 12.3        # voltage
        assert args[2] == 25.5        # temperature

    @pytest.mark.asyncio
    async def test_receive_data_and_eof_frames(self, mock_save_image, mock_image, mock_influx_client, mock_serial_connection, mock_write_sensor_data, setup_test_environment):
        # モックオブジェクトの準備
        mock_transport = MagicMock()
        mock_protocol = MagicMock()
        mock_transport.serial = MagicMock(port="test_port")
        mock_serial_connection.return_value = (mock_transport, mock_protocol)

        # データフレームの作成（有効なJPEG画像データをシミュレート）
        mac_bytes = b"\x01\x02\x03\x04\x05\x06"
        sender_mac = "01:02:03:04:05:06"
        seq_num_data = 1
        
        # 有効なJPEGヘッダーとフッターを含む画像データを作成（1000バイト以上）
        jpeg_header = b'\xff\xd8\xff\xe0'  # JPEG SOI + APP0
        jpeg_data = b'\x00' * 1200  # 1000バイト以上のダミーデータ
        jpeg_footer = b'\xff\xd9'  # JPEG EOI
        
        # 3つのチャンクに分割（各チャンクが512バイト以下になるように）
        chunk1_size = 400  # 最大制限内
        chunk2_size = 400  # 最大制限内
        
        data_chunk_1 = (jpeg_header + jpeg_data + jpeg_footer)[:chunk1_size]
        data_len_1 = len(data_chunk_1)
        frame_data_1 = (
            START_MARKER +
            mac_bytes +
            bytes([FRAME_TYPE_DATA]) +
            seq_num_data.to_bytes(SEQUENCE_NUM_LENGTH, byteorder="big") +
            data_len_1.to_bytes(LENGTH_FIELD_BYTES, byteorder="big") +
            data_chunk_1 +
            b'\x00' * CHECKSUM_LENGTH +
            END_MARKER
        )

        data_chunk_2 = (jpeg_header + jpeg_data + jpeg_footer)[chunk1_size:chunk1_size + chunk2_size]
        data_len_2 = len(data_chunk_2)
        frame_data_2 = (
            START_MARKER +
            mac_bytes +
            bytes([FRAME_TYPE_DATA]) +
            (seq_num_data + 1).to_bytes(SEQUENCE_NUM_LENGTH, byteorder="big") +  # シーケンス番号は連続でなくても良い
            data_len_2.to_bytes(LENGTH_FIELD_BYTES, byteorder="big") +
            data_chunk_2 +
            b'\x00' * CHECKSUM_LENGTH +
            END_MARKER
        )

        data_chunk_3 = (jpeg_header + jpeg_data + jpeg_footer)[chunk1_size + chunk2_size:]
        data_len_3 = len(data_chunk_3)
        frame_data_3 = (
            START_MARKER +
            mac_bytes +
            bytes([FRAME_TYPE_DATA]) +
            (seq_num_data + 2).to_bytes(SEQUENCE_NUM_LENGTH, byteorder="big") +
            data_len_3.to_bytes(LENGTH_FIELD_BYTES, byteorder="big") +
            data_chunk_3 +
            b'\x00' * CHECKSUM_LENGTH +
            END_MARKER
        )

        # EOFフレームの作成
        seq_num_eof = 3  # EOFフレームのデータ長は0
        data_len_eof = 0
        frame_eof = (
            START_MARKER +
            mac_bytes +
            bytes([FRAME_TYPE_EOF]) +
            seq_num_eof.to_bytes(SEQUENCE_NUM_LENGTH, byteorder="big") +
            data_len_eof.to_bytes(LENGTH_FIELD_BYTES, byteorder="big") +
            b'' +  # データなし
            b'\x00' * CHECKSUM_LENGTH +
            END_MARKER
        )
        
        # プロトコルインスタンス作成
        loop = asyncio.get_running_loop()
        connection_lost_future = loop.create_future()
        image_buffers = {}
        last_receive_time = {}
        stats = {"received_images": 0, "total_bytes": 0, "start_time": 0}
        protocol = SerialProtocol(connection_lost_future, image_buffers, last_receive_time, stats)
        protocol.connection_made(mock_transport)

        # データ受信
        protocol.data_received(frame_data_1)
        protocol.data_received(frame_data_2)
        protocol.data_received(frame_data_3)
        protocol.data_received(frame_eof)

        # save_image関数が呼ばれたか確認
        mock_save_image.assert_called_once()
        args, _ = mock_save_image.call_args
        assert args[0] == sender_mac
        assert args[1] == data_chunk_1 + data_chunk_2 + data_chunk_3
        
        # バッファがクリアされたか確認
        assert sender_mac not in protocol.image_buffers
        assert sender_mac not in protocol.last_receive_time
