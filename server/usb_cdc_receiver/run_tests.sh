#!/bin/bash

# USB CDC Receiver ホストマシンユニットテスト実行スクリプト
# Before first use, make this script executable with: chmod +x run_tests.sh
set -e  # エラーで停止

echo "================================"
echo "USB CDC Receiver ユニットテスト実行"
echo "================================"
echo ""

echo "📝 テスト対象:"
echo "  - esp_now::frame (ESP-NOWフレームパーサー)"
echo "  - esp_now::mac_address (MACアドレス処理)"
echo "  - command::parser (コマンドパーサー)"
echo "  - usb::mock (USB CDCモック統合テスト)"
echo ""

# ホストアーキテクチャを検出
HOST_TARGET=$(rustc --version --verbose | grep host | awk '{print $2}')
echo "🖥️  ホストターゲット: $HOST_TARGET"
echo ""

# stable toolchainでテストを実行（.cargo/config.tomlを変更せずに実行）
# --targetオプションでホストターゲットを明示的に指定
echo "🧪 すべてのユニットテスト実行..."
RUST_BACKTRACE=1 cargo +stable test --lib --tests --target "$HOST_TARGET" --no-default-features

echo ""
echo "================================"
echo "✅ すべてのテスト完了"
echo "================================"
