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
echo "  - mac_address (MACアドレス処理)"
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

# MACアドレスモジュールのテスト
echo "🧪 MACアドレス処理のテスト..."
cd ../
echo "Compiling mac_address tests..."
rustc +stable --test mac_address.rs --edition 2021 -o ../target/mac_tests
echo "Running mac_address tests..."
../target/mac_tests
echo ""

echo "================================"
echo "✅ すべてのテスト完了"
echo "================================"
