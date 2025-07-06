use crate::mac_address::MacAddress;
use log::{info, warn};
use std::str::FromStr;

/// この設定はcompile時にbuild.rsによってcfg.tomlから読み込まれる
#[toml_cfg::toml_config]
pub struct Config {
    #[default("")]
    image_sender_cam1: &'static str,
    #[default("")]
    image_sender_cam2: &'static str,
    #[default("")]
    image_sender_cam3: &'static str,
    #[default("")]
    image_sender_cam4: &'static str,
    #[default("")]
    image_sender_cam5: &'static str,
    #[default("")]
    image_sender_cam6: &'static str,
}

/// 設定から解析されたカメラ情報を格納する構造体
#[derive(Debug, Clone)]
pub struct CameraConfig {
    pub name: String,
    pub mac_address: MacAddress,
}

/// 設定ファイルからカメラ設定を読み込む
pub fn load_camera_configs() -> Vec<CameraConfig> {
    let config = CONFIG;
    let mut cameras = Vec::new();

    // 詳細なログ出力を追加（設定読み込みの診断用）
    info!("Loading camera configurations from cfg.toml...");
    info!(
        "Raw config - cam1: '{}', cam2: '{}', cam3: '{}', cam4: '{}'",
        if config.image_sender_cam1.is_empty() {
            "<empty>"
        } else {
            config.image_sender_cam1
        },
        if config.image_sender_cam2.is_empty() {
            "<empty>"
        } else {
            config.image_sender_cam2
        },
        if config.image_sender_cam3.is_empty() {
            "<empty>"
        } else {
            config.image_sender_cam3
        },
        if config.image_sender_cam4.is_empty() {
            "<empty>"
        } else {
            config.image_sender_cam4
        }
    );

    // カメラ1の設定を確認
    if !config.image_sender_cam1.is_empty() {
        info!("Processing camera 1 config: {}", config.image_sender_cam1);
        add_camera_if_valid(&mut cameras, "cam1", config.image_sender_cam1);
    }

    // カメラ2の設定を確認
    if !config.image_sender_cam2.is_empty() {
        info!("Processing camera 2 config: {}", config.image_sender_cam2);
        add_camera_if_valid(&mut cameras, "cam2", config.image_sender_cam2);
    }

    // カメラ3の設定を確認
    if !config.image_sender_cam3.is_empty() {
        info!("Processing camera 3 config: {}", config.image_sender_cam3);
        add_camera_if_valid(&mut cameras, "cam3", config.image_sender_cam3);
    }

    // カメラ4の設定を確認
    if !config.image_sender_cam4.is_empty() {
        info!("Processing camera 4 config: {}", config.image_sender_cam4);
        add_camera_if_valid(&mut cameras, "cam4", config.image_sender_cam4);
    }

    // カメラ5の設定を確認
    if !config.image_sender_cam5.is_empty() {
        info!("Processing camera 5 config: {}", config.image_sender_cam5);
        add_camera_if_valid(&mut cameras, "cam5", config.image_sender_cam5);
    }

    // カメラ6の設定を確認
    if !config.image_sender_cam6.is_empty() {
        info!("Processing camera 6 config: {}", config.image_sender_cam6);
        add_camera_if_valid(&mut cameras, "cam6", config.image_sender_cam6);
    }

    // 設定の結果を報告
    if cameras.is_empty() {
        warn!("No valid camera MAC addresses configured in cfg.toml.");
    } else {
        info!("Found {} valid camera configs in cfg.toml", cameras.len());
        for camera in &cameras {
            info!("Camera {} with MAC: {}", camera.name, camera.mac_address);
        }
    }

    cameras
}

/// MACアドレスが有効であればカメラ設定を追加する
fn add_camera_if_valid(cameras: &mut Vec<CameraConfig>, name: &str, mac_str: &str) {
    match MacAddress::from_str(mac_str) {
        Ok(mac_address) => {
            cameras.push(CameraConfig {
                name: name.to_string(),
                mac_address,
            });
        }
        Err(e) => {
            warn!("Invalid MAC address for camera {}: {}", name, e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_camera_if_valid() {
        let mut cameras = Vec::new();

        // 有効なMACアドレス
        add_camera_if_valid(&mut cameras, "test1", "12:34:56:78:9a:bc");
        assert_eq!(cameras.len(), 1);
        assert_eq!(cameras[0].name, "test1");

        // 無効なMACアドレス
        add_camera_if_valid(&mut cameras, "test2", "invalid");
        // 無効なものは追加されないので数は変わらない
        assert_eq!(cameras.len(), 1);
    }

    // 注: load_camera_configs()のテストは実際のcfg.tomlファイルに依存するため、
    // モックを使用するかテスト環境専用のcfg.tomlを用意する必要がある
    // 統合テスト環境で実施することが望ましい
}
