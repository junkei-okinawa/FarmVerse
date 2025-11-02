# ホストマシンユニットテスト

ESP32-S3実機を使用せず、ホストマシン（Mac）でユニットテストを実行する方法

---

## 概要

ハードウェア非依存のロジックを純粋関数として分離し、ホストマシンでテストします。

### テスト対象モジュール

| モジュール | ファイル | テスト内容 | テスト数 |
|----------|---------|-----------|---------|
| utils::voltage_calc | `src/utils/voltage_calc.rs` | 電圧パーセンテージ計算 | 10 |
| utils::tds_calc | `src/utils/tds_calc.rs` | TDS/EC計算 | 23 |
| mac_address | `src/mac_address.rs` | MACアドレス処理 | 13 |
| core::measured_data | `src/core/measured_data.rs` | 測定データ構造 | 18 |

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

🧪 TDS計算ロジックのテスト...
test result: ok. 23 passed; 0 failed; 0 ignored

🧪 MACアドレス処理のテスト...
test result: ok. 13 passed; 0 failed; 0 ignored

🧪 測定データ構造のテスト...
test result: ok. 18 passed; 0 failed; 0 ignored

================================
✅ テスト完了
================================
```

### 方法2: 個別にテスト実行

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

### 2. TDS計算（`utils::tds_calc`）

#### テストケース概要

**ECからTDS計算 (8テスト)**
- 標準変換、ゼロ値、高/低係数、負値処理、実用値テスト

**温度補正 (6テスト)**  
- 同温度、高温/低温補正、負値、極端温度、ゼロ係数

**ADCからEC計算 (7テスト)**
- 校正一致、高/低値、ゼロ処理、実用範囲

**統合テスト (2テスト)**
- 完全な計算パイプライン、境界値

#### 主要な計算式

```rust
// TDS (ppm) = EC (μS/cm) × TDS Factor / 1000
TDS = EC × Factor / 1000

// 温度補正: EC_25℃ = EC_raw / (1 + coefficient × (T - 25))
EC_compensated = EC / (1 + 0.02 × (temp - 25))

// ADC線形補正: EC = (ADC値 / 校正ADC) × 校正EC  
EC = (ADC / calibrate_ADC) × calibrate_EC
```

---

### 3. MACアドレス処理（`mac_address`）

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

### 4. 測定データ構造（`core::measured_data`）

#### テストケース一覧

| テスト名 | 説明 | 期待結果 |
|---------|------|---------|
| test_new_creates_minimal_data | 最小データ生成 | デフォルト値確認 |
| test_new_with_image_data | 画像データ付き生成 | 画像データ保持 |
| test_builder_pattern_with_temperature | 温度データ追加 | ビルダーパターン |
| test_builder_pattern_with_tds_voltage | TDS電圧追加 | ビルダーパターン |
| test_builder_pattern_with_tds | TDS値追加 | ビルダーパターン |
| test_builder_pattern_chaining | メソッドチェーン | 全データ設定 |
| test_add_warning | 警告追加 | 警告リスト更新 |
| test_get_summary_minimal | 最小サマリ | 電圧のみ表示 |
| test_get_summary_with_temperature | 温度付きサマリ | 温度表示 |
| test_get_summary_with_tds_voltage | TDS電圧付きサマリ | 電圧表示 |
| test_get_summary_with_tds | TDS付きサマリ | TDS表示 |
| test_get_summary_with_image | 画像付きサマリ | バイト数表示 |
| test_get_summary_with_warnings | 警告付きサマリ | 警告件数表示 |
| test_get_summary_full | 完全サマリ | 全項目表示 |
| test_voltage_boundary_values | 電圧境界値 | 0%, 100%正常 |
| test_temperature_negative | 負の温度 | マイナス温度対応 |
| test_empty_image_data | 空画像データ | 0bytes表示 |
| test_clone | クローン機能 | データコピー |

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

**最終更新**: 2024-11-02  
**テスト数**: 64 (Phase 2 Step 2)  
**カバレッジ**: 基礎ユーティリティ・データ構造・センサー計算 95%+
