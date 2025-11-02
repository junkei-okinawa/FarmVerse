# ESP32-S3 Sense ユニットテスト調査報告書

**調査日**: 2025-10-19  
**対象**: XIAO ESP32-S3 Sense デバイス（Rustプロジェクト）  
**目的**: QEMU Emulatorまたは代替テスト手法の導入可能性評価

---

## エグゼクティブサマリー

QEMU Emulatorを使用したESP32-S3のユニットテストは、**技術的制約により現時点では非推奨**です。代わりに、**ホストマシンでのユニットテスト**と**実機での統合テスト**を組み合わせる戦略を推奨します。

### 結論
- ❌ **QEMU導入**: 非推奨（WiFi/カメラ等の主要機能が非対応）
- ✅ **ホストユニットテスト**: 推奨（計算ロジック、データ変換）
- ✅ **実機統合テスト**: 推奨（リリース前の最終確認）

---

## 環境情報

| 項目 | 詳細 |
|-----|------|
| ホストOS | macOS 15.6.1 |
| アーキテクチャ | ARM64 (Apple Silicon) |
| ESP-IDF | v5.1.6 |
| プロジェクト言語 | Rust (esp-idf-svc 0.51.0) |
| ターゲット | ESP32-S3 (Xtensa) |

---

## QEMU Emulator 調査結果

### 1. ESP-IDF QEMUサポート状況

#### 確認事項
- ✅ ESP-IDF v5.1.6 が正常にインストール済み
- ❌ `idf.py qemu` コマンドが存在しない
- ❌ QEMU関連ツールがESP-IDFに統合されていない
- ⚠️ ESP32-S3のQEMUサポートは実験的段階

#### 技術的制約

| 機能 | QEMUサポート | 本プロジェクトでの使用 |
|-----|------------|-------------------|
| WiFi/ESP-NOW | ❌ 非対応 | ✅ 使用中（通信の主軸） |
| カメラ（OV2640） | ❌ 非対応 | ✅ 使用中 |
| ADC | ⚠️ 部分対応 | ✅ 使用中（電圧・TDS） |
| GPIO | ✅ 基本対応 | ✅ 使用中（センサー） |
| 1-Wire | ❌ 非対応 | ✅ 使用中（DS18B20） |

**結果**: 本プロジェクトの主要機能（ESP-NOW通信、カメラ、センサー）はQEMUでエミュレート不可能。

### 2. Apple Silicon (ARM64) での追加制約

- ESP32はXtensaアーキテクチャ（RISC-V対応もあり）
- QEMUでクロスアーキテクチャエミュレーションが必要
- Xtensaエミュレーションのパフォーマンスが低い
- HomebrewからQEMUをインストール可能だが、ESP32専用ビルドが必要

### 3. QEMU導入コスト評価

| 項目 | 評価 | 備考 |
|-----|------|------|
| セットアップの複雑さ | 高 | ESP32専用QEMUビルドが必要 |
| 学習コスト | 高 | ドキュメント不足、実験的機能 |
| 機能カバレッジ | 低 | 主要機能が非対応 |
| メンテナンスコスト | 高 | 継続的なアップデートが必要 |
| ROI（投資対効果） | **低** | コストに対してメリットが少ない |

**判断**: QEMU導入は**投資対効果が低い**ため非推奨。

---

## 推奨代替案

### 戦略1: ホストマシンでのユニットテスト（優先度: 高）

#### 概要
ハードウェア非依存のビジネスロジックをホスト環境でテスト。

#### 対象範囲
- ✅ 電圧計算（ADC値 → パーセンテージ変換）
- ✅ 温度計算（DS18B20生データ → 摂氏変換）
- ✅ TDS計算（電圧 → ppm変換）
- ✅ ESP-NOWフレーム構築ロジック
- ✅ 設定パース・バリデーション
- ✅ エラーハンドリング

#### 実装方法
```rust
// 純粋関数として分離
pub fn calculate_voltage_percentage(voltage_mv: f32, min_mv: f32, max_mv: f32) -> u8 {
    // 計算ロジック
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_voltage_calculation() {
        assert_eq!(calculate_voltage_percentage(1629.0, 128.0, 3130.0), 50);
    }
}
```

#### メリット
- ⚡ 高速な開発サイクル（秒単位でテスト実行）
- 🔄 CI/CDで自動実行可能
- 🐛 デバッガー利用可能
- 💰 追加コスト不要

#### デメリット
- ハードウェア依存部分はテスト不可（→ 実機テストで補完）

### 戦略2: トレイトベースのモック（優先度: 中）

#### 概要
ハードウェアインターフェースをトレイトで抽象化し、テスト用モックを作成。

#### 実装例
```rust
pub trait VoltageReader {
    fn read_raw(&mut self) -> Result<u16, Error>;
}

// 実機用
pub struct AdcVoltageReader { /* ... */ }

// テスト用
#[cfg(test)]
pub struct MockVoltageReader {
    values: Vec<u16>,
}
```

#### メリット
- エッジケースを簡単にシミュレート
- 決定論的なテスト

#### デメリット
- モック実装のメンテナンスコスト

### 戦略3: 実機統合テスト（優先度: 中〜低）

#### 概要
最終的なリリース前に実機でエンドツーエンドテストを実施。

#### 実装方法
- ESP-IDF Unityフレームワーク（C言語）
- pytest + esp-idf-pytest（Python）
- 手動テスト + ログ確認

#### メリット
- 実際のハードウェア動作を確認
- 統合問題を発見可能

#### デメリット
- 実機必須
- 時間がかかる
- CI環境構築が複雑

---

## 実装ロードマップ

### Phase 1: 即座に実装可能（〜1週間）

**目標**: 既存コードにテストモジュールを追加

- [x] テスト戦略ドキュメント作成（`TESTING_STRATEGY.md`）
- [x] テスト実装例作成（`tests/unit_test_example.rs`）
- [x] クイックスタートガイド作成（`tests/QUICKSTART.md`）
- [ ] 既存ファイルに `#[cfg(test)]` モジュール追加
  - [ ] `src/hardware/voltage_sensor.rs`
  - [ ] `src/communication/esp_now/sender.rs`
  - [ ] `src/config.rs`

**成果物**: テストコードが記述され、将来的に実行可能な状態

### Phase 2: アーキテクチャ改善（1〜2週間）

**目標**: ビジネスロジックを分離してテスト実行可能に

- [ ] 純粋関数をヘルパー関数として分離
- [ ] `src/lib.rs` を作成
- [ ] `Cargo.toml` に lib セクション追加
- [ ] `cargo test --lib --target x86_64-apple-darwin` で実行可能に

**成果物**: ホストマシンでユニットテストが実行可能

### Phase 3: CI/CD統合（1週間）

**目標**: 自動テストパイプライン構築

- [ ] GitHub Actions ワークフローを作成
- [ ] PR時に自動テスト実行
- [ ] テストカバレッジレポート生成

**成果物**: 継続的なテスト実行環境

### Phase 4（オプション）: 高度なテスト（必要に応じて）

- [ ] トレイトベースの抽象化
- [ ] モック実装
- [ ] 実機テスト自動化（pytest）

---

## テストカバレッジ目標

| コンポーネント | テスト方法 | カバレッジ目標 | 優先度 |
|--------------|----------|--------------|-------|
| 計算ロジック | ユニットテスト（ホスト） | 90%以上 | 🔴 高 |
| データフォーマット | ユニットテスト（ホスト） | 90%以上 | 🔴 高 |
| 設定パース | ユニットテスト（ホスト） | 80%以上 | 🟡 中 |
| ハードウェアドライバー | 実機テスト | 主要パスのみ | 🟢 低 |
| 統合フロー | 実機テスト | エンドツーエンド | 🟡 中 |

---

## リスク評価

| リスク | 影響度 | 発生確率 | 対策 |
|-------|-------|---------|-----|
| QEMUへの過剰投資 | 高 | 中 | ホストテストを優先 |
| テスト実行環境の複雑化 | 中 | 低 | 段階的な導入 |
| 実機テスト不足 | 高 | 中 | Phase 1で統合テスト計画 |
| メンテナンスコスト増 | 中 | 中 | シンプルなテスト設計 |

---

## 次のアクション

### 即座に実行すべき項目

1. ✅ **テスト戦略ドキュメントのレビュー**（完了）
2. **Phase 1の実装開始**
   - `src/hardware/voltage_sensor.rs` にテスト追加
   - `src/communication/esp_now/sender.rs` にテスト追加
3. **チームでのテスト戦略の合意形成**

### 中期的な項目（1〜2ヶ月）

1. ビジネスロジックの分離
2. CI/CD統合
3. テストカバレッジ50%達成

### 長期的な項目（3ヶ月以上）

1. トレイトベースの抽象化
2. 実機テスト自動化
3. テストカバレッジ80%達成

---

## 付録: 作成ドキュメント

本調査で以下のドキュメントを作成しました：

1. **`TESTING_STRATEGY.md`** - 包括的なテスト戦略文書
2. **`tests/README.md`** - テスト実装ガイド
3. **`tests/QUICKSTART.md`** - クイックスタートガイド
4. **`tests/unit_test_example.rs`** - テスト実装例
5. **`INVESTIGATION_REPORT.md`** - 本レポート

---

## 参考資料

- [ESP-IDF QEMU Documentation](https://docs.espressif.com/projects/esp-idf/en/stable/esp32s3/api-guides/tools/qemu.html)
- [Rust Embedded Book - Testing](https://docs.rust-embedded.org/book/start/qemu.html)
- [esp-idf-svc Documentation](https://github.com/esp-rs/esp-idf-svc)

---

**結論**: QEMU Emulatorは現時点では非推奨。ホストマシンでのユニットテストを中心とした段階的なテスト戦略を採用することで、実用的かつ効果的なテスト環境を構築できます。

## 追加調査: probe-rs + embedded-test による実機ユニットテスト

**調査日**: 2025-11-02  
**目的**: probe-rsとembedded-testクレートを使用した、ESP32-S3実機でのユニットテスト導入可能性評価

---

### 1. probe-rsとembedded-testの概要

#### probe-rsとは
- Rustで書かれたオンチップデバッグツール
- ARM、RISC-V、**Xtensa (ESP32-S3)** をサポート
- USB-JTAG経由でファームウェアのフラッシュ、デバッグ、テスト実行が可能
- ESP32-S3の内蔵USB-JTAGインターフェースを使用（外部ハードウェア不要）

#### embedded-testとは
- 組み込みデバイス向けのテストハーネス
- probe-rsと連携して実機上でユニット・統合テストを実行
- `cargo test`コマンドで実行可能
- libtest互換のテストランナー（VSCode/IntelliJでの個別テスト実行対応）

#### 動作フロー
1. ELFファイルからテスト情報を読み取り
2. 全テストケースをデバイスに一括フラッシュ
3. 各テストケースを順次実行：
   - デバイスをリセット
   - セミホスティング経由でテストケースIDを通知
   - テスト完了を待機（成功/失敗を取得）
4. 結果をレポート

---

### 2. ESP32-S3サポート状況

#### ✅ サポート内容
- **フラッシング**: 安定動作（USB-JTAG経由）
- **ログ出力**: RTT/defmtによるログ出力が可能
- **基本的なテスト実行**: 実機上でのユニット・統合テストが実行可能
- **内蔵JTAG**: XIAO ESP32-S3 Senseの内蔵USB-JTAGインターフェースを使用可能

#### ⚠️ 制限事項
- **Xtensaサポートは発展途上**: ARMやRISC-Vと比較して成熟度が低い
- **高度なデバッグ機能**: ブレークポイント、レジスタアクセスなどは不安定な場合がある
- **一部機能の不具合**: フラッシュ消去やデバッグ操作で報告されている問題あり
- **ドキュメント**: ESP32-S3固有のドキュメントが限定的

#### 📊 コミュニティの評価
- 公式のRust on ESP Bookでprobe-rsが紹介されている
- Espressif自身がRustプロジェクトでembedded-testを活用
- 活発な開発が継続中（継続的な改善が期待できる）

---

### 3. 実装例: ESP32C6プロジェクト（参考）

probe-rs/embedded-testリポジトリのESP32C6サンプルから、ESP32-S3への適用方法を分析：

#### Cargo.toml構成
```toml
[dependencies]
esp-hal = { version = "1.0.0", features = ["esp32s3", "unstable"] }
esp-rtos = { version = "0.2.0", features = ["embassy", "esp32s3", "log-04"] }
embedded-test-linker-script = "0.1.0"

[dev-dependencies]
embedded-test = { version = "0.7.0", features = ["embassy", "external-executor"] }

[[test]]
name = "example_test"
harness = false  # 重要: embedded-testの独自ハーネスを使用

[lib]
harness = false
```

#### .cargo/config.toml
```toml
[target.xtensa-esp32s3-espidf]
runner = "probe-rs run --chip esp32s3"

[env]
ESP_LOG = "debug"
```

#### build.rs
```rust
fn main() {
    println!("cargo:rustc-link-arg=-Tlinkall.x");
    println!("cargo::rustc-link-arg=-Tembedded-test.x");  # 重要
}
```

#### テストコード例 (tests/example_test.rs)
```rust
#![no_std]
#![no_main]

#[embedded_test::setup]
fn setup_log() {
    rtt_target::rtt_init_log!();
}

#[cfg(test)]
#[embedded_test::tests(executor=esp_rtos::embassy::Executor::new())]
mod tests {
    #[init]
    async fn init() -> MyPeripherals {
        // テスト前の初期化処理
        let peripherals = esp_hal::init(esp_hal::Config::default());
        MyPeripherals { /* ... */ }
    }

    #[test]
    async fn test_sensor_reading(state: MyPeripherals) {
        // 実際のハードウェアを使ったテスト
        let value = read_sensor(&state.adc).await;
        assert!(value > 0);
    }

    #[test]
    #[should_panic]
    fn test_error_handling() {
        panic!("Expected error");
    }

    #[test]
    #[timeout(5)]
    fn test_with_timeout() {
        // 5秒タイムアウトのテスト
    }
}
```

---

### 4. 本プロジェクトへの適用可能性

#### ✅ 適用可能なテスト領域
- **ハードウェア統合テスト**: センサー読み取り、GPIO制御などの実機動作確認
- **ペリフェラル動作確認**: ADC、1-Wire、I2Cなどの実動作テスト
- **エンドツーエンドテスト**: 実際のデータ収集→処理→送信フローの確認
- **タイミング依存処理**: 実際のクロック速度での動作確認

#### ❌ 不向きなテスト領域
- **純粋なロジックテスト**: ホストマシンでのユニットテストが効率的
- **頻繁な実行が必要なテスト**: フラッシュ＋リセットのオーバーヘッドあり
- **WiFi/ESP-NOW完全テスト**: 実機テストでも環境依存が大きい

#### 本プロジェクトでの位置づけ
```
テスト戦略のレイヤー構造:
┌────────────────────────────────────────┐
│ Layer 3: 手動実機テスト               │ <- 最終動作確認
│   (WiFi環境、複数デバイス連携)        │
├────────────────────────────────────────┤
│ Layer 2: probe-rs実機テスト           │ <- NEW! 今回提案
│   (センサー、GPIO、ADCの動作確認)     │
├────────────────────────────────────────┤
│ Layer 1: ホストユニットテスト         │ <- 既存推奨戦略
│   (計算ロジック、データ変換)          │
└────────────────────────────────────────┘
```

---

### 5. 導入のメリット・デメリット

#### ✅ メリット
1. **実機での自動テスト**: 実際のハードウェアを使った再現性のあるテスト
2. **CI/CD統合可能**: GitHub Actionsでのself-hosted runner利用可能
3. **開発効率向上**: `cargo test`でテストが実行でき、IDEとの統合も良好
4. **ハードウェア不具合の早期発見**: ホストテストでは検出不可能な問題を発見
5. **外部ハードウェア不要**: ESP32-S3内蔵のUSB-JTAGを使用
6. **各テストの独立性**: テストごとにデバイスリセット（状態クリーン）

#### ❌ デメリット
1. **実行速度**: フラッシュ＋リセットのため、ホストテストより遅い
2. **ESP32-S3サポートの成熟度**: ARM/RISC-Vより不安定な可能性
3. **学習コスト**: セミホスティング、リンカスクリプトなどの理解が必要
4. **デバッグの複雑性**: テスト失敗時のデバッグがホストより難しい
5. **ハードウェア依存**: 実機が必要（CIでは専用ランナー必要）

---

### 6. 導入ステップ（推奨）

#### Phase 1: 環境構築とPoC（1〜2日）
1. probe-rsのインストール
   ```bash
   cargo install probe-rs-tools --locked
   ```
2. シンプルなテストケースの作成
   - 1つのGPIOテストまたはシンプルなADCテスト
3. `.cargo/config.toml`と`build.rs`の設定
4. `cargo test`での実行確認

#### Phase 2: 主要センサーのテスト追加（1週間）
1. 電圧センサー（ADC）テスト
2. 温度センサー（DS18B20）テスト
3. ECセンサーテスト
4. カメラ初期化テスト（撮影は除く）

#### Phase 3: CI/CD統合（1週間）
1. ローカルでの安定性確認
2. Self-hosted runner構築（ESP32-S3接続済みMac）
3. GitHub Actions ワークフロー作成
4. PR時の自動テスト実行設定

---

### 7. 技術的課題と対策

| 課題 | 影響度 | 対策 |
|-----|-------|-----|
| Xtensaサポートの不安定性 | 中 | 定期的なprobe-rsアップデート、イシュートラッキング |
| WiFi/ESP-NOWテスト不可 | 低 | Layer 3（手動テスト）で補完 |
| テスト実行時間 | 中 | ホストテストと併用、重要テストに絞る |
| CI環境構築 | 高 | Self-hosted runnerの準備、段階的導入 |
| セミホスティングの知識 | 低 | embedded-testが抽象化、テンプレート活用 |

---

### 8. コスト・ベネフィット分析

#### 初期投資
- 学習時間: 2〜3日
- 実装時間: 1〜2週間
- CI環境構築: 1週間（Self-hosted runner）

#### 継続的なベネフィット
- ハードウェア不具合の早期発見
- リグレッションテストの自動化
- リリース品質の向上
- デバッグ時間の削減（特にセンサー関連）

#### ROI評価: ⭐⭐⭐⭐☆ (4/5)
QEMUと比較して**実用性が高く、投資価値あり**。
ただし、ホストユニットテストを優先し、その補完として導入すべき。

---

### 9. 最終推奨事項

#### ✅ 推奨：段階的に導入
probe-rs + embedded-testは**ESP32-S3プロジェクトに有効**だが、既存の戦略と組み合わせることを推奨：

```
優先順位:
1. ホストユニットテスト（Phase 1から継続）
   └─ 計算ロジック、データ変換、設定パース
2. probe-rs実機テスト（新規導入）
   └─ センサー読み取り、ペリフェラル動作確認
3. 手動実機テスト
   └─ WiFi/ESP-NOW、カメラ撮影、システム全体
```

#### 導入判断基準
**以下の条件を満たす場合、導入を推奨：**
- ✅ ハードウェア関連の不具合が頻発している
- ✅ センサーやペリフェラルのリグレッションテストが必要
- ✅ CI/CD環境でSelf-hosted runnerが利用可能
- ✅ チームにembedded開発の経験がある

**以下の場合、導入を延期：**
- ❌ まだホストユニットテストが未整備
- ❌ プロジェクトが初期段階で仕様変更が頻繁
- ❌ CI環境のセットアップリソースがない

---

### 10. 次のアクション

#### 即座に実行可能な項目
1. ✅ probe-rs調査完了
2. **Phase 1の実装開始** (推奨)
   - probe-rsツールのインストール
   - シンプルなGPIOテストでPoC
   - 実行可能性の確認

#### 中期的な項目（1〜2ヶ月）
1. センサーテストの段階的追加
2. テストカバレッジの拡充
3. CI/CD環境の整備検討

#### 長期的な項目（3ヶ月以上）
1. 全ペリフェラルのテストカバレッジ達成
2. Self-hosted runnerでの自動テスト
3. プロジェクトテンプレート化

---

### 11. 参考資料

- [probe-rs公式サイト](https://probe.rs/)
- [embedded-testクレート](https://crates.io/crates/embedded-test)
- [Rust on ESP Book - probe-rs](https://docs.espressif.com/projects/rust/book/getting-started/tooling/probe-rs.html)
- [embedded-test ESP32C6サンプル](https://github.com/probe-rs/embedded-test/tree/master/examples/esp32c6)
- [probe-rs ESP32-S3 Issues](https://github.com/probe-rs/embedded-test/issues/37)

---

**結論**: probe-rs + embedded-testは、QEMUと異なり**実用的で導入価値が高い**。
ホストユニットテストを基盤とし、その上に実機テストレイヤーを追加することで、
効率的かつ信頼性の高いテスト戦略を構築できる。