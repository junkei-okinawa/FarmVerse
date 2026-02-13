use esp_camera_rs::{Camera, CameraParams, FrameBuffer};
use esp_idf_svc::hal::gpio;
use esp_idf_sys::camera::*;
use log::{error, info, warn}; // logクレートの必要な要素をインポート
use std::sync::Arc;
use super::ov2640_sequence::{resume_sequence, standby_clkrc_write, standby_sequence, RegWrite};

#[derive(Debug, Clone, Copy)] // Added Clone
pub enum CustomFrameSize {
    /// 96x96 解像度
    Qcif = framesize_t_FRAMESIZE_QCIF as isize,
    /// QQVGA 解像度
    Qqvga = framesize_t_FRAMESIZE_QQVGA as isize,
    /// 240x240 解像度
    _240x240 = framesize_t_FRAMESIZE_240X240 as isize,
    /// QVGA 解像度
    Qvga = framesize_t_FRAMESIZE_QVGA as isize,
    /// CIF 解像度
    Cif = framesize_t_FRAMESIZE_CIF as isize,
    /// HVGA 解像度
    Hvga = framesize_t_FRAMESIZE_HVGA as isize,
    /// VGA 解像度
    Vga = framesize_t_FRAMESIZE_VGA as isize,
    /// SVGA 解像度
    Svga = framesize_t_FRAMESIZE_SVGA as isize,
    /// XGA 解像度
    Xga = framesize_t_FRAMESIZE_XGA as isize,
    /// HD 解像度
    Hd = framesize_t_FRAMESIZE_HD as isize,
    /// SXGA 解像度
    Sxga = framesize_t_FRAMESIZE_SXGA as isize,
    /// UXGA 解像度
    Uxga = framesize_t_FRAMESIZE_UXGA as isize,
    /// FHD 解像度
    Fhd = framesize_t_FRAMESIZE_FHD as isize,
    /// P_HD 解像度
    PHd = framesize_t_FRAMESIZE_P_HD as isize, // Corrected from P_hd
    /// P_3MP 解像度
    P3mp = framesize_t_FRAMESIZE_P_3MP as isize, // Corrected from P_3mp
    /// QXGA 解像度
    Qxga = framesize_t_FRAMESIZE_QXGA as isize,
    /// QHD 解像度
    Qhd = framesize_t_FRAMESIZE_QHD as isize,
    /// WQXGA 解像度
    Wqxga = framesize_t_FRAMESIZE_WQXGA as isize,
    /// P_FHD 解像度
    PFhd = framesize_t_FRAMESIZE_P_FHD as isize, // Corrected from P_fhd
    /// QSXGA 解像度
    Qsxga = framesize_t_FRAMESIZE_QSXGA as isize,
}

#[derive(Clone, Debug)] // Added Clone
pub struct M5UnitCamConfig {
    // pub with_psram: bool, // Removed unused field
    pub frame_size: CustomFrameSize,
    // pub jpeg_quality: i32, // JPEG品質設定は現在エラーを引き起こすため削除 (NO-EOI error)
}

impl Default for M5UnitCamConfig {
    fn default() -> Self {
        Self {
            frame_size: CustomFrameSize::Svga, // デフォルトはSVGA
                                               // jpeg_quality: 12, // デフォルトのJPEG品質、現在は未使用
        }
    }
}

impl M5UnitCamConfig {
    /// 文字列から framesize_t 定数を取得します
    pub fn from_string(size_str: &str) -> CustomFrameSize {
        match size_str.to_uppercase().as_str() {
            "96X96" => CustomFrameSize::Qcif,
            "QQVGA" => CustomFrameSize::Qqvga,
            "QCIF" => CustomFrameSize::Qcif,
            "HQVGA" => CustomFrameSize::Hvga, // Assuming HQVGA maps to Hvga based on previous logic
            "240X240" => CustomFrameSize::_240x240,
            "QVGA" => CustomFrameSize::Qvga,
            "CIF" => CustomFrameSize::Cif,
            "HVGA" => CustomFrameSize::Hvga,
            "VGA" => CustomFrameSize::Vga,
            "SVGA" => CustomFrameSize::Svga,
            "XGA" => CustomFrameSize::Xga,
            "HD" => CustomFrameSize::Hd,
            "SXGA" => CustomFrameSize::Sxga,
            "UXGA" => CustomFrameSize::Uxga,
            "FHD" => CustomFrameSize::Fhd,
            "P_HD" => CustomFrameSize::PHd,   // Corrected
            "P_3MP" => CustomFrameSize::P3mp, // Corrected
            "QXGA" => CustomFrameSize::Qxga,
            "QHD" => CustomFrameSize::Qhd,
            "WQXGA" => CustomFrameSize::Wqxga,
            "P_FHD" => CustomFrameSize::PFhd, // Corrected
            "QSXGA" => CustomFrameSize::Qsxga,
            _ => {
                warn!(
                    "無効なフレームサイズ '{}' が指定されました。デフォルトの SVGA を使用します。",
                    size_str
                );
                CustomFrameSize::Svga // デフォルト値
            }
        }
    }

    /// CameraPinsから設定を作成
    pub fn from_pins(_pins: crate::hardware::CameraPins) -> Self {
        // ピンは使用されるが、現在の実装では設定に直接影響しない
        // 将来的にピン設定に基づいた最適化が可能
        Self::default()
    }
}

/// カメラ制御に関するエラー
#[derive(Debug, thiserror::Error)]
pub enum CameraError {
    #[error("カメラの初期化に失敗しました: {0}")]
    InitFailed(String),

    #[error("画像キャプチャに失敗しました")]
    CaptureFailed,
}

/// M5Stack Unit Cam (ESP32)向けのカメラコントローラー
pub struct CameraController {
    camera: Arc<Camera<'static>>,
}

impl CameraController {
    /// ペリフェラルから新しいカメラコントローラーを作成します
    ///
    /// # 引数
    ///
    /// * `clock_pin` - カメラクロックピン (gpio27)
    /// * `d0_pin` - データピン0 (gpio32)
    /// * `d1_pin` - データピン1 (gpio35)
    /// * `d2_pin` - データピン2 (gpio34)
    /// * `d3_pin` - データピン3 (gpio5)
    /// * `d4_pin` - データピン4 (gpio39)
    /// * `d5_pin` - データピン5 (gpio18)
    /// * `d6_pin` - データピン6 (gpio36)
    /// * `d7_pin` - データピン7 (gpio19)
    /// * `vsync_pin` - 垂直同期ピン (gpio22)
    /// * `href_pin` - 水平同期ピン (gpio26)
    /// * `pclk_pin` - ピクセルクロックピン (gpio21)
    /// * `sda_pin` - I2C SDAピン (gpio25)
    /// * `scl_pin` - I2C SCLピン (gpio23)
    /// * `config` - 追加の構成パラメータ
    ///
    /// # エラー
    ///
    /// カメラの初期化に失敗した場合にエラーを返します
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        clock_pin: gpio::Gpio27,
        d0_pin: gpio::Gpio32,
        d1_pin: gpio::Gpio35,
        d2_pin: gpio::Gpio34,
        d3_pin: gpio::Gpio5,
        d4_pin: gpio::Gpio39,
        d5_pin: gpio::Gpio18,
        d6_pin: gpio::Gpio36,
        d7_pin: gpio::Gpio19,
        vsync_pin: gpio::Gpio22,
        href_pin: gpio::Gpio26,
        pclk_pin: gpio::Gpio21,
        sda_pin: gpio::Gpio25,
        scl_pin: gpio::Gpio23,
        config: M5UnitCamConfig,
    ) -> Result<Self, CameraError> {
        info!("カメラを初期化しています");

        let camera_params = CameraParams::new()
            .set_clock_pin(clock_pin)
            .set_d0_pin(d0_pin)
            .set_d1_pin(d1_pin)
            .set_d2_pin(d2_pin)
            .set_d3_pin(d3_pin)
            .set_d4_pin(d4_pin)
            .set_d5_pin(d5_pin)
            .set_d6_pin(d6_pin)
            .set_d7_pin(d7_pin)
            .set_vertical_sync_pin(vsync_pin)
            .set_horizontal_reference_pin(href_pin)
            .set_pixel_clock_pin(pclk_pin)
            .set_sda_pin(sda_pin)
            .set_scl_pin(scl_pin)
            .set_frame_size(config.frame_size as u32) // Cast to u32
            // .set_jpeg_quality(config.jpeg_quality) // 注意: この設定を有効にすると `cam_hal: NO-EOI` エラーが発生する (2025-05-09時点)
            .set_fb_location(camera_fb_location_t_CAMERA_FB_IN_DRAM);

        let camera =
            Camera::new(&camera_params).map_err(|e| CameraError::InitFailed(format!("{:?}", e)))?;

        Ok(Self {
            camera: Arc::new(camera),
        })
    }

    /// 画像を撮影します
    ///
    /// 最初のフレームは捨てて、2枚目のフレームを返します。
    /// これは一部のカメラで最初のフレームが適切に露出調整されないことがあるためです。
    ///
    /// # エラー
    ///
    /// 画像キャプチャに失敗した場合にエラーを返します
    pub fn capture_image(&self) -> Result<FrameBuffer<'_>, CameraError> {
        self.camera
            .get_framebuffer()
            .ok_or(CameraError::CaptureFailed)
    }

    /// 露光設定を行います。
    ///
    /// # 引数
    /// * `auto` - true の場合、自動露出制御 (AEC) を有効にします。false の場合、AECを無効にします。
    /// * `manual_value` - `auto` が false の場合に手動で設定する露光値 (AEC value)。
    ///                  `lib.rs` の `set_aec_value` が `i32` 型を期待するようになったため、
    ///                  この `i32` 値が直接露光関連の設定値として使用されます。
    pub fn configure_exposure(
        &self,
        auto: bool,
        manual_value: Option<i32>,
    ) -> Result<(), CameraError> {
        let sensor = self.camera.sensor();
        if auto {
            sensor
                .set_exposure_ctrl(true) // sensor.set_exposure_ctrl(bool) を使用
                .map_err(|e| CameraError::InitFailed(format!("自動露出有効化エラー: {:?}", e)))?;
            info!("自動露出制御 (AEC) を有効にしました。");
        } else {
            sensor
                .set_exposure_ctrl(false) // sensor.set_exposure_ctrl(bool) を使用
                .map_err(|e| CameraError::InitFailed(format!("自動露出無効化エラー: {:?}", e)))?;
            info!("自動露出制御 (AEC) を無効にしました。");

            if let Some(value_i32) = manual_value {
                // sensor.set_aec_value() は lib.rs により i32 型の引数を取ります。
                // これにより、指定された value_i32 が露光レベルとして設定されます。
                sensor
                    .set_aec_value(value_i32) // sensor.set_aec_value(i32) を使用
                    .map_err(|e| {
                        CameraError::InitFailed(format!(
                            "set_aec_value({}) 呼び出しエラー: {:?}",
                            value_i32, e
                        ))
                    })?;
                info!("手動露光値 (AEC value) を {} に設定しました。", value_i32);
            } else {
                info!("手動露光値が指定されなかったため、AEC value の設定はスキップされました。");
            }
        }
        Ok(())
    }

    /// 現在のAEC値を取得します。
    ///
    /// `lib.rs` の `aec_value` ゲッターは `i32` 型の値を返します。
    pub fn get_current_aec_value(&self) -> i32 {
        self.camera.sensor().aec_value() // sensor.aec_value() -> i32 を使用
    }

    /// OV2640をSCCB経由でソフトウェアスタンバイに移行します。
    ///
    /// 注意:
    /// - COM2(0x09) Standby bitは使いません（SDAロック既知問題のため）。
    /// - COM7(0x12) Bit4 + CLKRC分周で省電力化を試みます。
    pub fn enter_standby_via_sccb(&self) -> Result<(), CameraError> {
        info!("カメラをSCCB経由でスタンバイモードに移行します...");
        let sensor = self.camera.sensor();

        for write in standby_sequence() {
            apply_reg_write(&sensor, write)?;
        }
        // Keep CLKRC write explicit for easier tuning.
        apply_reg_write(&sensor, standby_clkrc_write())?;

        info!("✓ カメラをSCCBスタンバイに移行しました");
        Ok(())
    }

    /// SCCBスタンバイから復帰します。
    ///
    /// COM7 Bit4をクリアし、CLKRCをデフォルト寄りに戻します。
    pub fn exit_standby_via_sccb(&self) -> Result<(), CameraError> {
        info!("カメラのSCCBスタンバイを解除します...");
        let sensor = self.camera.sensor();

        for write in resume_sequence() {
            apply_reg_write(&sensor, write)?;
        }

        info!("✓ カメラのSCCBスタンバイを解除しました");
        Ok(())
    }
}

fn apply_reg_write(sensor: &esp_camera_rs::Sensor, write: RegWrite) -> Result<(), CameraError> {
    sensor
        .set_reg(write.reg, write.mask, write.value)
        .map_err(|e| {
            CameraError::InitFailed(format!(
                "SCCBレジスタ設定失敗 reg=0x{:02X} mask=0x{:02X} value=0x{:02X}: {:?}",
                write.reg, write.mask, write.value, e
            ))
        })
}

#[cfg(test)]
mod tests {
    // テストはハードウェア依存のため省略
}
