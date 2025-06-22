#!/usr/bin/env python3
"""
ESP-NOW スリープコマンドテストスクリプト

このスクリプトは、サーバーからUSBゲートウェイに対して
スリープコマンドを送信し、M5Stack Unit Camに到達するかをテストします。
"""

import time
import serial
import sys
import os

def test_sleep_command_transmission():
    """スリープコマンドの送信テスト"""
    
    # M5Stack Unit CamのMACアドレス
    target_mac = "34:ab:95:fb:3f:c4"
    sleep_duration = 60  # 60秒
    
    # USBゲートウェイに送信するコマンド
    command = f"CMD_SEND_ESP_NOW:{target_mac}:{sleep_duration}\n"
    
    print("=== ESP-NOW スリープコマンド送信テスト ===")
    print(f"対象デバイス: {target_mac}")
    print(f"スリープ時間: {sleep_duration}秒")
    print(f"送信コマンド: {repr(command)}")
    
    # シリアルポートを検索（macOS用）
    possible_ports = [
        "/dev/tty.usbserial-*",
        "/dev/tty.usbmodem*",
        "/dev/cu.usbserial-*",
        "/dev/cu.usbmodem*"
    ]
    
    print("\n利用可能なシリアルポートを検索中...")
    import glob
    found_ports = []
    for pattern in possible_ports:
        found_ports.extend(glob.glob(pattern))
    
    print(f"見つかったポート: {found_ports}")
    
    if not found_ports:
        print("ERROR: USBシリアルポートが見つかりません")
        print("USBゲートウェイが接続されているか確認してください")
        return False
    
    # 最初に見つかったポートを使用
    port = found_ports[0]
    print(f"使用するポート: {port}")
    
    try:
        # シリアル接続を開く
        print("シリアル接続を開いています...")
        ser = serial.Serial(port, 115200, timeout=1)
        time.sleep(2)  # 接続安定待機
        
        print("コマンドを送信中...")
        ser.write(command.encode('utf-8'))
        ser.flush()
        
        print("送信完了。応答を待機中...")
        
        # 応答を待機（最大10秒）
        start_time = time.time()
        while time.time() - start_time < 10:
            if ser.in_waiting > 0:
                response = ser.readline().decode('utf-8', errors='ignore').strip()
                if response:
                    print(f"応答: {response}")
            time.sleep(0.1)
        
        ser.close()
        print("テスト完了")
        
        return True
        
    except serial.SerialException as e:
        print(f"シリアル通信エラー: {e}")
        return False
    except Exception as e:
        print(f"予期しないエラー: {e}")
        return False

if __name__ == "__main__":
    success = test_sleep_command_transmission()
    sys.exit(0 if success else 1)
