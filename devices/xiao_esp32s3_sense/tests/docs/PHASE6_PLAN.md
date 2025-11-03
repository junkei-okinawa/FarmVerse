# Phase 6: テストカバレッジの可視化と実機統合テストの基盤構築

## 目標
- **テストカバレッジの可視化と品質向上**（Option A）
- **実機統合テストの基盤構築**（Option B）

## 背景

### 現状の問題
1. テストカバレッジが可視化されておらず、品質が不透明
2. `src/lib.rs`に統合テストが存在するが、`#[cfg(not(test))]`によりホスト環境で実行できない
3. 統合テストは実機でのみ実行可能だが、実行方法が明確でない
4. Phase 5で「統合テスト」と呼んでいたものは実際にはモジュール間の**ユニットテスト**

### 用語の明確化
- **ユニットテスト**: 単一モジュール/関数のロジックテスト（ホスト環境で実行可能）
- **統合テスト**: 複数モジュール間の連携テスト（ホスト環境で実行可能）
- **実機テスト**: 実際のESP32ハードウェアで実行するテスト（実機必須）

## 実装計画

### Option A: テストカバレッジの可視化と品質向上

#### Step 1: カバレッジ測定ツールの導入

**ツール選定**: `cargo-llvm-cov` または `tarpaulin`

**導入手順**:
```bash
# cargo-llvm-covのインストール
cargo install cargo-llvm-cov

# カバレッジ測定
cargo llvm-cov --html

# ブラウザで結果確認
open target/llvm-cov/html/index.html
```

**GitHub Actionsへの統合**:
```yaml
- name: Install llvm-cov
  run: cargo install cargo-llvm-cov

- name: Generate coverage report
  run: cargo llvm-cov --lcov --output-path lcov.info

- name: Upload coverage to Codecov
  uses: codecov/codecov-action@v3
  with:
    files: lcov.info
```

#### Step 2: カバレッジレポートの分析

**分析項目**:
1. 現在のカバレッジ率の確認
2. カバーされていないコードブロックの特定
3. 優先度の高いテスト追加箇所の選定

**目標カバレッジ率**:
- 全体: 70%以上
- コアロジック（`src/core/`, `src/communication/`）: 80%以上
- ユーティリティ（`src/utils/`）: 90%以上

#### Step 3: 不足しているテストの追加

**優先順位**:
1. **Critical**: エラーハンドリングパス
2. **High**: データ変換ロジック（エンディアン、チェックサム）
3. **Medium**: エッジケース（境界値、空データ）

**実装アプローチ**:
```rust
#[cfg(all(test, not(target_os = "espidf")))]
mod tests {
    use super::*;

    #[test]
    fn test_create_sensor_data_frame() {
        let mac = [0x8c, 0xbf, 0xea, 0x8f, 0x3c, 0x88];
        let data = vec![1, 2, 3, 4, 5];
        let seq = 42;
        
        let frame = create_sensor_data_frame(
            &mac,
            FrameType::Data,
            seq,
            &data
        ).unwrap();

        // START markerの検証（big-endian）
        assert_eq!(&frame[0..4], &[0xFA, 0xCE, 0xAA, 0xBB]);
        
        // MACアドレスの検証
        assert_eq!(&frame[4..10], &mac);
        
        // フレームタイプの検証
        assert_eq!(frame[10], 2); // Data = 2
        
        // シーケンス番号の検証（little-endian）
        let seq_bytes = u32::to_le_bytes(seq);
        assert_eq!(&frame[11..15], &seq_bytes);
        
        // データ長の検証（little-endian）
        let len_bytes = u32::to_le_bytes(5);
        assert_eq!(&frame[15..19], &len_bytes);
        
        // データの検証
        assert_eq!(&frame[19..24], &data);
        
        // チェックサムの検証（XOR）
        let checksum = calculate_xor_checksum(&data);
        let checksum_bytes = u32::to_le_bytes(checksum);
        assert_eq!(&frame[24..28], &checksum_bytes);
        
        // END markerの検証（big-endian）
        assert_eq!(&frame[28..32], &[0xCD, 0xEF, 0x56, 0x78]);
    }

    #[test]
    fn test_streaming_message_sequence() {
        // ストリーミングメッセージの連続性テスト
        let data = vec![0; 1000]; // 1KB
        let msgs = StreamingMessage::create_data_stream(
            12345,
            &data,
            200 // 200バイト/チャンク
        );

        // START, DATA(5chunk), ENDの7メッセージ
        assert_eq!(msgs.len(), 7);
        
        // START フレーム
        assert!(matches!(msgs[0].message_type, MessageType::StartFrame));
        
        // DATA フレーム（5つ）
        for i in 1..6 {
            assert!(matches!(msgs[i].message_type, MessageType::DataChunk { .. }));
        }
        
        // END フレーム
        assert!(matches!(msgs[6].message_type, MessageType::EndFrame));
    }
}
```

#### Step 4: ドキュメントへのカバレッジバッジ追加

**README.mdへの統合**:
```markdown
# Sensor Data Sender

[![Test Coverage](https://codecov.io/gh/junkei-okinawa/FarmVerse/branch/main/graph/badge.svg)](https://codecov.io/gh/junkei-okinawa/FarmVerse)

...
```

**tests/docs/README.mdへの記載**:
- カバレッジレポートの確認方法
- カバレッジ目標値
- カバレッジ向上のガイドライン

### Option B: 実機統合テストの基盤構築

#### 前提: Phase 5の問題解決

**Phase 5で判明した問題**:
- `src/lib.rs`の統合テストが`#[cfg(not(test))]`によりホスト環境で実行できない
- 実機テストの実行方法が不明確

**Phase 6での解決方針**:
- 実機テストを専用ディレクトリに移動
- 実行手順を明確化
- CI/CDから除外（手動実行）

#### Step 1: 実機テスト専用ディレクトリの作成
**構造**:
```
tests/
├── unit/                   # ホスト環境ユニットテスト
│   ├── voltage_test.rs
│   ├── mac_test.rs
│   └── ...
├── integration/            # ホスト環境統合テスト（未実装）
│   └── (将来の拡張用)
└── device/                 # 実機専用テスト（新規）
    ├── README.md           # 実機テスト実行手順
    └── sensor_integration.rs
```

#### Step 2: 実機テストの再配置
**タスク**:
1. `src/lib.rs`の統合テストを`tests/device/sensor_integration.rs`に移動
2. 実機テスト実行手順をドキュメント化
3. `run_tests.sh`の整理（ホストテストと実機テストの分離）

**実機テスト実行手順例**:
```bash
# 実機を接続して実行（probe-rs使用）
cd devices/xiao_esp32s3_sense

# 方法1: probe-rsを使用（推奨）
cargo test --test sensor_integration --features device-test

# 方法2: espflash経由でフラッシュ後、手動確認
cargo espflash flash --partition-table partitions.csv --monitor --port /dev/tty.usbmodem101
# シリアルモニターでテスト結果を確認
```

#### Step 3: `run_tests.sh`の更新
**変更内容**:
```bash
#!/bin/bash
set -e

echo "========================================="
echo "  Running Host Unit Tests"
echo "========================================="

# ホスト環境でのユニットテスト
cargo +stable test --lib --tests --exclude device-test

echo "========================================="
echo "  Host Tests Completed Successfully"
echo "========================================="

echo ""
echo "NOTE: Device tests require ESP32-S3 hardware."
echo "To run device tests, execute:"
echo "  cargo test --test sensor_integration --features device-test"
```

#### Step 4: ドキュメント作成
**ファイル**: `tests/device/README.md`

**内容**:
- 実機テストの目的
- 必要なハードウェア
- 実行方法（probe-rs / espflash）
- トラブルシューティング

### プロトコルドキュメントのREADME統合

#### タスク
以下の重要情報を`README.md`に統合:

1. **データフロー図**:
   ```
   xiao_esp32s3_sense → [ESP-NOW] → USB_CDC_Receiver → [USB CDC] → sensor_data_receiver
   ```

2. **Protocol 1: ESP-NOW**
   - フレーム構造（27+ バイト）
   - マーカー: START=0xFACEAABB, END=0xCDEF5678
   - エンディアン: big（マーカー）、little（データフィールド）
   - チェックサム: XOR

3. **Protocol 2: USB CDC**（調査後に追加）
   - フレーム構造
   - マーカー: START=0xAABBCCDD, END=0xDDCCBBAA
   - エンディアン: little

4. **実装場所の明記**:
   - 送信側: `src/communication/esp_now/sender.rs`
   - 受信側: `server/usb_cdc_receiver/src/esp_now/frame.rs`

## 実行順序

### 優先度 High: Option B → Option A
**理由**:
- USB CDCモジュールの方がハードウェア依存度が低い
- テスト実装が容易で、成功体験を早期に得られる
- ESP-NOWモジュールのテスト設計の参考になる

### 推奨順序

#### Option A: テストカバレッジの可視化と品質向上

1. **Step 1: カバレッジ測定ツールの導入**（1-2時間）
   - `cargo-llvm-cov`のインストール
   - ローカル環境での動作確認
   - GitHub Actionsへの統合

2. **Step 2: カバレッジレポートの分析**（1時間）
   - 現状カバレッジの確認
   - 未カバー箇所の特定
   - 優先度の決定

3. **Step 3: 不足しているテストの追加**（3-4時間）
   - エラーハンドリングのテスト追加
   - エッジケースのテスト追加
   - カバレッジ目標達成

4. **Step 4: ドキュメント更新**（30分）
   - README.mdにバッジ追加
   - tests/docs/にカバレッジガイド追加

#### Option B: 実機統合テストの基盤構築

1. **Step 1: 実機テスト専用ディレクトリの作成**（30分）
   - `tests/device/`作成
   - README.md作成

2. **Step 2: 実機テストの再配置**（1-2時間）
   - `src/lib.rs`から`tests/device/sensor_integration.rs`へ移動
   - `#[cfg(not(test))]`問題の解決
   - 実機テスト実行手順の文書化

3. **Step 3: `run_tests.sh`の更新**（30分）
   - ホストテストと実機テストの分離
   - 実機テスト実行コマンドの明記

4. **Step 4: プロトコルドキュメントのREADME統合**（1時間）
   - データフロー図の追加
   - ESP-NOWプロトコル仕様の記載
   - 実装場所の明記

5. **Step 5: CI/CD更新とPR作成**（1時間）
   - GitHub Actionsからの実機テスト除外確認
   - PR作成・レビュー

## 成功基準

### Option A: テストカバレッジの可視化と品質向上
- [ ] `cargo-llvm-cov`が導入され、ローカル環境で実行可能
- [ ] GitHub Actionsでカバレッジレポートが自動生成される
- [ ] Codecov（または類似サービス）へのカバレッジアップロードが完了
- [ ] README.mdにカバレッジバッジが表示されている
- [ ] 全体のカバレッジ率が70%以上
- [ ] コアロジックのカバレッジ率が80%以上
- [ ] 不足していたテスト（エラーハンドリング、エッジケース）が追加されている

### Option B: 実機統合テストの基盤構築
- [ ] `tests/device/`ディレクトリが作成されている
- [ ] 実機テスト実行手順が`tests/device/README.md`に明確に記載されている
- [ ] `src/lib.rs`の統合テストが`tests/device/sensor_integration.rs`に移動
- [ ] `src/lib.rs`の`#[cfg(not(test))]`問題が解決されている
- [ ] `run_tests.sh`がホストテストのみを実行するように更新されている
- [ ] 実機テスト実行コマンドがドキュメント化されている
- [ ] プロトコル仕様（ESP-NOW）が`README.md`に統合されている
- [ ] データフロー図が`README.md`に追加されている
- [ ] 開発者がプロトコル仕様に簡単にアクセスできる

## 注意事項

### Option A: テストカバレッジ関連
1. **カバレッジ測定の制約**
   - ESP32ターゲット（`xtensa-esp32s3-espidf`）ではカバレッジ測定不可
   - ホスト環境（`x86_64-unknown-linux-gnu`など）でのみ測定
   - `#[cfg(all(test, not(target_os = "espidf")))]`でテストを記述

2. **カバレッジ目標の現実性**
   - ハードウェア依存部分はカバレッジ対象外
   - モック可能なロジックのみが対象
   - 目標値は現実的に設定（過度な期待をしない）

3. **段階的な品質向上**
   - 一度に100%を目指さない
   - Phase 6で70-80%を達成
   - 継続的な改善で徐々に向上

### Option B: 実機テスト関連
1. **実機テストは別管理**
   - ホストテストと実機テストを明確に分離
   - 実機テストの実行は任意（CI/CDからは除外）
   - 開発者が必要に応じて手動実行

2. **ドキュメントの重要性**
   - 実機テストの実行手順は詳細に記載
   - トラブルシューティング情報も含める
   - 将来の開発者が迷わないようにする

3. **段階的な実装**
   - 一度にすべてを実装せず、Step by Stepで進める
   - 各Stepで動作確認とPR作成

## 参考資料

- `tests/docs/02_STRATEGY.md`: テスト戦略
- `tests/docs/PHASE5_PLAN.md`: Phase 5プラン
- `tests/docs/PHASE5_IMPLEMENTATION_REPORT.md`: Phase 5実装レポート
- `server/sensor_data_receiver/protocol/frame_parser.py`: USB CDCプロトコルの参考実装
- `/Users/junkei/esp/v5.1.6/esp-idf/export.sh`: ESP-IDF環境設定（実機テスト時に使用）

## 用語補足

### WDT (Watchdog Timer)
**役割**: システムがハングした場合に自動的にリセットする安全機構

**ESP32での実装**:
- Task WDT: タスクが一定時間応答しない場合にリセット
- Interrupt WDT: 割り込みハンドラーがブロックされた場合にリセット

**テストへの影響**:
- 実機テストでは、長時間の処理でWDTタイムアウトに注意
- ホストテストでは無関係（WDTは実機のみ）

**対策**:
```rust
// 長時間処理の場合、WDTをリセット
esp_task_wdt_reset();
```

**参考**:
- [ESP-IDF Watchdog Documentation](https://docs.espressif.com/projects/esp-idf/en/latest/esp32s3/api-reference/system/wdts.html)
