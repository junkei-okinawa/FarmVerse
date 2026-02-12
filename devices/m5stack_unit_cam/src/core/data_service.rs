use esp_idf_svc::hal::delay::FreeRtos;
use log::{error, info, warn};

use crate::communication::esp_now::EspNowSender;
use crate::core::{should_capture_image, INVALID_VOLTAGE_PERCENT, LOW_VOLTAGE_THRESHOLD_PERCENT};
use crate::core::config::AppConfig;
use crate::core::prepare_image_payload;
use crate::hardware::camera::{CameraController, M5UnitCamConfig};
use crate::hardware::led::StatusLed;

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
        if !should_capture_image(voltage_percent) {
            if voltage_percent <= LOW_VOLTAGE_THRESHOLD_PERCENT {
                warn!("ADC電圧が低すぎるため画像キャプチャをスキップします: {}%", voltage_percent);
            } else if voltage_percent >= INVALID_VOLTAGE_PERCENT {
                warn!("ADC電圧測定値が異常です: {}%", voltage_percent);
            }
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

        // カメラウォームアップ（設定回数分画像を捨てる）
        let warmup_count = _app_config.camera_warmup_frames.unwrap_or(0);
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

        // 画像データの処理と送信
        let (image_data, _hash) = prepare_image_payload(measured_data.image_data);
        if image_data.is_empty() {
            warn!("画像データなし、ダミーデータを送信");
        } else {
            info!("画像データを送信中: {} bytes", image_data.len());
        }

        // 設定されたサーバーMACアドレスを使用
        info!("設定されたサーバーMACアドレス: {}", app_config.receiver_mac);
        
        // 画像データを送信（チャンク形式 - 設定値を使用）
        match esp_now_sender.send_image_chunks(
            image_data,
            app_config.esp_now_chunk_size as usize,  // 設定からチャンクサイズを取得
            app_config.esp_now_chunk_delay_ms,  // 設定からチャンク間遅延を取得
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
        let current_time = "2025/06/22 12:00:00.000"; // 簡易タイムスタンプ
        match esp_now_sender.send_hash_frame(
            &_hash,
            measured_data.voltage_percent,
            None,
            None,
            current_time,
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
