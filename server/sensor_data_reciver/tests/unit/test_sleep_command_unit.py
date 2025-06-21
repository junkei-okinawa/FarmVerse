import os
import sys

import pytest

# テストファイルから見た app.py への正しいパス
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', '..'))

from app import format_sleep_command_to_gateway, config


class TestSleepCommandFormatting:
    """スリープコマンドフォーマット機能のユニットテスト"""
    
    def test_format_sleep_command_basic(self):
        """基本的なスリープコマンドフォーマットのテスト"""
        sender_mac = "aa:bb:cc:dd:ee:ff"
        sleep_duration = 60
        
        result = format_sleep_command_to_gateway(sender_mac, sleep_duration)
        expected = "CMD_SEND_ESP_NOW:aa:bb:cc:dd:ee:ff:60\n"
        
        assert result == expected
    
    def test_format_sleep_command_different_durations(self):
        """異なるスリープ時間でのフォーマットテスト"""
        test_cases = [
            ("aa:bb:cc:dd:ee:ff", 30, "CMD_SEND_ESP_NOW:aa:bb:cc:dd:ee:ff:30\n"),
            ("11:22:33:44:55:66", 120, "CMD_SEND_ESP_NOW:11:22:33:44:55:66:120\n"),
            ("ff:ee:dd:cc:bb:aa", 300, "CMD_SEND_ESP_NOW:ff:ee:dd:cc:bb:aa:300\n"),
        ]
        
        for mac, duration, expected in test_cases:
            result = format_sleep_command_to_gateway(mac, duration)
            assert result == expected
    
    def test_format_sleep_command_edge_cases(self):
        """エッジケースのテスト"""
        # 最小値
        result = format_sleep_command_to_gateway("00:00:00:00:00:00", 0)
        expected = "CMD_SEND_ESP_NOW:00:00:00:00:00:00:0\n"
        assert result == expected
        
        # 大きな値
        result = format_sleep_command_to_gateway("ff:ff:ff:ff:ff:ff", 86400)  # 24時間
        expected = "CMD_SEND_ESP_NOW:ff:ff:ff:ff:ff:ff:86400\n"
        assert result == expected
    
    def test_format_sleep_command_default_duration(self):
        """デフォルトのスリープ時間を使用するテスト"""
        sender_mac = "aa:bb:cc:dd:ee:ff"
        default_duration = config.DEFAULT_SLEEP_DURATION_S
        
        result = format_sleep_command_to_gateway(sender_mac, default_duration)
        expected = f"CMD_SEND_ESP_NOW:{sender_mac}:{default_duration}\n"
        
        assert result == expected
    
    def test_sleep_command_format_contains_newline(self):
        """スリープコマンドが改行文字で終わることのテスト"""
        result = format_sleep_command_to_gateway("aa:bb:cc:dd:ee:ff", 60)
        assert result.endswith("\n")
    
    def test_sleep_command_format_structure(self):
        """スリープコマンドの構造が正しいことのテスト"""
        sender_mac = "aa:bb:cc:dd:ee:ff"
        duration = 120
        
        result = format_sleep_command_to_gateway(sender_mac, duration)
        
        # 改行を除去して構造をチェック
        command_parts = result.strip().split(":")
        
        assert len(command_parts) == 8  # CMD_SEND_ESP_NOW + MAC(6部分) + duration
        assert command_parts[0] == "CMD_SEND_ESP_NOW"
        assert command_parts[-1] == str(duration)
        
        # MACアドレス部分のチェック
        mac_parts = command_parts[1:7]  # インデックス1-6がMACアドレス
        expected_mac_parts = sender_mac.split(":")
        assert mac_parts == expected_mac_parts


class TestConfigValues:
    """設定値のテスト"""
    
    def test_default_sleep_duration_exists(self):
        """DEFAULT_SLEEP_DURATION_Sが設定されていることのテスト"""
        assert hasattr(config, 'DEFAULT_SLEEP_DURATION_S')
        assert isinstance(config.DEFAULT_SLEEP_DURATION_S, int)
        assert config.DEFAULT_SLEEP_DURATION_S > 0
    
    def test_default_sleep_duration_reasonable_value(self):
        """DEFAULT_SLEEP_DURATION_Sが妥当な値であることのテスト"""
        # 30秒から24時間の範囲であることを確認
        assert 30 <= config.DEFAULT_SLEEP_DURATION_S <= 86400


if __name__ == "__main__":
    pytest.main([__file__])
