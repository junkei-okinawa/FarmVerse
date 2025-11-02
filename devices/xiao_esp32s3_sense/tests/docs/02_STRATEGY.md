# ESP32-S3 Senseユニットテスト戦略調査レポート

## 実行環境
- **ホストOS**: macOS 15.6.1
- **アーキテクチャ**: ARM64 (Apple Silicon)
- **ESP-IDF**: v5.1.6
- **プロジェクト**: Rust (esp-idf-svc) ベース

## QEMU Emulatorの調査結果

### 1. ESP-IDF QEMU サポート状況

#### ESP32-S3のQEMUサポート
Espressifの公式QEMUドキュメント ([https://docs.espressif.com/projects/esp-idf/en/stable/esp32s3/api-guides/tools/qemu.html](https://docs.espressif.com/projects/esp-idf/en/stable/esp32s3/api-guides/tools/qemu.html)) によると：

**現状の課題:**
- ESP-IDF v5.1.6には、QEMU用のツールやコマンドが統合されていない
- `idf.py qemu` コマンドが存在しない
- ESP32-S3向けのQEMUサポートは**実験的段階**
- 特にWiFi、ESP-NOW、カメラなどの周辺機器エミュレーションは**非対応**

#### ハードウェア依存の機能
本プロジェクト（xiao_esp32s3_sense）で使用している機能：
- ❌ ESP-NOW通信（WiFi依存）
- ❌ カメラ（OV2640）
- ❌ ADC（電圧センサー）
- ❌ GPIO（温度センサー DS18B20）
- ❌ 1-Wire通信
- ❌ TDS/ECセンサー（ADC依存）

→ **本プロジェクトの主要機能はQEMUでエミュレート不可能**

### 2. Apple Silicon (ARM64) での制約
- ESP32はXtensa/RISC-Vアーキテクチャ
- QEMUはクロスアーキテクチャエミュレーションが必要
- Apple SiliconでのXtensaエミュレーションはパフォーマンスが低い
- Homebrewから標準QEMUはインストール可能だが、ESP32専用ビルドが必要

## 代替テスト戦略の提案

### 戦略1: ホストマシンでのユニットテスト（推奨）

Rustの`cfg`属性と条件コンパイルを活用し、ハードウェア非依存のロジックをホスト環境でテスト。

#### 実装アプローチ
```rust
// 例: config.rsのテスト
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_parsing() {
        // 設定解析ロジックのテスト
    }

    #[test]
    fn test_voltage_calculation() {
        // 電圧計算ロジックのテスト（ADC値→電圧変換）
    }
}
```

#### メリット
- ✅ 高速な開発サイクル
- ✅ CI/CDで自動実行可能
- ✅ 追加のツール不要
- ✅ デバッガー利用可能

#### デメリット
- ❌ ハードウェア依存部分はテスト不可

#### 適用範囲
- データ変換ロジック（ADC値→電圧、温度計算）
- 設定パースと検証
- ESP-NOWフレーム構築ロジック
- エラーハンドリング
- 状態管理

### 戦略2: モック/スタブを使用したテスト

ハードウェアインターフェースをトレイトで抽象化し、テスト用モックを作成。

#### 実装例
```rust
// hardware/voltage_sensor.rs
pub trait VoltageReader {
    fn read_raw(&mut self) -> Result<u16, Error>;
}

// 実機用実装
pub struct AdcVoltageReader {
    adc: AdcDriver<'static, Adc1>,
}

// テスト用モック
#[cfg(test)]
pub struct MockVoltageReader {
    values: Vec<u16>,
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_voltage_reading_with_mock() {
        let mut mock = MockVoltageReader::new(vec![1024, 2048]);
        // テスト実行
    }
}
```

#### メリット
- ✅ ハードウェアなしでビジネスロジックをテスト
- ✅ エッジケースを簡単にシミュレート
- ✅ 決定論的なテスト

#### デメリット
- ❌ モック実装のメンテナンスコスト
- ❌ 実際のハードウェア動作との差異

### 戦略3: Unity Test Framework（ESP-IDF標準）

ESP-IDFの統合テストフレームワークを使用。

#### 実装方法
```c
// components/your_component/test/test_sensor.c
#include "unity.h"
#include "esp_log.h"

TEST_CASE("sensor initialization", "[sensor]")
{
    // 実機でのテスト
}
```

#### メリット
- ✅ 実機でのハードウェアテスト
- ✅ ESP-IDF標準フレームワーク
- ✅ CI対応可能（実機必要）

#### デメリット
- ❌ C言語での記述が必要
- ❌ Rustコードとの統合が複雑
- ❌ 実機が必須

### 戦略4: pytest + esp-idf-pytest（統合テスト）

ESP-IDFのpytestフレームワークを使用した統合テスト。

#### セットアップ
```python
# test_sensor_integration.py
import pytest

@pytest.mark.esp32s3
def test_sensor_data_transmission(dut):
    dut.expect("Sensor initialized")
    dut.write("trigger_measurement")
    dut.expect("Temperature: 25.0")
```

#### メリット
- ✅ 実機での統合テスト
- ✅ 自動化可能
- ✅ ログ解析が容易

#### デメリット
- ❌ 実機必須
- ❌ CI環境構築が複雑

### 戦略5: Wokwi シミュレーター（軽量代替案）

オンラインESP32シミュレーター ([https://wokwi.com](https://wokwi.com)) を活用。

#### 特徴
- ✅ ブラウザベース（環境構築不要）
- ✅ ESP32-S3対応
- ✅ 基本的なGPIO、ADC、I2Cシミュレーション
- ❌ WiFi/ESP-NOWは制限あり
- ❌ カメラ非対応
- ❌ 高度なセンサーシミュレーションは困難

### 戦略6: probe-rs + embedded-test（実機テスト）⭐推奨

probe-rsとembedded-testクレートを使用した実機でのユニット・統合テスト。

#### 特徴
- ✅ 実際のハードウェアでテスト実行
- ✅ `cargo test`で実行可能（IDE統合良好）
- ✅ ESP32-S3内蔵USB-JTAG使用（外部ハードウェア不要）
- ✅ 各テストごとにデバイスリセット（クリーンな状態）
- ✅ CI/CD統合可能（Self-hosted runner）
- ⚠️ Xtensaサポートは発展途上（ARMより成熟度低い）
- ❌ フラッシュ+リセットのオーバーヘッドあり

#### 適用範囲
- センサー読み取り（ADC、温度、EC）
- GPIO制御の動作確認
- ペリフェラル初期化テスト
- タイミング依存処理の確認

#### 実装例
```rust
#![no_std]
#![no_main]

#[embedded_test::tests]
mod tests {
    #[init]
    fn init() -> Peripherals {
        Peripherals::take().unwrap()
    }

    #[test]
    fn test_voltage_sensor(peripherals: Peripherals) {
        let adc = AdcDriver::new(peripherals.adc1).unwrap();
        let value = adc.read().unwrap();
        assert!(value > 0);
    }
}
```

詳細は`PROBE_RS_SETUP_GUIDE.md`を参照。

## 推奨実装プラン（更新版）

### 3層テスト戦略

```
┌─────────────────────────────────────────────────────┐
│ Layer 3: 手動実機テスト（優先度: 低）              │
│  - WiFi/ESP-NOW統合テスト                           │
│  - カメラ撮影テスト                                 │
│  - システム全体のエンドツーエンド                   │
│  実行: 手動 / リリース前                            │
├─────────────────────────────────────────────────────┤
│ Layer 2: probe-rs実機テスト（優先度: 中）⭐NEW     │
│  - センサー読み取り（ADC、温度、EC）                │
│  - GPIO、ペリフェラル動作確認                       │
│  - ハードウェア初期化テスト                         │
│  実行: cargo test --test hardware_test              │
├─────────────────────────────────────────────────────┤
│ Layer 1: ホストユニットテスト（優先度: 高）        │
│  - 計算ロジック（電圧変換、温度計算）               │
│  - データ変換・フレーム構築                         │
│  - 設定パース・バリデーション                       │
│  実行: cargo test --lib                             │
└─────────────────────────────────────────────────────┘
```

### フェーズ1: ホストマシンユニットテスト（優先度: 高）

```
devices/xiao_esp32s3_sense/
├── src/
│   ├── config.rs            # 設定解析のテスト
│   ├── core/
│   │   └── data_service.rs  # データ変換ロジックのテスト
│   └── communication/
│       └── esp_now/
│           └── sender.rs    # フレーム構築のテスト
└── tests/
    ├── unit/
    │   ├── config_test.rs
    │   ├── voltage_conversion_test.rs
    │   └── frame_builder_test.rs
    └── integration/           # 実機必須
        └── sensor_test.rs
```

#### 実装ステップ
1. ビジネスロジックを関数として分離
2. `#[cfg(test)]` モジュールを追加
3. `cargo test` で実行可能なテストを記述
4. CI/CDパイプラインに統合

### フェーズ2: probe-rs実機テスト（優先度: 中）⭐NEW

probe-rsとembedded-testを使用した実機での自動テスト。

#### セットアップ
1. probe-rsツールのインストール
2. テスト用Cargo.toml設定
3. 基本的なハードウェアテスト作成

詳細: `04_PROBE_RS_SETUP_GUIDE.md`参照

### フェーズ3: トレイトベースのモック（優先度: 低）

ハードウェアインターフェースをトレイトで抽象化し、モック実装を追加。

### フェーズ4: 実機統合テスト（優先度: 低）

リリース前の最終確認として手動実機テストを実施。

## CI/CD統合案（更新版）

```yaml
# .github/workflows/esp32_test.yml
name = "ESP32 Tests

on: [push, pull_request]

jobs:
  # Layer 1: ホストユニットテスト
  unit-test:
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v3
      - name: Install Rust
        uses: actions-rs/toolchain@v1
      - name: Run unit tests
        run: |
          cd devices/xiao_esp32s3_sense
          cargo test --lib --target x86_64-apple-darwin

  # Layer 2: probe-rs実機テスト（Self-hosted runner）
  hardware-test:
    runs-on: self-hosted  # ESP32-S3接続済みMac
    needs: unit-test
    steps:
      - uses: actions/checkout@v3
      - name: Install probe-rs
        run: cargo install probe-rs-tools --locked
      - name: Run hardware tests
        run: |
          cd devices/xiao_esp32s3_sense
          cargo test --test hardware_test

  # Layer 3: 手動実機テスト（手動トリガーのみ）
  integration-test:
    runs-on: self-hosted
    if: github.event_name == 'workflow_dispatch'
    needs: hardware-test
    steps:
      - name: Manual integration testing
        run: |
          # 手動テストスクリプト
          echo "Manual testing required"
```

## 結論（更新版）

**QEMU Emulatorの導入は現時点では非推奨**です。理由：
1. ESP32-S3のQEMUサポートが実験的段階
2. WiFi/ESP-NOW/カメラなど主要機能が非対応
3. Apple Silicon環境での性能問題
4. セットアップの複雑さ

**推奨アプローチ（3層戦略）:**
1. **Layer 1: ホストマシンユニットテスト**（優先度: 高）
   - 計算ロジック、データ変換、設定パースを重点的にテスト
   - 高速な開発サイクル、CI/CD統合容易

2. **Layer 2: probe-rs実機テスト**（優先度: 中）⭐NEW
   - センサー、GPIO、ペリフェラルの実機動作確認
   - `cargo test`で自動実行可能
   - ハードウェア不具合の早期発見

3. **Layer 3: 手動実機テスト**（優先度: 低）
   - WiFi/ESP-NOW統合、カメラ撮影などの最終確認
   - リリース前の品質保証

この3層戦略により、**開発速度の向上**、**テストカバレッジの確保**、**品質の向上**を同時に実現できます。

### 参考資料

### QEMU関連
- [ESP-IDF QEMU Documentation](https://docs.espressif.com/projects/esp-idf/en/stable/esp32s3/api-guides/tools/qemu.html)
- [Rust Embedded Book - Testing](https://docs.rust-embedded.org/book/start/qemu.html)

### probe-rs関連⭐NEW
- [probe-rs公式サイト](https://probe.rs/)
- [embedded-testクレート](https://crates.io/crates/embedded-test)
- [Rust on ESP Book - probe-rs](https://docs.espressif.com/projects/rust/book/getting-started/tooling/probe-rs.html)
- [embedded-test ESP32C6サンプル](https://github.com/probe-rs/embedded-test/tree/master/examples/esp32c6)
- `04_PROBE_RS_SETUP_GUIDE.md` - 本プロジェクト用セットアップガイド

### プロジェクト関連
- [esp-idf-svc Documentation](https://github.com/esp-rs/esp-idf-svc)
- `03_INVESTIGATION_REPORT.md` - 詳細調査レポート
