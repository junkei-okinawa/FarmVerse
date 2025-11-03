# 実機統合テスト戦略

## 現状

### ホストマシンでの統合テスト
- **場所**: `src/lib.rs::integration_tests`
- **条件**: `#[cfg(all(test, not(target_os = "espidf")))]`
- **内容**: MeasuredData → StreamingMessage のデータフロー検証
- **実行方法**: `cargo +stable test --lib integration_tests`

### 実機テストの必要性

現在のホスト統合テストではカバーできない項目:

1. **ハードウェア依存機能**
   - カメラ撮影・画像取得
   - センサー読み取り（温度、EC/TDS、電圧）
   - LED制御
   - ESP-NOW 通信

2. **システム統合**
   - DeepSleep/Wake動作
   - RTC メモリ永続化
   - メモリ管理（PSRAM使用）

3. **エンドツーエンド**
   - センサーデータ取得 → ESP-NOW送信
   - 受信機での受信確認

## 実機テスト実装オプション

### オプション A: 専用テストバイナリ（推奨）

実機専用のテストプログラムを別途作成:

```rust
// tests/device_integration_test.rs
#![cfg(target_os = "espidf")]

use esp_idf_sys as _;
use sensor_data_sender::*;

#[test]
fn test_camera_capture() {
    // カメラ初期化・撮影テスト
}

#[test]
fn test_sensor_reading() {
    // センサー読み取りテスト
}

#[test]
fn test_esp_now_communication() {
    // ESP-NOW送受信テスト
}
```

**実行方法**:
```bash
# 実機に書き込んで実行
cargo test --target xtensa-esp32s3-espidf --test device_integration_test
espflash flash --monitor
```

### オプション B: 手動検証スクリプト

テスト用のmain.rsを作成して動作確認:

```rust
// examples/manual_test.rs
fn main() {
    println!("=== 実機統合テスト開始 ===");
    
    // 1. センサーテスト
    test_sensors();
    
    // 2. カメラテスト
    test_camera();
    
    // 3. ESP-NOWテスト
    test_esp_now();
    
    println!("=== テスト完了 ===");
}
```

**実行方法**:
```bash
cargo espflash flash --example manual_test --monitor
```

### オプション C: CI/CD での実機テスト（将来的）

GitHub Actions Self-hosted runner + 実機接続:
- テストファーム環境でのE2Eテスト
- 現時点では実装困難（実機セットアップが必要）

## 推奨アプローチ

**Phase 5: 実機統合テスト追加**

1. **テストハーネス作成**
   - `tests/device/` ディレクトリ作成
   - 実機専用テストを追加

2. **テストカバレッジ**
   - [ ] カメラ初期化・撮影
   - [ ] 温度センサー読み取り
   - [ ] EC/TDSセンサー読み取り
   - [ ] 電圧測定
   - [ ] ESP-NOW送信
   - [ ] DeepSleep動作

3. **実行手順書**
   - 実機接続方法
   - テスト実行コマンド
   - 期待される出力

## 現時点の方針

**Phase 4までの完了を優先**し、実機テストは**Phase 5で検討**:

- ✅ Phase 1-3: ホストユニットテスト (完了)
- ✅ Phase 4A/4B: ストリーミング統合テスト (完了)
- ⏸️ Phase 5: 実機統合テスト (保留)

**理由**:
- ホストテストで主要ロジックは検証済み
- 実機テストは手動検証で補完可能
- CI/CDパイプラインは現状ホストテストのみで十分

## 実機での手動検証方法

現在の実機検証は以下の手順で実施:

```bash
# 1. 実機に書き込み
cargo espflash flash --partition-table partitions.csv --monitor --port /dev/tty.usbmodem101

# 2. シリアルモニターで動作確認
# - センサー値の表示
# - ESP-NOW送信ログ
# - エラーメッセージ

# 3. 受信機側での確認
# USB_CDC_Receiver + sensor_data_receiver でデータ受信確認
```

## まとめ

- **ホスト統合テスト**: データフローの論理検証（実装済み）
- **実機テスト**: 手動検証で補完（現状の運用）
- **自動実機テスト**: Phase 5で検討（将来課題）

Phase 4までの成果で、主要なロジックは十分にテストされており、実機での動作は手動検証で確保できています。
