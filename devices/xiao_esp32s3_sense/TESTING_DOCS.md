# テストドキュメント

ESP32-S3実機を使用しないユニットテスト手法の調査結果

---

## 📖 ドキュメントの場所

すべてのテスト関連ドキュメントは **`tests/docs/`** ディレクトリに整理されています。

```
tests/docs/
├── 00_README.md                      # インデックス（スタート地点）
├── 01_SUMMARY.md                     # 調査結果サマリー
├── 02_STRATEGY.md                    # テスト戦略
├── 03_INVESTIGATION_REPORT.md        # 詳細調査レポート
├── 04_PROBE_RS_SETUP_GUIDE.md        # probe-rsセットアップガイド
└── 05_PROBE_RS_README.md             # 参考資料
```

---

## 🚀 クイックスタート

### 1. まずはここから読む
**[tests/docs/00_README.md](./tests/docs/00_README.md)**

ドキュメント全体のインデックスとクイックスタートガイド

### 2. エグゼクティブサマリー
**[tests/docs/01_SUMMARY.md](./tests/docs/01_SUMMARY.md)**

調査結果の要約と推奨事項（5分で読める）

### 3. 実装を始める
**[tests/docs/04_PROBE_RS_SETUP_GUIDE.md](./tests/docs/04_PROBE_RS_SETUP_GUIDE.md)**

probe-rsの具体的なセットアップ手順

---

## 📊 調査結果概要

| 手法 | 推奨度 | 用途 |
|-----|-------|------|
| ホストユニットテスト | ⭐⭐⭐⭐⭐ | 計算ロジック、データ変換 |
| probe-rs実機テスト | ⭐⭐⭐⭐☆ | センサー、GPIO、ペリフェラル |
| QEMU | ⭐☆☆☆☆ | 非推奨 |

---

## 🎯 推奨3層テスト戦略

```
Layer 1: ホストユニットテスト    （優先度: 高）
Layer 2: probe-rs実機テスト      （優先度: 中）⭐NEW
Layer 3: 手動実機テスト          （優先度: 低）
```

詳細は[tests/docs/](./tests/docs/)を参照してください。

---

**調査期間**: 2025-10-19 〜 2025-11-02  
**最終更新**: 2025-11-02
