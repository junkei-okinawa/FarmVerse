use esp_idf_svc::hal::{delay::FreeRtos, gpio::*};
use log::{error, info};

use crate::camera::{CameraController, M5UnitCamConfig};
use crate::config::AppConfig;
use crate::esp_now::{EspNowSender, ImageFrame};
use crate::led::StatusLed;

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

/// カメラ用ピンの構造体
pub struct CameraPins {
    pub clock: Gpio27,
    pub d0: Gpio32,
    pub d1: Gpio35,
    pub d2: Gpio34,
    pub d3: Gpio5,
    pub d4: Gpio39,
    pub d5: Gpio18,
    pub d6: Gpio36,
    pub d7: Gpio19,
    pub vsync: Gpio22,
    pub href: Gpio26,
    pub pclk: Gpio21,
    pub sda: Gpio25,
    pub scl: Gpio23,
}

impl CameraPins {
    /// 個別のピンから作成
    pub fn new(
        clock: Gpio27, d0: Gpio32, d1: Gpio35, d2: Gpio34,
        d3: Gpio5, d4: Gpio39, d5: Gpio18, d6: Gpio36,
        d7: Gpio19, vsync: Gpio22, href: Gpio26, pclk: Gpio21,
        sda: Gpio25, scl: Gpio23,
    ) -> Self {
        Self {
            clock, d0, d1, d2, d3, d4, d5, d6, d7,
            vsync, href, pclk, sda, scl,
        }
    }
}

/// データ収集と送信を管理するモジュール
pub struct DataCollector;

impl DataCollector {
    /// ADC電圧レベルに基づいて画像キャプチャを実行
    pub fn capture_image_if_voltage_sufficient(
        voltage_percent: u8,
        camera_pins: CameraPins,
        config: &AppConfig,
        led: &mut StatusLed,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        if voltage_percent >= LOW_VOLTAGE_THRESHOLD_PERCENT && voltage_percent != 255 {
            info!(
                "電圧 {}% (>= {}% かつ != 255%) は十分なため、カメラを初期化し画像をキャプチャします。",
                voltage_percent, LOW_VOLTAGE_THRESHOLD_PERCENT
            );

            Self::capture_camera_image(camera_pins, config, led)
        } else {
            if voltage_percent == 255 {
                info!("ADC電圧測定エラー (255%) のため、カメラ処理をスキップします。");
            } else {
                info!(
                    "ADC電圧が低い ({}% < {}%) ため、カメラ処理をスキップします。",
                    voltage_percent, LOW_VOLTAGE_THRESHOLD_PERCENT
                );
            }
            led.blink_error()?;
            Ok(None)
        }
    }

    /// カメラを初期化して画像をキャプチャ
    fn capture_camera_image(
        camera_pins: CameraPins,
        config: &AppConfig,
        led: &mut StatusLed,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        let camera_config = M5UnitCamConfig {
            frame_size: M5UnitCamConfig::from_string(&config.frame_size),
        };

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
            camera_config,
        )?;

        let current_aec_value = camera.get_current_aec_value();
        let _ = camera.configure_exposure(config.auto_exposure_enabled, Some(current_aec_value));
        
        // ウォームアップフレーム
        if let Some(warmup_frames) = config.camera_warmup_frames {
            info!("カメラウォームアップフレーム数: {}", warmup_frames);
            for _ in 0..warmup_frames {
                match camera.capture_image() {
                    Ok(_) => {
                        info!("カメラウォームアップフレームキャプチャ成功");
                    }
                    Err(e) => {
                        error!("カメラウォームアップフレームキャプチャ失敗: {:?}", e);
                        led.blink_error()?;
                    }
                }
                FreeRtos::delay_ms(1000);
            }
        }

        // 実際の画像キャプチャ
        let image_result = camera.capture_image();
        match image_result {
            Ok(fb) => {
                info!("画像キャプチャ成功: {} バイト", fb.data().len());
                let image_data = fb.data().to_vec();
                Ok(Some(image_data))
            }
            Err(e) => {
                error!("画像キャプチャ失敗 (最終): {:?}", e);
                led.blink_error()?;
                Ok(None)
            }
        }
    }

    /// 測定データをESP-NOW経由で送信
    pub fn transmit_data(
        config: &AppConfig,
        esp_now_sender: &EspNowSender,
        led: &mut StatusLed,
        measured_data: MeasuredData,
    ) -> anyhow::Result<()> {
        // タイムスタンプを準備
        let tz: chrono_tz::Tz = config.timezone.parse().unwrap_or(chrono_tz::Asia::Tokyo);
        let current_time_formatted = chrono::Utc::now()
            .with_timezone(&tz)
            .format("%Y/%m/%d %H:%M:%S%.3f")
            .to_string();

        match measured_data.image_data {
            Some(image_data) => {
                Self::transmit_with_image(
                    config,
                    esp_now_sender,
                    led,
                    &image_data,
                    measured_data.voltage_percent,
                    &current_time_formatted,
                )
            }
            None => {
                Self::transmit_without_image(
                    config,
                    esp_now_sender,
                    measured_data.voltage_percent,
                    &current_time_formatted,
                )
            }
        }
    }

    /// 画像データありで送信
    fn transmit_with_image(
        config: &AppConfig,
        esp_now_sender: &EspNowSender,
        led: &mut StatusLed,
        image_data: &[u8],
        voltage_percent: u8,
        timestamp: &str,
    ) -> anyhow::Result<()> {
        match ImageFrame::calculate_hash(image_data) {
            Ok(hash_str) => {
                let payload = format!(
                    "HASH:{},VOLT:{},{}",
                    hash_str, voltage_percent, timestamp
                );
                let payload_bytes = payload.into_bytes();

                info!(
                    "送信データ準備完了 (画像あり): ペイロードサイズ={}, 時刻={}, ハッシュ={}, 電圧={}%",
                    payload_bytes.len(), timestamp, hash_str, voltage_percent
                );

                esp_now_sender.send(&config.receiver_mac, &payload_bytes, 1000)?;
                info!("画像ハッシュ、電圧情報、時刻を送信しました。");

                // 画像データをチャンクで送信
                match esp_now_sender.send_image_chunks(&config.receiver_mac, image_data.to_vec(), 250, 5) {
                    Ok(_) => {
                        info!("画像送信完了");
                        led.indicate_sending()?;
                    }
                    Err(e) => {
                        error!("画像送信エラー: {:?}", e);
                        led.blink_error()?;
                        return Err(e.into());
                    }
                }
            }
            Err(e) => {
                error!("ハッシュ計算エラー: {:?}", e);
                led.blink_error()?;
                return Err(e.into());
            }
        }
        Ok(())
    }

    /// 画像データなしで送信（ダミーハッシュ使用）
    fn transmit_without_image(
        config: &AppConfig,
        esp_now_sender: &EspNowSender,
        voltage_percent: u8,
        timestamp: &str,
    ) -> anyhow::Result<()> {
        let payload = format!(
            "HASH:{},{},VOLT:{}",
            timestamp, DUMMY_HASH, voltage_percent
        );
        let payload_bytes = payload.into_bytes();

        info!(
            "送信データ準備完了 (画像なし - ダミーハッシュ): ペイロードサイズ={}, 時刻={}, 電圧={}%",
            payload_bytes.len(), timestamp, voltage_percent
        );

        esp_now_sender.send(&config.receiver_mac, &payload_bytes, 1000)?;
        info!("ダミーハッシュ、電圧情報、時刻を送信しました (画像なし)。");
        
        Ok(())
    }
}
