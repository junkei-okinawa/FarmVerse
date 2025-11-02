# テスト実装のクイックスタートガイド

## 即座に実装できるユニットテスト

### ステップ1: 既存コードにテストを追加

以下のファイルにテストモジュールを追加できます：

#### `src/communication/esp_now/sender.rs` にフレーム構築のテストを追加

```rust
// ファイルの最後に追加

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_hash_data_string_with_all_sensors() {
        let temp = Some(25.5);
        let tds = Some(3.2);
        let temp_data = temp.unwrap_or(-999.0);
        let tds_data = tds.unwrap_or(-999.0);
        
        let result = format!(
            "HASH:{},VOLT:{},TEMP:{:.1},TDS_VOLT:{:.1},{}",
            "abc123", 85, temp_data, tds_data, "2024-01-15T10:30:00Z"
        );
        
        assert_eq!(
            result,
            "HASH:abc123,VOLT:85,TEMP:25.5,TDS_VOLT:3.2,2024-01-15T10:30:00Z"
        );
    }

    #[test]
    fn test_build_hash_data_string_without_sensors() {
        let temp: Option<f32> = None;
        let tds: Option<f32> = None;
        let temp_data = temp.unwrap_or(-999.0);
        let tds_data = tds.unwrap_or(-999.0);
        
        let result = format!(
            "HASH:{},VOLT:{},TEMP:{:.1},TDS_VOLT:{:.1},{}",
            "xyz789", 100, temp_data, tds_data, "2024-01-15T10:30:00Z"
        );
        
        assert_eq!(
            result,
            "HASH:xyz789,VOLT:100,TEMP:-999.0,TDS_VOLT:-999.0,2024-01-15T10:30:00Z"
        );
    }
}
```

#### `src/config.rs` に設定値のバリデーションテストを追加

```rust
// ファイルの最後に追加

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_singleton_access() {
        // CONFIG がアクセス可能であることを確認
        assert!(CONFIG.adc_voltage_min_mv > 0);
        assert!(CONFIG.adc_voltage_max_mv > CONFIG.adc_voltage_min_mv);
    }

    #[test]
    fn test_voltage_range_validity() {
        // 電圧範囲が有効であることを確認
        let range = CONFIG.adc_voltage_max_mv - CONFIG.adc_voltage_min_mv;
        assert!(range > 0, "Voltage range must be positive");
        assert!(range < 5000, "Voltage range seems too large");
    }

    #[test]
    fn test_device_id_not_empty() {
        // デバイスIDが設定されていることを確認
        assert!(!CONFIG.device_id.is_empty(), "Device ID should not be empty");
    }
}
```

### ステップ2: 純粋関数を分離してテスト

#### `src/hardware/voltage_sensor.rs` をリファクタリング

既存のコードを変更せず、ヘルパー関数を追加：

```rust
// 既存コードの上に追加
impl VoltageSensor {
    /// 電圧パーセンテージ計算（純粋関数）
    /// 
    /// # Arguments
    /// - `voltage_mv`: 測定電圧（mV）
    /// - `min_mv`: 最小電圧（mV）
    /// - `max_mv`: 最大電圧（mV）
    /// 
    /// # Returns
    /// 0-100: パーセンテージ
    #[inline]
    pub fn calculate_percentage(voltage_mv: f32, min_mv: f32, max_mv: f32) -> u8 {
        let range_mv = max_mv - min_mv;
        
        if range_mv <= 0.0 {
            return 0;
        }
        
        let percentage = ((voltage_mv - min_mv) / range_mv * 100.0)
            .max(0.0)
            .min(100.0);
        
        percentage.round() as u8
    }

    // 既存の measure_voltage_percentage メソッド内で使用：
    // let percentage = Self::calculate_percentage(voltage_mv, min_mv, max_mv);
}

// ファイルの最後に追加
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_percentage_midpoint() {
        let result = VoltageSensor::calculate_percentage(1629.0, 128.0, 3130.0);
        assert_eq!(result, 50);
    }

    #[test]
    fn test_calculate_percentage_min() {
        let result = VoltageSensor::calculate_percentage(128.0, 128.0, 3130.0);
        assert_eq!(result, 0);
    }

    #[test]
    fn test_calculate_percentage_max() {
        let result = VoltageSensor::calculate_percentage(3130.0, 128.0, 3130.0);
        assert_eq!(result, 100);
    }

    #[test]
    fn test_calculate_percentage_clamp_below() {
        let result = VoltageSensor::calculate_percentage(0.0, 128.0, 3130.0);
        assert_eq!(result, 0);
    }

    #[test]
    fn test_calculate_percentage_clamp_above() {
        let result = VoltageSensor::calculate_percentage(5000.0, 128.0, 3130.0);
        assert_eq!(result, 100);
    }

    #[test]
    fn test_calculate_percentage_invalid_range() {
        let result = VoltageSensor::calculate_percentage(1500.0, 3130.0, 128.0);
        assert_eq!(result, 0);
    }
}
```

### ステップ3: テストの実行（将来対応）

現在、ESP32専用ビルドのため、以下の準備が必要：

1. **`Cargo.toml` に lib セクションを追加**（オプション）

```toml
[lib]
name = "xiao_esp32s3_sense"
path = "src/lib.rs"

[[bin]]
name = "xiao-esp32s3-sense"
path = "src/main.rs"
```

2. **`src/lib.rs` を作成**

```rust
// src/lib.rs
#![cfg_attr(target_arch = "xtensa", no_std)]

// テスト可能な純粋関数のみ公開
pub mod utils {
    pub fn calculate_voltage_percentage(voltage_mv: f32, min_mv: f32, max_mv: f32) -> u8 {
        let range_mv = max_mv - min_mv;
        
        if range_mv <= 0.0 {
            return 0;
        }
        
        let percentage = ((voltage_mv - min_mv) / range_mv * 100.0)
            .max(0.0)
            .min(100.0);
        
        percentage.round() as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_voltage_calculation() {
        assert_eq!(utils::calculate_voltage_percentage(1629.0, 128.0, 3130.0), 50);
    }
}
```

3. **テスト実行コマンド**（ホストマシン）

```bash
# ライブラリ部分のみテスト
cargo test --lib

# または特定ターゲットを指定
cargo test --lib --target x86_64-apple-darwin
```

## 推奨：段階的な導入

### Phase 1（即座に実装可能）
- [x] 既存ファイルに `#[cfg(test)]` モジュールを追加
- [x] 純粋関数の単体テストを記述
- [ ] テストドキュメントを追加

### Phase 2（軽微な修正）
- [ ] ビジネスロジックを純粋関数として分離
- [ ] lib.rs を作成してテスト可能な関数を公開
- [ ] `cargo test --lib` で実行可能に

### Phase 3（アーキテクチャ変更）
- [ ] トレイトベースの抽象化
- [ ] モック実装の追加
- [ ] CI/CDパイプラインの統合

## テスト実行の制約事項

### 現在の制限
- ESP32専用クレート（esp-idf-svc）はホストマシンでビルド不可
- `cargo test` は実行できない（クロスコンパイル制約）
- テストコードは記述できるが、実行は将来の対応待ち

### 回避策
1. **条件コンパイル**: `#[cfg(not(target_arch = "xtensa"))]` でホスト用コードを分離
2. **別クレート**: ロジック部分を独立したクレートに分離（ESP32依存なし）
3. **ドキュメントテスト**: `/// ` コメント内の例をテストとして実行

## 次のステップ

1. まず、既存ファイルに `#[cfg(test)]` モジュールを追加
2. 純粋関数を静的メソッドとして分離
3. テストが追加されたことをコミット
4. 将来的に実行環境を整備

この順序で進めることで、実行できなくてもテストコードの価値を保持できます。
