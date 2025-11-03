# ユニットテスト実装ガイド

このディレクトリには、ESP32-S3 Senseプロジェクトのユニットテスト実装例が含まれています。

## ディレクトリ構造

```
tests/
├── unit_test_example.rs    # ユニットテスト実装例（参考用）
└── README.md               # このファイル
```

## テストの実行方法

### 標準的なユニットテスト（ホストマシン）

現在、プロジェクトはESP32ターゲット専用にビルドされているため、標準の`cargo test`は実行できません。

ホストマシンでユニットテストを実行するには、以下のアプローチが必要です：

#### アプローチ1: 条件コンパイルを使用

```rust
// src/lib.rs または個別のモジュール内
#[cfg(not(target_arch = "xtensa"))]
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_voltage_calculation() {
        // テストコード
    }
}
```

実行:
```bash
cargo test --lib --target x86_64-apple-darwin
```

#### アプローチ2: 別のワークスペースメンバーを作成

プロジェクトルートに`Cargo.toml`を追加してワークスペース化し、テスト専用クレートを作成:

```toml
[workspace]
members = [
    "devices/xiao_esp32s3_sense",
    "devices/xiao_esp32s3_sense_tests"  # テスト専用
]
```

### ESP32実機でのテスト

実機テストは、ESP-IDFのUnityフレームワークまたはpytestを使用します。

```bash
# ESP-IDF環境の有効化
. ~/esp/v5.1.6/esp-idf/export.sh

# ビルドとフラッシュ
cd devices/xiao_esp32s3_sense
cargo build --release
espflash flash --monitor target/xtensa-esp32s3-espidf/release/xiao-esp32s3-sense
```

## テスト実装のベストプラクティス

### 1. ビジネスロジックの分離

ハードウェアアクセスとビジネスロジックを分離し、ロジック部分をテスト可能にします。

**悪い例:**
```rust
pub fn read_sensor() -> Result<u8, Error> {
    let adc = init_adc()?;
    let raw = adc.read()?;
    let percentage = ((raw as f32 - 128.0) / 3002.0 * 100.0) as u8;
    Ok(percentage)
}
```

**良い例:**
```rust
// テスト可能な純粋関数
pub fn calculate_percentage(raw: f32, min: f32, max: f32) -> u8 {
    ((raw - min) / (max - min) * 100.0).clamp(0.0, 100.0) as u8
}

// ハードウェアアクセス
pub fn read_sensor() -> Result<u8, Error> {
    let adc = init_adc()?;
    let raw = adc.read()? as f32;
    Ok(calculate_percentage(raw, 128.0, 3130.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_percentage_calculation() {
        assert_eq!(calculate_percentage(1629.0, 128.0, 3130.0), 50);
    }
}
```

### 2. トレイトを使用した抽象化

ハードウェアインターフェースをトレイトで抽象化します。

```rust
pub trait VoltageReader {
    fn read_voltage(&mut self) -> Result<u16, Error>;
}

// 実機用実装
pub struct AdcVoltageReader {
    adc: AdcDriver<'static>,
}

impl VoltageReader for AdcVoltageReader {
    fn read_voltage(&mut self) -> Result<u16, Error> {
        self.adc.read()
    }
}

// テスト用モック
#[cfg(test)]
pub struct MockVoltageReader {
    values: Vec<u16>,
    index: usize,
}

#[cfg(test)]
impl VoltageReader for MockVoltageReader {
    fn read_voltage(&mut self) -> Result<u16, Error> {
        let value = self.values[self.index];
        self.index += 1;
        Ok(value)
    }
}
```

### 3. 条件コンパイルの活用

プラットフォーム固有のコードを分離します。

```rust
#[cfg(target_arch = "xtensa")]
use esp_idf_svc::hal::adc::AdcDriver;

#[cfg(not(target_arch = "xtensa"))]
type AdcDriver = (); // ダミー型

// 共通ロジック
pub fn process_data(value: f32) -> String {
    format!("Processed: {:.2}", value)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_process_data() {
        assert_eq!(process_data(25.5), "Processed: 25.50");
    }
}
```

## テストカバレッジの推奨範囲

| コンポーネント | テスト方法 | カバレッジ目標 |
|--------------|----------|--------------|
| 計算ロジック（電圧変換、温度変換） | ユニットテスト（ホスト） | 90%以上 |
| データフォーマット（フレーム構築） | ユニットテスト（ホスト） | 90%以上 |
| 設定パース | ユニットテスト（ホスト） | 80%以上 |
| ハードウェアドライバー | 実機テスト | 主要パスのみ |
| 統合フロー | 実機テスト | エンドツーエンド |

## CI/CD統合

GitHub Actionsでのユニットテスト例:

```yaml
# .github/workflows/test.yml
name: Unit Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v3
      
      - name: Install Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
          target: x86_64-apple-darwin
      
      - name: Run tests
        run: |
          cd devices/xiao_esp32s3_sense
          # ロジック部分のテストのみ実行
          cargo test --lib --target x86_64-apple-darwin
```

## 関連ドキュメント

- [TESTING_STRATEGY.md](../TESTING_STRATEGY.md) - 詳細なテスト戦略
- [unit_test_example.rs](./unit_test_example.rs) - 実装例

## トラブルシューティング

### `cargo test`でリンクエラーが発生する

ESP32専用のクレート（esp-idf-svc等）がホスト環境でビルドできません。
条件コンパイルで分離するか、テスト専用クレートを作成してください。

### モックの作成が複雑

最初は計算ロジックのみをテストし、徐々にモック範囲を拡大してください。
完璧を目指さず、重要なビジネスロジックから優先的にテストを追加します。
