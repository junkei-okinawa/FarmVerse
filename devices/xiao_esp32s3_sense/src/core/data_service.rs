use esp_idf_svc::hal::delay::FreeRtos;
use log::{error, info, warn};
use chrono::{DateTime, NaiveDateTime, Utc};

use crate::communication::esp_now::EspNowSender;
use crate::config::AppConfig;
use crate::hardware::camera::{CameraController, CamConfig};
use crate::hardware::led::StatusLed;

/// 低電圧閾値（パーセンテージ）
const LOW_VOLTAGE_THRESHOLD_PERCENT: u8 = 8;

/// ダミーハッシュ（SHA256の64文字）
const DUMMY_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// 測定データ構造体
#[derive(Debug)]
pub struct MeasuredData {
    pub voltage_percent: u8,
    pub image_data: Option<Vec<u8>>,
    pub temperature_celsius: Option<f32>,
    pub tds_voltage: Option<f32>,
    pub tds_ppm: Option<f32>,
    pub sensor_warnings: Vec<String>,
}

impl MeasuredData {
    pub fn new(voltage_percent: u8, image_data: Option<Vec<u8>>) -> Self {
        Self {
            voltage_percent,
            image_data,
            temperature_celsius: None,
            tds_voltage: None,
            tds_ppm: None,
            sensor_warnings: Vec::new(),
        }
    }

    /// 温度データを追加
    pub fn with_temperature(mut self, temperature: Option<f32>) -> Self {
        self.temperature_celsius = temperature;
        self
    }

    /// TDS電圧データを追加
    pub fn with_tds_voltage(mut self, voltage: Option<f32>) -> Self {
        self.tds_voltage = voltage;
        self
    }
    
    /// TDSデータを追加
    pub fn with_tds(mut self, tds: Option<f32>) -> Self {
        self.tds_ppm = tds;
        self
    }

    /// 警告メッセージを追加
    pub fn add_warning(&mut self, warning: String) {
        self.sensor_warnings.push(warning);
    }

    /// 測定データのサマリを取得
    pub fn get_summary(&self) -> String {
        let mut parts = vec![format!("電圧:{}%", self.voltage_percent)];

        if let Some(temp) = self.temperature_celsius {
            parts.push(format!("温度:{:.1}°C", temp));
        }

        if let Some(voltage) = self.tds_voltage {
            parts.push(format!("TDS電圧:{:.2}V", voltage));
        }

        if let Some(tds) = self.tds_ppm {
            parts.push(format!("TDS:{:.1}ppm", tds));
        }

        if let Some(ref image_data) = self.image_data {
            parts.push(format!("画像:{}bytes", image_data.len()));
        }

        if !self.sensor_warnings.is_empty() {
            parts.push(format!("警告:{}件", self.sensor_warnings.len()));
        }

        parts.join(", ")
    }
}

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

        let frame_buffer = camera.capture_image()?;
        let image_data = frame_buffer.data().to_vec();
        info!("画像キャプチャ完了: {} bytes", image_data.len());

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
        let datetime: DateTime<Utc> = chrono::DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(current_time, 0), Utc);
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
                esp_idf_svc::hal::delay::FreeRtos::delay_ms(1000); // 1秒待機（改修前相当）
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
