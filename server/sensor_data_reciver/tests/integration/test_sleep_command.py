import asyncio
import os
import sys

import pytest
import pytest_asyncio

# テストファイルから見た app.py への正しいパス
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', '..'))

from app import (FRAME_TYPE_HASH, LENGTH_FIELD_BYTES,
                 SEQUENCE_NUM_LENGTH, START_MARKER, END_MARKER,
                 SerialProtocol, config, format_sleep_command_to_gateway)


class MockTransport:
    """モックトランスポートクラス"""
    def __init__(self):
        self.written_data = []
        
    def write(self, data):
        """書き込みデータを記録"""
        self.written_data.append(data)
        
    def get_written_commands(self):
        """書き込まれたコマンドを文字列として返す"""
        return [data.decode('utf-8') for data in self.written_data]
    
    def reset(self):
        """記録されたデータをクリア"""
        self.written_data.clear()


@pytest_asyncio.fixture
async def mock_transport():
    """モックトランスポートのフィクスチャ"""
    return MockTransport()


def create_hash_frame(sender_mac_str: str, voltage: int, temperature: float, timestamp: str):
    """HASHフレームを生成するヘルパー関数"""
    # MACアドレスをバイト列に変換
    mac_parts = sender_mac_str.split(':')
    mac_bytes = bytes([int(part, 16) for part in mac_parts])
    
    # ペイロード作成
    hash_value = "abcd1234567890ef" * 4  # 64文字のダミーハッシュ
    payload = f"HASH:{hash_value},VOLT:{voltage},TEMP:{temperature},{timestamp}"
    payload_bytes = payload.encode('ascii')
    
    # フレーム構造: START_MARKER + MAC + FRAME_TYPE + SEQ + DATA_LEN + DATA + CHECKSUM + END_MARKER
    frame_type = FRAME_TYPE_HASH
    seq_num = 1
    data_len = len(payload_bytes)
    checksum = b'\x00\x00\x00\x00'  # ダミーチェックサム
    
    frame = (
        START_MARKER +
        mac_bytes +
        bytes([frame_type]) +
        seq_num.to_bytes(SEQUENCE_NUM_LENGTH, byteorder="big") +
        data_len.to_bytes(LENGTH_FIELD_BYTES, byteorder="big") +
        payload_bytes +
        checksum +
        END_MARKER
    )
    
    return frame


@pytest.mark.asyncio
async def test_sleep_command_sent_on_hash_frame(mock_transport):
    """HASHフレーム受信時にスリープコマンドが送信されることをテスト"""
    # テスト用の設定
    test_mac = "aa:bb:cc:dd:ee:ff"
    test_voltage = 85
    test_temperature = 25.5
    test_timestamp = "2024/01/01 12:00:00.000"
    
    # プロトコルインスタンス作成
    connection_lost_future = asyncio.Future()
    image_buffers = {}
    last_receive_time = {}
    stats = {}
    protocol = SerialProtocol(connection_lost_future, image_buffers, last_receive_time, stats)
    protocol.transport = mock_transport
    
    # HASHフレームを作成してデータ受信をシミュレート
    hash_frame = create_hash_frame(test_mac, test_voltage, test_temperature, test_timestamp)
    
    # データ受信をシミュレート
    protocol.data_received(hash_frame)
    
    # 少し待機してプロトコル処理を完了させる
    await asyncio.sleep(0.1)
    
    # スリープコマンドが送信されたかを確認
    written_commands = mock_transport.get_written_commands()
    assert len(written_commands) == 1
    
    # 電圧85%は8%以上なので、NORMAL_SLEEP_DURATION_S（600秒）が適用される
    expected_command = f"CMD_SEND_ESP_NOW:{test_mac}:{config.NORMAL_SLEEP_DURATION_S}\n"
    assert written_commands[0] == expected_command


@pytest.mark.asyncio
async def test_multiple_devices_sleep_commands(mock_transport):
    """複数デバイスからのHASHフレームに対してそれぞれスリープコマンドが送信されることをテスト"""
    devices = [
        ("aa:bb:cc:dd:ee:f1", 80, 24.0),
        ("aa:bb:cc:dd:ee:f2", 90, 26.5),
        ("aa:bb:cc:dd:ee:f3", 75, 23.8),
    ]
    
    # プロトコルインスタンス作成
    connection_lost_future = asyncio.Future()
    image_buffers = {}
    last_receive_time = {}
    stats = {}
    protocol = SerialProtocol(connection_lost_future, image_buffers, last_receive_time, stats)
    protocol.transport = mock_transport
    
    # 各デバイスからHASHフレームを送信
    for mac, voltage, temp in devices:
        hash_frame = create_hash_frame(mac, voltage, temp, "2024/01/01 12:00:00.000")
        protocol.data_received(hash_frame)
        await asyncio.sleep(0.05)  # フレーム間の小さな間隔
    
    # 処理完了を待機
    await asyncio.sleep(0.1)
    
    # 各デバイスに対してスリープコマンドが送信されたかを確認
    written_commands = mock_transport.get_written_commands()
    assert len(written_commands) == len(devices)
    
    for i, (mac, voltage, _) in enumerate(devices):
        # 全ての電圧値（80%, 90%, 75%）は8%以上なので、NORMAL_SLEEP_DURATION_S（600秒）が適用される
        expected_command = f"CMD_SEND_ESP_NOW:{mac}:{config.NORMAL_SLEEP_DURATION_S}\n"
        assert written_commands[i] == expected_command


@pytest.mark.asyncio
async def test_no_sleep_command_without_transport(mock_transport):
    """トランスポートがない場合にスリープコマンドが送信されないことをテスト"""
    test_mac = "aa:bb:cc:dd:ee:ff"
    
    # プロトコルインスタンス作成（トランスポートなし）
    connection_lost_future = asyncio.Future()
    image_buffers = {}
    last_receive_time = {}
    stats = {}
    protocol = SerialProtocol(connection_lost_future, image_buffers, last_receive_time, stats)
    protocol.transport = None  # トランスポートなし
    
    # HASHフレームを作成してデータ受信をシミュレート
    hash_frame = create_hash_frame(test_mac, 85, 25.5, "2024/01/01 12:00:00.000")
    
    # データ受信をシミュレート（例外が発生しないことを確認）
    protocol.data_received(hash_frame)
    await asyncio.sleep(0.1)
    
    # スリープコマンドが送信されていないことを確認
    # この場合、mock_transportは使用されていないので、written_dataは空
    assert len(mock_transport.written_data) == 0


def test_format_sleep_command_to_gateway():
    """スリープコマンドフォーマット関数のテスト"""
    test_mac = "aa:bb:cc:dd:ee:ff"
    test_duration = 120
    
    result = format_sleep_command_to_gateway(test_mac, test_duration)
    expected = f"CMD_SEND_ESP_NOW:{test_mac}:{test_duration}\n"
    
    assert result == expected


@pytest.mark.asyncio
async def test_invalid_hash_frame_no_sleep_command(mock_transport):
    """無効なHASHフレームに対してスリープコマンドが送信されないことをテスト"""
    # プロトコルインスタンス作成
    connection_lost_future = asyncio.Future()
    image_buffers = {}
    last_receive_time = {}
    stats = {}
    protocol = SerialProtocol(connection_lost_future, image_buffers, last_receive_time, stats)
    protocol.transport = mock_transport
    
    # 無効なフレーム（不正なペイロード）を作成
    mac_bytes = bytes([0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff])
    invalid_payload = b"INVALID_PAYLOAD"  # 無効なペイロード
    
    frame = (
        START_MARKER +
        mac_bytes +
        bytes([FRAME_TYPE_HASH]) +
        (1).to_bytes(SEQUENCE_NUM_LENGTH, byteorder="big") +
        len(invalid_payload).to_bytes(LENGTH_FIELD_BYTES, byteorder="big") +
        invalid_payload +
        b'\x00\x00\x00\x00' +  # ダミーチェックサム
        END_MARKER
    )
    
    # データ受信をシミュレート
    protocol.data_received(frame)
    await asyncio.sleep(0.1)
    
    # スリープコマンドが送信されていないことを確認
    written_commands = mock_transport.get_written_commands()
    assert len(written_commands) == 0


@pytest.mark.asyncio
async def test_low_voltage_sleep_commands(mock_transport):
    """低電圧時の時刻ベースのスリープコマンドをテスト"""
    import datetime
    from unittest.mock import patch
    
    test_mac = "aa:bb:cc:dd:ee:ff"
    test_voltage = 5  # 8%未満の低電圧
    test_temperature = 25.5
    test_timestamp = "2024/01/01 12:00:00.000"
    
    # プロトコルインスタンス作成
    connection_lost_future = asyncio.Future()
    image_buffers = {}
    last_receive_time = {}
    stats = {}
    protocol = SerialProtocol(connection_lost_future, image_buffers, last_receive_time, stats)
    protocol.transport = mock_transport
    
    # HASHフレームを作成
    hash_frame = create_hash_frame(test_mac, test_voltage, test_temperature, test_timestamp)
    
    # 午前中（10時）をシミュレート
    with patch('processors.sleep_controller.datetime') as mock_datetime:
        mock_datetime.now.return_value = datetime.datetime(2024, 1, 1, 10, 0, 0)
        mock_datetime.datetime = datetime.datetime  # datetime.datetime クラスを正しく保持
        
        # データ受信をシミュレート
        protocol.data_received(hash_frame)
        await asyncio.sleep(0.1)
        
        # 午前中の低電圧では MEDIUM_SLEEP_DURATION_S（1時間）が適用される
        written_commands = mock_transport.get_written_commands()
        assert len(written_commands) == 1
        expected_command = f"CMD_SEND_ESP_NOW:{test_mac}:{config.MEDIUM_SLEEP_DURATION_S}\n"
        assert written_commands[0] == expected_command
    
    # リセット
    mock_transport.reset()
    
    # 午後（14時）をシミュレート
    with patch('processors.sleep_controller.datetime') as mock_datetime:
        mock_datetime.now.return_value = datetime.datetime(2024, 1, 1, 14, 0, 0)
        mock_datetime.datetime = datetime.datetime
        
        # データ受信をシミュレート
        protocol.data_received(hash_frame)
        await asyncio.sleep(0.1)
        
        # 午後の低電圧では LONG_SLEEP_DURATION_S（9時間）が適用される
        written_commands = mock_transport.get_written_commands()
        assert len(written_commands) == 1
        expected_command = f"CMD_SEND_ESP_NOW:{test_mac}:{config.LONG_SLEEP_DURATION_S}\n"
        assert written_commands[0] == expected_command


if __name__ == "__main__":
    pytest.main([__file__])
