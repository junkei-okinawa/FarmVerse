# Phase 5: Integration Tests and Coverage

## 目標
- **統合テストの実装**
- **AppControllerのロジックテスト**
- **テストカバレッジの確認と改善**

## 現状分析
- プロジェクトに明示的な「スケジューラー」は存在しない
- `AppController`がアプリケーションのフロー制御を担当
- 主な制御ロジック:
  - スリープコマンドの処理
  - タイムアウト管理
  - フォールバック処理

## 実装計画

### Step 1: AppControllerのユニットテスト設計
#### タスク
1. `src/core/app_controller.rs` の分析
   - スリープコマンド処理ロジック
   - タイムアウトハンドリング
   - フォールバック処理

2. テスト設計
   - `handle_sleep_with_server_command` のテスト
   - `fallback_sleep` のテスト
   - エラーハンドリングのテスト

#### 期待される成果
- AppControllerのテスト設計ドキュメント
- テストケースリスト

### Step 2: AppControllerユニットテストの実装
#### タスク
1. `src/core/app_controller.rs` にテストモジュール追加
   - 正常系: スリープコマンド受信成功
   - 異常系: タイムアウト
   - 異常系: 無効なスリープ時間
   - フォールバック処理

2. モック/スタブの実装
   - `EspNowReceiver`のモック
   - `DeepSleep`のモック

#### テストケース例
```rust
#[cfg(all(test, not(target_os = "espidf")))]
mod tests {
    use super::*;

    struct MockDeepSleepPlatform;
    impl DeepSleepPlatform for MockDeepSleepPlatform {
        fn sleep(&self, _duration_us: u64) {}
    }

    #[test]
    fn test_handle_sleep_command_success() {
        // スリープコマンド受信成功のテスト
    }

    #[test]
    fn test_handle_sleep_command_timeout() {
        // タイムアウト時のフォールバック動作テスト
    }

    #[test]
    fn test_fallback_sleep() {
        // エラー時のフォールバック処理テスト
    }
}
```

### Step 3: 統合テストの実装
#### タスク
1. `tests/integration/data_flow_test.rs` の作成
   - センサーデータ取得 → フレーム構築 → 送信準備の統合テスト
   - 設定読み込み → データ処理の統合テスト

2. テストシナリオ
   - 完全なデータフロー（モックセンサー → フレーム → ストリーミング）
   - エラー発生時のリカバリー
   - 設定変更の反映

#### 実装例
```rust
#[cfg(all(test, not(target_os = "espidf")))]
mod integration_tests {
    use super::*;

    #[test]
    fn test_sensor_to_streaming_pipeline() {
        // センサーデータ → ストリーミングフレーム変換の統合テスト
        let measured_data = MeasuredData::new(75.5, Some(25.3));
        
        // JSONシリアライズ
        let json = serde_json::to_string(&measured_data).unwrap();
        
        // ストリーミングメッセージに変換
        let msgs = StreamingMessage::create_data_stream(
            12345,
            json.as_bytes(),
            200
        );
        
        assert!(!msgs.is_empty());
        // 各メッセージの検証
    }

    #[test]
    fn test_config_to_sensor_data() {
        // 設定 → センサーデータ生成の統合テスト
    }
}
```

### Step 4: テストカバレッジの確認と改善
#### タスク
1. 既存テストの実行確認
   ```bash
   cd devices/xiao_esp32s3_sense
   ./run_tests.sh
   ```

2. カバレッジ分析（`cargo-llvm-cov`を使用）
   ```bash
   cargo +stable llvm-cov --lib --tests --html
   ```

3. カバレッジレポートの分析
   - 未テストの重要機能の特定
   - 優先度の高い部分のテスト追加

4. 不足しているテストの追加
   - `MeasuredData`の追加テスト
   - `DataService`のテスト
   - エラーハンドリングのテスト

### Step 5: CI/CD統合とドキュメント更新
#### タスク
1. GitHub Actionsワークフローの更新
   - 統合テストの実行
   - カバレッジレポートの生成（オプション）

2. READMEへのテスト実行方法の追記
   - ホストテストの実行方法
   - 統合テストの実行方法
   - カバレッジ確認方法

3. Phase 5実装レポートの作成
   - 実装内容のサマリー
   - テスト結果
   - カバレッジ結果
   - 今後の改善点

## 成功基準
- [ ] AppControllerのユニットテストが実装され、すべてパス
- [ ] 統合テストが実装され、すべてパス
- [ ] テストカバレッジ50%以上達成（ハードウェア依存部分を除く）
- [ ] CI/CDで自動実行され、成功
- [ ] ドキュメントが更新されている
- [ ] `run_tests.sh`ですべてのテストが実行可能

## 注意事項
- ハードウェア依存部分（ESP-NOW受信、実際のDeepSleep）はモック化
- テストは決定論的で再現可能であること
- モック/スタブは最小限に抑える
- 実装コードの変更は最小限に

## 参考資料
- `tests/docs/02_STRATEGY.md`: テスト戦略
- `tests/docs/PHASE4A_IMPLEMENTATION_REPORT.md`: Phase 4Aの実装レポート
- `tests/docs/PHASE4AB_IMPLEMENTATION_REPORT.md`: Phase 4A/Bの実装レポート
- `run_tests.sh`: テスト実行スクリプト

