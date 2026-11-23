# power — FarmVerse 電源システム

ソーラーパネル 1 枚と EcoFlow River 3 Plus 2 台 (A: 主、B: 副) で FarmVerse サーバー (ThinkPad) を動かすための
監視・制御。設計は [docs/3_PowerSystem/01_電源システム設計.md](../docs/3_PowerSystem/01_電源システム設計.md)。

## 構成 (予定)

| ディレクトリ | 内容 |
|---|---|
| `manager/` | ThinkPad 上の常駐プログラム。ポタ電 A / B の残量・ソーラー入力を見て、パネルの切替・スマートプラグ・ThinkPad の充電上限を制御する |
| `relay_firmware/` | XIAO ESP32C6 のファームウェア。USB シリアルの命令でパネルの切替 GPIO を操作し、命令が途絶えたら A 側に戻す |
| `hardware/` | 配線ボックスの回路図・部品表・組み立て写真 |

ポタ電の電力値の収集 (`deploy/host/farmverse-river3`) と NUT の設定は、サーバー構築側 (`deploy/host/`) にある。
