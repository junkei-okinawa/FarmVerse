# probe-rs + embedded-test セットアップガイド

ESP32-S3実機でのユニットテスト実行環境構築手順

---

## 前提条件

- ✅ macOS 15.6.1 (Apple Silicon)
- ✅ Rust toolchain インストール済み
- ✅ ESP-IDF v5.1.6 セットアップ済み
- ✅ XIAO ESP32-S3 Sense デバイス
- ✅ USB-Cケーブル（デバイス接続用）

---

## Step 1: probe-rsツールのインストール

```bash
# probe-rsツールチェーンのインストール
cargo install probe-rs-tools --locked

# インストール確認
probe-rs --version
# 期待される出力: probe-rs 0.30.0 (または最新版)
```

---

## Step 2: プロジェクト構成の変更

### 2.1 Cargo.tomlの更新

```toml
# Cargo.toml

[package]
name = "sensor-data-sender"
version = "0.2.0"
edition = "2021"

[[bin]]
name = "sensor_data_sender"
path = "src/main.rs"
test = false  # バイナリのテストを無効化

# ライブラリセクションを追加（テストハーネス用）
[lib]
name = "sensor_lib"
path = "src/lib.rs"
harness = false  # embedded-testの独自ハーネスを使用

[dependencies]
# 既存の依存関係...
embedded-test-linker-script = "0.1.0"
rtt-target = "0.6.1"  # ログ出力用

[dev-dependencies]
embedded-test = { version = "0.7.0", features = ["xtensa-semihosting"] }

# テストバイナリの定義
[[test]]
name = "hardware_test"
harness = false  # 重要: embedded-testハーネスを使用
```

### 2.2 build.rsの更新

既存の`build.rs`に以下を追加：

```rust
// build.rs
fn main() {
    embuild::espidf::sysenv::output();
    
    // embedded-testリンカスクリプトを追加
    println!("cargo::rustc-link-arg=-Tembedded-test.x");
}
```

### 2.3 .cargo/config.tomlの作成

プロジェクトルートに`.cargo/config.toml`を作成（存在しない場合）：

```toml
# .cargo/config.toml

[env]
ESP_LOG = "info"

[target.xtensa-esp32s3-espidf]
# 通常のビルド用runner（既存）
# runner = "espflash flash --monitor"

# テスト用runner（probe-rs）
runner = "probe-rs run --chip esp32s3"

[build]
target = "xtensa-esp32s3-espidf"
```

**注意**: `runner`設定は1つのみ有効。通常ビルドとテストで切り替える場合は、
環境変数や別の設定ファイルを使用。

---

## Step 3: シンプルなテストケースの作成

### 3.1 テストディレクトリの作成

```bash
mkdir -p tests
```

### 3.2 最初のテストファイル作成

`tests/hardware_test.rs`を作成：

```rust
// tests/hardware_test.rs
#![no_std]
#![no_main]

// ログ出力の初期化
#[embedded_test::setup]
fn setup_log() {
    rtt_target::rtt_init_log!();
}

#[cfg(test)]
#[embedded_test::tests]
mod tests {
    use esp_idf_hal::peripherals::Peripherals;
    use esp_idf_hal::delay::FreeRtos;

    // テスト前の初期化処理
    #[init]
    fn init() -> Peripherals {
        esp_idf_sys::link_patches();
        Peripherals::take().unwrap()
    }

    // シンプルな動作確認テスト
    #[test]
    fn basic_test() {
        assert_eq!(2 + 2, 4);
        log::info!("Basic test passed!");
    }

    // ペリフェラルを使用するテスト
    #[test]
    fn test_peripherals_available(peripherals: Peripherals) {
        // ペリフェラルが取得できることを確認
        assert!(peripherals.pins.gpio1.is_some());
        log::info!("Peripherals test passed!");
    }

    // タイムアウト付きテスト（5秒）
    #[test]
    #[timeout(5)]
    fn test_with_delay() {
        FreeRtos::delay_ms(100);
        log::info!("Delay test passed!");
    }

    // 失敗が期待されるテスト
    #[test]
    #[should_panic]
    fn test_expected_panic() {
        panic!("This panic is expected");
    }

    // 無視されるテスト（開発中）
    #[test]
    #[ignore]
    fn test_work_in_progress() {
        // まだ実装していない機能のテスト
    }
}
```

---

## Step 4: デバイスの接続確認

```bash
# デバイスが認識されているか確認
probe-rs list

# 期待される出力例:
# The following debug probes were found:
# [0]: ESP JTAG -- 303a:1001:XX:XX:XX:XX:XX:XX (EspJtag)
```

---

## Step 5: テストの実行

```bash
# プロジェクトルートで実行
cd ~/Documents/FarmVerse/devices/xiao_esp32s3_sense

# テストのビルドと実行
cargo test --test hardware_test

# 期待される出力:
# Compiling sensor-data-sender v0.2.0
# Finished `test` profile [optimized + debuginfo] target(s) in X.XXs
# Running tests/hardware_test.rs (target/xtensa-esp32s3-espidf/debug/deps/hardware_test-...)
# 
# running 5 tests
# test basic_test ... ok
# test test_peripherals_available ... ok
# test test_with_delay ... ok
# test test_expected_panic ... ok
# test test_work_in_progress ... ignored
# 
# test result: ok. 4 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out
```

---

## Step 6: センサーテストの追加例

### 電圧センサーテスト

```rust
// tests/voltage_sensor_test.rs
#![no_std]
#![no_main]

#[embedded_test::setup]
fn setup_log() {
    rtt_target::rtt_init_log!();
}

#[cfg(test)]
#[embedded_test::tests]
mod tests {
    use esp_idf_hal::peripherals::Peripherals;
    use esp_idf_hal::adc::{AdcDriver, AdcChannelDriver, attenuation};
    use esp_idf_hal::gpio::Gpio1;

    struct TestContext {
        adc: AdcDriver<'static, esp_idf_hal::adc::ADC1>,
        adc_pin: AdcChannelDriver<'static, Gpio1, esp_idf_hal::adc::ADC1>,
    }

    #[init]
    fn init() -> TestContext {
        esp_idf_sys::link_patches();
        let peripherals = Peripherals::take().unwrap();
        
        let adc = AdcDriver::new(
            peripherals.adc1,
            &esp_idf_hal::adc::config::Config::new()
        ).unwrap();
        
        let adc_pin = AdcChannelDriver::new(
            peripherals.pins.gpio1,
            attenuation::DB_11
        ).unwrap();
        
        TestContext { adc, adc_pin }
    }

    #[test]
    fn test_voltage_reading(mut ctx: TestContext) {
        // ADCから値を読み取り
        let raw_value = ctx.adc.read(&mut ctx.adc_pin).unwrap();
        
        log::info!("ADC raw value: {}", raw_value);
        
        // 妥当な範囲内かチェック（0-4095）
        assert!(raw_value >= 0);
        assert!(raw_value <= 4095);
    }

    #[test]
    fn test_voltage_stability() {
        // 複数回読み取って安定性を確認
        let mut ctx = init();
        let mut readings = [0u16; 10];
        
        for i in 0..10 {
            readings[i] = ctx.adc.read(&mut ctx.adc_pin).unwrap();
            FreeRtos::delay_ms(10);
        }
        
        // 変動が許容範囲内かチェック
        let max = readings.iter().max().unwrap();
        let min = readings.iter().min().unwrap();
        let variance = max - min;
        
        log::info!("Voltage variance: {}", variance);
        assert!(variance < 200, "Voltage reading too unstable");
    }
}
```

---

## トラブルシューティング

### 問題1: デバイスが認識されない

```bash
# USB接続を確認
system_profiler SPUSBDataType | grep -A 10 ESP

# 権限の問題がある場合（macOS）
sudo probe-rs list
```

### 問題2: フラッシュエラー

```bash
# デバイスを完全にリセット
probe-rs erase --chip esp32s3

# 再度テストを実行
cargo test --test hardware_test
```

### 問題3: リンカエラー

```bash
# ビルドキャッシュをクリア
cargo clean

# 依存関係を再ビルド
cargo build --release
```

### 問題4: セミホスティングエラー

`Cargo.toml`のembedded-testフィーチャーを確認：

```toml
[dev-dependencies]
embedded-test = { version = "0.7.0", features = ["xtensa-semihosting"] }
```

---

## ベストプラクティス

### 1. テストの分離

```rust
// 各テストはデバイスリセット後に実行されるため、
// グローバル状態に依存しないこと

#[test]
fn test_a() {
    // OK: 独立したテスト
}

#[test]
fn test_b() {
    // OK: test_aの結果に依存しない
}
```

### 2. タイムアウトの設定

```rust
// 長時間かかる可能性のあるテストにはタイムアウトを設定
#[test]
#[timeout(10)]  // 10秒
fn test_sensor_warm_up() {
    // センサーのウォームアップを待つテスト
}
```

### 3. ログの活用

```rust
#[test]
fn test_with_logging() {
    log::info!("Test started");
    
    let result = perform_operation();
    log::info!("Operation result: {:?}", result);
    
    assert!(result.is_ok());
    log::info!("Test passed");
}
```

### 4. エラーハンドリング

```rust
#[test]
fn test_with_error() -> Result<(), &'static str> {
    let result = risky_operation();
    
    if result.is_err() {
        return Err("Operation failed");
    }
    
    Ok(())
}
```

---

## 次のステップ

1. ✅ 基本的なテスト実行を確認
2. **センサー別のテストを追加**
   - `tests/voltage_sensor_test.rs`
   - `tests/temperature_sensor_test.rs`
   - `tests/ec_sensor_test.rs`
3. **CI/CD統合の検討**
   - Self-hosted runnerのセットアップ
   - GitHub Actionsワークフロー作成
4. **テストカバレッジの拡充**

---

## 参考資料

- [probe-rs公式ドキュメント](https://probe.rs/docs/)
- [embedded-testドキュメント](https://docs.rs/embedded-test/)
- [ESP32C6サンプルプロジェクト](https://github.com/probe-rs/embedded-test/tree/master/examples/esp32c6)
- [Rust on ESP Book](https://esp-rs.github.io/book/)
