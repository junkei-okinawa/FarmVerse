#!/bin/bash

# ホストマシンでのユニットテスト実行スクリプト
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
if [ $? -eq 0 ]; then
    echo "Running voltage_calc tests..."
    ../../target/voltage_tests
    VOLTAGE_RESULT=$?
    echo ""
else
    echo "❌ コンパイルエラー"
    exit 1
fi

# MACアドレスモジュールのテスト
echo "🧪 MACアドレス処理のテスト..."
cd ../
echo "Compiling mac_address tests..."
rustc +stable --test mac_address.rs --edition 2021 -o ../target/mac_tests
if [ $? -eq 0 ]; then
    echo "Running mac_address tests..."
    ../target/mac_tests
    MAC_RESULT=$?
    echo ""
else
    echo "❌ コンパイルエラー"
    exit 1
fi

echo "================================"
if [ $VOLTAGE_RESULT -eq 0 ] && [ $MAC_RESULT -eq 0 ]; then
    echo "✅ すべてのテスト完了"
    echo "================================"
    exit 0
else
    echo "❌ テスト失敗"
    echo "================================"
    exit 1
fi
