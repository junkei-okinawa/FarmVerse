# ホストマシンユニットテスト

ESP32-S3実機を使用せず、ホストマシン（Mac）でユニットテストを実行する方法

---

## 概要

ハードウェア非依存のロジックを純粋関数として分離し、ホストマシンでテストします。

### テスト対象モジュール

| モジュール | ファイル | テスト内容 | テスト数 |
|----------|---------|-----------|---------|
| utils::voltage_calc | `src/utils/voltage_calc.rs` | 電圧パーセンテージ計算 | 10 |
| mac_address | `src/mac_address.rs` | MACアドレス処理 | 13 |

---

## テストの実行

### 方法1: テストスクリプトを使用（推奨）

```bash
./run_tests.sh
```

**出力例:**
```
================================
ホストユニットテスト実行
================================

🧪 電圧計算ロジックのテスト...
test result: ok. 10 passed; 0 failed; 0 ignored

🧪 MACアドレス処理のテスト...
test result: ok. 13 passed; 0 failed; 0 ignored

================================
✅ テスト完了
================================
```

###方法2: 個別にテスト実行

```bash
# 電圧計算のテスト
cd src/utils
rustc --test voltage_calc.rs --edition 2021 -o ../../target/voltage_tests
../../target/voltage_tests

# MACアドレスのテスト
cd src
rustc --test mac_address.rs --edition 2021 -o ../target/mac_tests
../target/mac_tests
```

---

## テストケース詳細

### 1. 電圧計算（`utils::voltage_calc`）

#### テストケース一覧

| テスト名 | 説明 | 入力 | 期待出力 |
|---------|------|------|---------|
| test_voltage_percentage_50_percent | 中間値 | 1629mV (128-3130) | 50% |
| test_voltage_percentage_0_percent | 最小値 | 128mV | 0% |
| test_voltage_percentage_100_percent | 最大値 | 3130mV | 100% |
| test_voltage_percentage_below_minimum | 最小値以下 | 50mV | 0% (クランプ) |
| test_voltage_percentage_above_maximum | 最大値以上 | 3500mV | 100% (クランプ) |
| test_voltage_percentage_invalid_range | 無効な範囲 | max < min | 0% |
| test_voltage_percentage_zero_range | 範囲ゼロ | min == max | 0% |
| test_voltage_percentage_realistic_2000mv | 実用例1 | 2000mV | 62% |
| test_voltage_percentage_realistic_500mv | 実用例2 | 500mV | 12% |
| test_voltage_percentage_realistic_2500mv | 実用例3 | 2500mV | 79% |

#### 計算ロジック

```rust
pub fn calculate_voltage_percentage(voltage_mv: f32, min_mv: f32, max_mv: f32) -> u8 {
    let range_mv = max_mv - min_mv;
    
    let percentage = if range_mv <= 0.0 {
        0.0
    } else {
        ((voltage_mv - min_mv) / range_mv * 100.0)
            .max(0.0)  // 0%以下をクランプ
            .min(100.0) // 100%以上をクランプ
    };
    
    percentage.round() as u8
}
```

---

### 2. MACアドレス処理（`mac_address`）

#### テストケース一覧

| テスト名 | 説明 | 期待結果 |
|---------|------|---------|
| test_mac_address_from_str | 基本パース | 成功 |
| test_mac_address_from_str_lowercase | 小文字16進数 | 成功 |
| test_mac_address_from_str_uppercase | 大文字16進数 | 成功 |
| test_mac_address_from_str_mixed_case | 混在16進数 | 成功 |
| test_mac_address_from_str_invalid_format | 不正フォーマット | エラー |
| test_mac_address_from_str_too_many_parts | パーツ過多 | エラー |
| test_mac_address_from_str_invalid_hex | 無効16進数 | エラー |
| test_mac_address_new | バイト配列から生成 | 成功 |
| test_mac_address_display | フォーマット出力 | `xx:xx:xx:xx:xx:xx` |
| test_mac_address_display_lowercase | 小文字出力 | `aa:bb:cc:dd:ee:ff` |
| test_mac_address_roundtrip | パース→表示往復 | 一致 |
| test_mac_address_zero | ゼロアドレス | `00:00:00:00:00:00` |
| test_mac_address_all_ff | ブロードキャスト | `ff:ff:ff:ff:ff:ff` |

---

## CI/CD統合

### GitHub Actions設定例

```yaml
# .github/workflows/unit_tests.yml
name: Host Unit Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v3
      - name: Run unit tests
        run: |
          cd devices/xiao_esp32s3_sense
          chmod +x run_tests.sh
          ./run_tests.sh
```

---

## テストカバレッジ

### 現在のカバレッジ

| コンポーネント | テスト済み | カバレッジ推定 |
|--------------|----------|--------------|
| 電圧計算ロジック | ✅ | 95%+ |
| MACアドレス処理 | ✅ | 95%+ |
| データサービス | �� 計画中 | - |
| ESP-NOWフレーム構築 | 🚧 計画中 | - |
| 設定パース | 🚧 計画中 | - |

### 目標カバレッジ

- ✅ Phase 1完了: 基礎ユーティリティ（電圧計算、MACアドレス） - **達成**
- 🎯 Phase 2: データ構造とフォーマット（`MeasuredData`） - **次回**
- 🎯 Phase 3: ビジネスロジック（設定、フレーム構築） - **計画中**

---

## トラブルシューティング

### Q: `cargo test`でESP-IDFビルドエラーが発生する

**A:** 現在、ESP-IDF依存により`cargo test`は直接使用できません。`run_tests.sh`スクリプトを使用してください。

### Q: テストが失敗する

**A:** 以下を確認:
1. Rustツールチェーンが最新か (`rustc --version`)
2. ファイルパスが正しいか
3. エラーメッセージを確認

### Q: 新しいテストを追加したい

**A:** 以下の手順:
1. モジュールファイルに`#[cfg(test)] mod tests { ... }`を追加
2. `#[test]`アトリビュートを付けた関数を作成
3. `run_tests.sh`を実行して確認

---

## 次のステップ

1. ✅ Phase 1完了（電圧計算、MACアドレス）
2. **Phase 2**: `MeasuredData`のテスト追加
   - `get_summary()`メソッド
   - ビルダーパターン
3. **Phase 3**: ESP-NOWフレーム構築のテスト
4. **Phase 4**: CI/CD統合

---

**最終更新**: 2025-11-02  
**テスト数**: 23  
**カバレッジ**: 基礎ユーティリティ 95%+
