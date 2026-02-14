use esp_idf_svc::hal::delay::FreeRtos;
use log::{error, info, warn};

use crate::communication::esp_now::EspNowSender;
use crate::config::AppConfig;
use crate::core::MeasuredData;
use crate::hardware::camera::{CameraController, CamConfig, reset_camera_pins};
use crate::hardware::led::StatusLed;

/// 低電圧閾値（パーセンテージ）
const LOW_VOLTAGE_THRESHOLD_PERCENT: u8 = 8;

/// ダミーハッシュ（SHA256の64文字）
const DUMMY_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// データサービス - データ収集と送信を管理
pub struct DataService;

impl DataService {
    /// ADC電圧レベルに基づいて画像キャプチャを実行
    pub fn capture_image_if_voltage_sufficient(
        voltage_percent: u8,
        camera_pins: crate::hardware::CameraPins,
        app_config: &AppConfig,
        led: &mut StatusLed,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        // デバッグモードの場合は詳細ログを出力
        if app_config.debug_mode {
            info!("🔧 デバッグ: 画像キャプチャ開始 - 電圧:{}%, force_camera_test:{}, bypass_voltage_threshold:{}", 
                voltage_percent, app_config.force_camera_test, app_config.bypass_voltage_threshold);
        }

        // 電圧チェック（bypass_voltage_thresholdが有効な場合はスキップ）
        let should_capture_by_voltage = if app_config.bypass_voltage_threshold {
            if app_config.debug_mode {
                info!("🔧 デバッグ: 電圧閾値チェックをバイパス中");
            }
            true
        } else if voltage_percent <= LOW_VOLTAGE_THRESHOLD_PERCENT {
            warn!("ADC電圧が低すぎるため画像キャプチャをスキップします: {}%", voltage_percent);
            false
        } else if voltage_percent >= 255 {
            warn!("ADC電圧測定値が異常です: {}%", voltage_percent);
            false
        } else {
            true
        };

        // カメラテスト強制実行の場合
        let force_capture = app_config.force_camera_test;
        if force_capture && app_config.debug_mode {
            info!("🔧 デバッグ: カメラテストを強制実行中");
        }

        // キャプチャ実行判定
        if !should_capture_by_voltage && !force_capture {
            return Ok(None);
        }

        info!("画像キャプチャを開始 (電圧:{}%, 強制実行:{})", voltage_percent, force_capture);
        led.turn_on()?;

        // カメラ初期化とキャプチャ
        let camera = CameraController::new(
            camera_pins.clock,
            camera_pins.d0,
            camera_pins.d1,
            camera_pins.d2,
            camera_pins.d3,
            camera_pins.d4,
            camera_pins.d5,
            camera_pins.d6,
            camera_pins.d7,
            camera_pins.vsync,
            camera_pins.href,
            camera_pins.pclk,
            camera_pins.sda,
            camera_pins.scl,
            20_000_000, // クロック周波数 (20MHz)
            12,
            2,
            esp_idf_sys::camera::camera_grab_mode_t_CAMERA_GRAB_LATEST,
            CamConfig::default(),
        )?;

        FreeRtos::delay_ms(100); // カメラの安定化を待つ

        // カメラウォームアップ（設定回数分画像を捨てる）
        let warmup_count = app_config.camera_warmup_frames.unwrap_or(0);
        for i in 0..warmup_count {
            let _ = camera.capture_image();
            info!("ウォームアップキャプチャ {} / {}", i + 1, warmup_count);
            FreeRtos::delay_ms(1000);
        }

        let image_data = {
            let frame_buffer = camera.capture_image()?;
            frame_buffer.data().to_vec()
        };
        info!("画像キャプチャ完了: {} bytes", image_data.len());

        // [CASE 4] カメラをソフトウェアスタンバイモードに移行
        // PWDNピンがないため、SCCB経由でスリープ命令を送る必要がある
        if let Err(e) = camera.standby() {
            warn!("カメラのスタンバイ移行に失敗しました: {:?}", e);
        }

        // 明示的にControllerをドロップしてカメラドライバを解放する（Dropトレイトでdeinitされる）
        drop(camera);
        
        // [CASE 3] カメラピンをプルダウン状態にリセットしてリークを遮断
        // Light Sleep復帰時のホールド解除処理を追加したため有効化
        reset_camera_pins();

        led.turn_off()?;
        Ok(Some(image_data))
    }

    /// 測定データを送信
    pub fn transmit_data(
        app_config: &AppConfig,
        esp_now_sender: &EspNowSender,
        led: &mut StatusLed,
        measured_data: MeasuredData,
    ) -> anyhow::Result<()> {
        led.turn_on()?;

        // デバッグモードの場合は詳細ログを出力
        if app_config.debug_mode {
            info!("🔧 デバッグ: データ送信開始 - 画像データサイズ:{} bytes", 
                measured_data.image_data.as_ref().map_or(0, |data| data.len()));
        }

        // 画像データの処理と送信
        let (image_data, _hash) = if let Some(data) = measured_data.image_data {
            if data.is_empty() {
                warn!("画像データが空です");
                (vec![], DUMMY_HASH.to_string())
            } else {
                info!("画像データを送信中: {} bytes", data.len());
                // 簡単なハッシュ計算（画像サイズとチェックサムベース）
                let hash = format!("{:08x}{:08x}", data.len(), data.iter().map(|&b| b as u32).sum::<u32>());
                (data, hash)
            }
        } else {
            info!("画像データなし、ダミーデータを送信");
            (vec![], DUMMY_HASH.to_string())
        };

        // 設定されたサーバーMACアドレスを使用
        info!("設定されたサーバーMACアドレス: {}", app_config.receiver_mac);
        
        // 画像データを送信（チャンク形式 - 設定値を使用）
        match esp_now_sender.send_image_chunks(
            image_data,
            app_config.esp_now_chunk_size as usize,  // 設定からチャンクサイズを取得
            app_config.esp_now_chunk_delay_ms as u32,  // 設定からチャンク間遅延を取得
        ) {
            Ok(_) => {
                info!("画像データの送信が完了しました");
            }
            Err(e) => {
                error!("画像データの送信に失敗しました: {:?}", e);
                led.blink_error()?;
                return Err(anyhow::anyhow!("データ送信エラー: {:?}", e));
            }
        }

        // HASHフレームを送信（サーバーがスリープコマンドを送信するために必要）
        // 取得失敗の場合はダミー値 1900/01/01 00:00:00.000 を使用
        let current_time = chrono::Utc::now().timestamp();
        let datetime = chrono::DateTime::from_timestamp(current_time, 0).unwrap_or_default();
        let formatted_time = datetime.format("%Y/%m/%d %H:%M:%S%.3f").to_string();

        match esp_now_sender.send_hash_frame(
            &_hash, 
            measured_data.voltage_percent, 
            measured_data.temperature_celsius,
            measured_data.tds_voltage,
            &formatted_time
        ) {
            Ok(_) => {
                info!("HASHフレームの送信が完了しました");
            }
            Err(e) => {
                error!("HASHフレームの送信に失敗しました: {:?}", e);
                led.blink_error()?;
                return Err(anyhow::anyhow!("HASHフレーム送信エラー: {:?}", e));
            }
        }

        // EOFマーカーを送信（画像送信完了を示す）
        match esp_now_sender.send_eof_marker() {
            Ok(_) => {
                info!("EOFマーカーの送信が完了しました");
                led.blink_success()?;
                
                // EOFマーカーが確実にサーバーに届くまで追加待機
                info!("EOFマーカー最終配信確認のため追加待機中...");
                esp_idf_svc::hal::delay::FreeRtos::delay_ms(200);
                info!("EOFマーカー送信プロセス完全完了");
            }
            Err(e) => {
                error!("EOFマーカーの送信に失敗しました: {:?}", e);
                led.blink_error()?;
                return Err(anyhow::anyhow!("EOFマーカー送信エラー: {:?}", e));
            }
        }

        led.turn_off()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_measured_data_new() {
        let data = MeasuredData::new(50, None);
        assert_eq!(data.voltage_percent, 50);
        assert!(data.image_data.is_none());
        assert!(data.temperature_celsius.is_none());
        assert!(data.tds_voltage.is_none());
        assert!(data.tds_ppm.is_none());
        assert!(data.sensor_warnings.is_empty());
    }

    #[test]
    fn test_measured_data_with_temperature() {
        let data = MeasuredData::new(75, None)
            .with_temperature(Some(25.5));
        
        assert_eq!(data.voltage_percent, 75);
        assert_eq!(data.temperature_celsius, Some(25.5));
    }

    #[test]
    fn test_measured_data_with_tds() {
        let data = MeasuredData::new(80, None)
            .with_tds_voltage(Some(1.5))
            .with_tds(Some(450.0));
        
        assert_eq!(data.tds_voltage, Some(1.5));
        assert_eq!(data.tds_ppm, Some(450.0));
    }

    #[test]
    fn test_measured_data_add_warning() {
        let mut data = MeasuredData::new(30, None);
        data.add_warning("Low voltage detected".to_string());
        data.add_warning("Sensor timeout".to_string());
        
        assert_eq!(data.sensor_warnings.len(), 2);
        assert_eq!(data.sensor_warnings[0], "Low voltage detected");
        assert_eq!(data.sensor_warnings[1], "Sensor timeout");
    }

    #[test]
    fn test_get_summary_voltage_only() {
        let data = MeasuredData::new(85, None);
        let summary = data.get_summary();
        
        assert_eq!(summary, "電圧:85%");
    }

    #[test]
    fn test_get_summary_with_temperature() {
        let data = MeasuredData::new(70, None)
            .with_temperature(Some(23.7));
        let summary = data.get_summary();
        
        assert_eq!(summary, "電圧:70%, 温度:23.7°C");
    }

    #[test]
    fn test_get_summary_with_tds() {
        let data = MeasuredData::new(60, None)
            .with_tds_voltage(Some(1.23))
            .with_tds(Some(567.8));
        let summary = data.get_summary();
        
        assert_eq!(summary, "電圧:60%, TDS電圧:1.23V, TDS:567.8ppm");
    }

    #[test]
    fn test_get_summary_with_image() {
        let image_data = vec![1, 2, 3, 4, 5];
        let data = MeasuredData::new(90, Some(image_data));
        let summary = data.get_summary();
        
        assert_eq!(summary, "電圧:90%, 画像:5bytes");
    }

    #[test]
    fn test_get_summary_with_warnings() {
        let mut data = MeasuredData::new(40, None);
        data.add_warning("Warning 1".to_string());
        data.add_warning("Warning 2".to_string());
        let summary = data.get_summary();
        
        assert_eq!(summary, "電圧:40%, 警告:2件");
    }

    #[test]
    fn test_get_summary_full_data() {
        let image_data = vec![0; 1024];
        let mut data = MeasuredData::new(95, Some(image_data))
            .with_temperature(Some(26.3))
            .with_tds_voltage(Some(2.15))
            .with_tds(Some(890.5));
        data.add_warning("Test warning".to_string());
        
        let summary = data.get_summary();
        
        assert!(summary.contains("電圧:95%"));
        assert!(summary.contains("温度:26.3°C"));
        assert!(summary.contains("TDS電圧:2.15V"));
        assert!(summary.contains("TDS:890.5ppm"));
        assert!(summary.contains("画像:1024bytes"));
        assert!(summary.contains("警告:1件"));
    }
}
