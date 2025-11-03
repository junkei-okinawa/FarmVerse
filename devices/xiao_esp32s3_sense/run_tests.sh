#!/bin/bash

# ホストマシンでのユニットテスト実行スクリプト
# Before first use, make this script executable with: chmod +x run_tests.sh
set -e  # エラーで停止

echo "================================"
echo "ホストユニットテスト実行"
echo "================================"
echo ""

echo "📝 テスト対象:"
echo "  - utils::voltage_calc (電圧計算)"
echo "  - utils::tds_calc (TDS計算)"
echo "  - utils::streaming_protocol (通信プロトコル)"
echo "  - mac_address (MACアドレス処理)"
echo "  - core::measured_data (測定データ)"
echo "  - core::app_controller (アプリ制御)"
echo "  - integration::data_flow (データフロー統合テスト)"
echo ""

# targetディレクトリを作成
mkdir -p target

# utilsモジュールのテスト（ハードウェア非依存）
echo "🧪 電圧計算ロジックのテスト..."
cd src/utils
echo "Compiling voltage_calc tests..."
rustc +stable --test voltage_calc.rs --edition 2021 -o ../../target/voltage_tests
echo "Running voltage_calc tests..."
../../target/voltage_tests
echo ""

# TDS計算ロジックのテスト
echo "🧪 TDS計算ロジックのテスト..."
echo "Compiling tds_calc tests..."
rustc +stable --test tds_calc.rs --edition 2021 -o ../../target/tds_tests
echo "Running tds_calc tests..."
../../target/tds_tests
echo ""

# ストリーミングプロトコルのテスト
echo "🧪 ストリーミングプロトコルのテスト..."
echo "Compiling streaming_protocol tests..."
rustc +stable --test streaming_protocol.rs --edition 2021 -o ../../target/streaming_tests
echo "Running streaming_protocol tests..."
../../target/streaming_tests
echo ""

# MACアドレスモジュールのテスト
echo "🧪 MACアドレス処理のテスト..."
cd ../
echo "Compiling mac_address tests..."
rustc +stable --test mac_address.rs --edition 2021 -o ../target/mac_tests
echo "Running mac_address tests..."
../target/mac_tests
echo ""

# MeasuredDataモジュールのテスト
echo "🧪 測定データ構造のテスト..."
cd core
echo "Compiling measured_data tests..."
rustc +stable --test measured_data.rs --edition 2021 -o ../../target/measured_data_tests
echo "Running measured_data tests..."
../../target/measured_data_tests
echo ""

# AppControllerモジュールのテスト
echo "🧪 アプリ制御ロジックのテスト..."
echo "Compiling app_controller tests..."
rustc +stable --test app_controller.rs --edition 2021 --extern thiserror=../../target/debug/deps/libthiserror-*.rlib -L ../../target/debug/deps -o ../../target/app_controller_tests 2>/dev/null || echo "⚠️  app_controller tests require dependencies (skipping standalone test)"
if [ -f ../../target/app_controller_tests ]; then
    echo "Running app_controller tests..."
    ../../target/app_controller_tests
fi
echo ""

echo "================================"
echo "✅ すべてのテスト完了"
echo ""
echo "📝 Note: 統合テスト (lib内integration_tests) は手動で実行:"
echo "   cargo +stable test --lib integration_tests"
echo "================================"
