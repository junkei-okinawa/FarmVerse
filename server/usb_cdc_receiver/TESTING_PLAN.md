# USB CDC Receiver Testing Plan

## 🎯 目標

ESP32-C3デバイスを使わずに、USB CDC ReceiverプロジェクトのUnitテストを実装する。

## 📋 現在の状況

### データフロー（確認済み）
```
┌─────────────────────┐
│ xiao_esp32s3_sense  │  送信側
│   (ESP32S3)         │
└──────────┬──────────┘
           │ ESP-NOW Protocol
           │ Frame: [START:0xFACEAABB][MAC:6][TYPE:1][SEQ:4][LEN:4][DATA][CHECKSUM:4][END:0xCDEF5678]
           ▼
┌─────────────────────┐
│ USB_CDC_Receiver    │  ← このプロジェクト
│  (ESP32-C3)         │
│                     │
│ 1. ESP-NOW受信      │  esp_now/receiver.rs, frame.rs
│    ↓               │
│ 2. バッファリング   │  queue/data_queue.rs
│    ↓               │
│ 3. USB CDC送信      │  usb/cdc.rs ★テスト対象
└──────────┬──────────┘
           │ USB CDC (raw bytes)
           ▼
┌─────────────────────┐
│sensor_data_receiver │  受信側 (Python/PC)
│   (Python)          │
└─────────────────────┘
```

### 重要な実装ファイル

#### テスト可能（ビジネスロジック）
- ✅ `src/esp_now/frame.rs` - ESP-NOWフレーム解析ロジック
- ✅ `src/esp_now/message.rs` - メッセージ構造
- ✅ `src/queue/data_queue.rs` - キューロジック（一部）
- ✅ `src/command.rs` - コマンドパース
- ✅ `src/mac_address.rs` - MACアドレス変換

#### テスト困難（ハードウェア依存）
- ⚠️ `src/usb/cdc.rs` - USB CDCドライバー（モック必要）
- ⚠️ `src/esp_now/receiver.rs` - ESP-NOW受信コールバック
- ⚠️ `src/esp_now/sender.rs` - ESP-NOW送信

## 🎨 テスト戦略

### Phase 1: ホストマシンUnitテスト（優先度: 高）

純粋なロジックのテスト - ハードウェア非依存

#### テスト対象モジュール

1. **ESP-NOW Frame Parser** (`esp_now/frame.rs`)
   - ✅ フレーム構造の解析
   - ✅ バイトオーダー検証（big/little endian）
   - ✅ チェックサム計算
   - ✅ エラーハンドリング

2. **Command Parser** (`command.rs`)
   - ✅ スリープコマンド解析
   - ✅ 不正コマンド処理
   - ✅ フォーマット検証

3. **MAC Address Utils** (`mac_address.rs`)
   - ✅ 文字列⇔バイト配列変換
   - ✅ フォーマット検証

4. **Message Structure** (`esp_now/message.rs`)
   - ✅ データ構造の正当性
   - ✅ シリアライズ/デシリアライズ

### Phase 2: USB CDC Mock Test（優先度: 中）

USB CDCインターフェースをモック化してロジックをテスト

#### Mock実装
```rust
#[cfg(test)]
pub struct MockUsbCdc {
    pub sent_data: Vec<Vec<u8>>,
    pub received_commands: Vec<String>,
}

impl MockUsbCdc {
    pub fn send_frame(&mut self, data: &[u8], _mac_str: &str) -> Result<usize> {
        self.sent_data.push(data.to_vec());
        Ok(data.len())
    }
    
    pub fn read_command(&mut self, _timeout_ms: u32) -> Result<Option<String>> {
        Ok(self.received_commands.pop())
    }
}
```

#### テストシナリオ
- ✅ ESP-NOWフレームを受信 → USB送信確認
- ✅ スリープコマンド受信 → ESP-NOW送信確認
- ✅ エラーハンドリング
- ✅ バッファオーバーフロー対策

### Phase 3: Integration Test with Hardware（オプション）

実機を使った統合テスト（probe-rs使用）

## 📂 テストディレクトリ構造

```
server/usb_cdc_receiver/
├── src/
│   ├── esp_now/
│   │   ├── frame.rs          # テスト追加
│   │   ├── message.rs        # テスト追加
│   │   └── ...
│   ├── command.rs            # テスト追加
│   ├── mac_address.rs        # テスト追加
│   ├── usb/
│   │   └── cdc.rs            # モックテスト追加
│   └── ...
├── tests/
│   ├── unit/
│   │   ├── esp_now_frame_test.rs
│   │   ├── command_parser_test.rs
│   │   └── mac_address_test.rs
│   └── integration/          # 将来的に
│       └── hardware_test.rs  # probe-rs使用
└── TESTING_PLAN.md           # このファイル
```

## 🚀 実装ステップ

### Step 1: ホストテスト環境のセットアップ ✅
- [x] Cargo.tomlに`[lib]`セクション追加
- [x] テストディレクトリ作成
- [x] `cargo test`で実行可能な環境構築

### Step 2: ユニットテストの実装
- [ ] ESP-NOWフレームパーサーのテスト
- [ ] コマンドパーサーのテスト
- [ ] MACアドレスユーティリティのテスト
- [ ] メッセージ構造のテスト

### Step 3: USB CDC Mockの実装
- [ ] `MockUsbCdc` trait実装
- [ ] データ送受信ロジックのテスト
- [ ] エラーハンドリングのテスト

### Step 4: CI/CD統合
- [ ] GitHub Actionsワークフロー作成
- [ ] テスト自動実行設定
- [ ] カバレッジレポート（オプション）

## 📊 期待される成果

- ✅ CI/CDパイプラインでのユニットテスト自動実行
- ✅ ビジネスロジックのテストカバレッジ向上
- ✅ リファクタリングの安全性向上
- ✅ バグの早期発見

## 🔄 次のアクション

1. ESP-NOWフレームパーサーのテストコード作成
2. コマンドパーサーのテストコード作成
3. USB CDC Mockインターフェース設計
4. GitHub Actions設定

---

**Last Updated**: 2025-11-02
