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
echo "  - mac_address (MACアドレス処理)"
echo "  - core::measured_data (測定データ)"
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

echo "================================"
echo "✅ すべてのテスト完了"
echo "================================"
