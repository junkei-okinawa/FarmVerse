import asyncio
import os
import sys
import time
from unittest.mock import MagicMock, patch

import pytest
import pytest_asyncio

# テストファイルから見た app.py への正しいパス
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', '..'))

from app import ImageReceiver, config, save_image


@pytest_asyncio.fixture
async def receiver_instance():
    receiver = ImageReceiver()
    # テスト用に設定を調整してメモリテストを容易にする
    config.MAX_BUFFER_SIZE = 1024 # 1KB
    config.IMAGE_TIMEOUT = 1 # 1秒
    yield receiver
    await receiver.cleanup_resources()

@pytest.mark.asyncio
async def test_check_memory_usage_ok(receiver_instance):
    mac1 = "01:02:03:04:05:06"
    mac2 = "07:08:09:0a:0b:0c"
    receiver_instance.image_buffers[mac1] = bytearray(b'a' * 500)
    receiver_instance.image_buffers[mac2] = bytearray(b'b' * 400)
    receiver_instance.last_receive_time[mac1] = time.monotonic()
    receiver_instance.last_receive_time[mac2] = time.monotonic()

    receiver_instance.check_memory_usage()

    assert len(receiver_instance.image_buffers) == 2

@pytest.mark.asyncio
async def test_check_memory_usage_cleanup(receiver_instance):
    mac1 = "01:02:03:04:05:06"
    mac2 = "07:08:09:0a:0b:0c"
    receiver_instance.image_buffers[mac1] = bytearray(b'a' * 600)
    receiver_instance.image_buffers[mac2] = bytearray(b'b' * 600)
    receiver_instance.last_receive_time[mac1] = time.monotonic()
    receiver_instance.last_receive_time[mac2] = time.monotonic() - 10 # mac2が古い

    receiver_instance.check_memory_usage()

    assert len(receiver_instance.image_buffers) == 1
    assert mac1 in receiver_instance.image_buffers
    assert mac2 not in receiver_instance.image_buffers

@pytest.mark.asyncio
async def test_cleanup_buffer(receiver_instance):
    mac1 = "01:02:03:04:05:06"
    mac2 = "07:08:09:0a:0b:0c"
    receiver_instance.image_buffers[mac1] = bytearray(b'a' * 100)
    receiver_instance.image_buffers[mac2] = bytearray(b'b' * 100)
    receiver_instance.last_receive_time[mac1] = time.monotonic()
    receiver_instance.last_receive_time[mac2] = time.monotonic()

    receiver_instance._cleanup_buffer(mac1)

    assert mac1 not in receiver_instance.image_buffers
    assert mac1 not in receiver_instance.last_receive_time
    assert mac2 in receiver_instance.image_buffers
    assert mac2 in receiver_instance.last_receive_time

@patch('processors.image_processor.write_file_sync')
@pytest.mark.asyncio
async def test_save_image(mock_write_file_sync):
    mac_str = "01:02:03:04:05:06"
    image_data = b'\x00' * 1024
    
    await save_image(mac_str, image_data)

    mock_write_file_sync.assert_called_once()
    args, _ = mock_write_file_sync.call_args
    filename = args[0]
    assert args[1] == image_data
    # ファイル名に関する基本的なチェックを追加
    assert mac_str.replace(':', '') in filename
    assert filename.endswith(".jpg")
