use esp_idf_svc::hal::delay::FreeRtos;
use log::{error, info, warn};

use crate::communication::esp_now::EspNowSender;
use crate::core::config::AppConfig;
use crate::hardware::camera::{CameraController, M5UnitCamConfig};
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
}

impl MeasuredData {
    pub fn new(voltage_percent: u8, image_data: Option<Vec<u8>>) -> Self {
        Self {
            voltage_percent,
            image_data,
        }
    }
}

/// データサービス - データ収集と送信を管理
pub struct DataService;

impl DataService {
    /// ADC電圧レベルに基づいて画像キャプチャを実行
    pub fn capture_image_if_voltage_sufficient(
        voltage_percent: u8,
        camera_pins: crate::hardware::CameraPins,
        _app_config: &AppConfig,
        led: &mut StatusLed,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        // ADC電圧条件をチェック
        if voltage_percent <= LOW_VOLTAGE_THRESHOLD_PERCENT {
            warn!("ADC電圧が低すぎるため画像キャプチャをスキップします: {}%", voltage_percent);
            return Ok(None);
        }

        if voltage_percent >= 255 {
            warn!("ADC電圧測定値が異常です: {}%", voltage_percent);
            return Ok(None);
        }

        info!("電圧条件OK({}%)、画像キャプチャを開始", voltage_percent);
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
            M5UnitCamConfig::default(),
        )?;

        FreeRtos::delay_ms(100); // カメラの安定化を待つ

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
        match esp_now_sender.send_image_chunks(
            image_data,
            250,  // チャンクサイズ (以前の動作していた値)
            5,    // チャンク間の遅延(ms) (以前の動作していた値)
        ) {
            Ok(_) => {
                info!("画像データの送信が完了しました");
                led.blink_success()?;
            }
            Err(e) => {
                error!("画像データの送信に失敗しました: {:?}", e);
                led.blink_error()?;
                return Err(anyhow::anyhow!("データ送信エラー: {:?}", e));
            }
        }

        led.turn_off()?;
        Ok(())
    }
}
