#!/bin/bash

# ホストマシンでのユニットテスト実行スクリプト

echo "================================"
echo "ホストユニットテスト実行"
echo "================================"
echo ""

echo "📝 テスト対象:"
echo "  - utils::voltage_calc (電圧計算)"
echo "  - mac_address (MACアドレス処理)"
echo ""

# utilsモジュールのテスト（ハードウェア非依存）
echo "🧪 電圧計算ロジックのテスト..."
cd src/utils
rustc --test voltage_calc.rs --edition 2021 -o ../../target/voltage_tests 2>/dev/null
if [ $? -eq 0 ]; then
    ../../target/voltage_tests
    echo ""
else
    echo "❌ コンパイルエラー"
    echo ""
fi

# MACアドレスモジュールのテスト
echo "🧪 MACアドレス処理のテスト..."
cd ../
rustc --test mac_address.rs --edition 2021 -o ../target/mac_tests 2>/dev/null
if [ $? -eq 0 ]; then
    ../target/mac_tests
    echo ""
else
    echo "❌ コンパイルエラー"
    echo ""
fi

echo "================================"
echo "✅ テスト完了"
echo "================================"
