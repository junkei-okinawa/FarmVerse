use crate::esp_now::frame::{create_frame, detect_frame_type};
use crate::esp_now::FrameType;
use crate::mac_address::format_mac_address;
use crate::queue::ReceivedData;
use esp_idf_svc::sys::{esp_now_recv_info_t, ESP_NOW_ETH_ALEN};
use log::{debug, error, warn};
use std::collections::HashMap;
use std::slice;
use std::sync::Mutex;

/// ESP-NOW送信元ごとのシーケンス番号を管理するグローバル変数
static SEQUENCE_COUNTERS: Mutex<Option<HashMap<[u8; 6], u32>>> = Mutex::new(None);

/// 送信元MACアドレスごとにシーケンス番号を管理するためのヘルパー関数
fn get_sequence_number(mac_address: [u8; 6], reset: bool) -> u32 {
    // シーケンスカウンターを取得または初期化
    let mut counters = SEQUENCE_COUNTERS.lock().unwrap();

    // 初めて使用される場合はHashMapを初期化
    if counters.is_none() {
        *counters = Some(HashMap::new());
    }

    if let Some(ref mut counter_map) = *counters {
        // リセットフラグが立っている場合、シーケンス番号をリセット
        if reset {
            counter_map.insert(mac_address, 0);
            0
        } else {
            // 既存のカウンターを取得するか、新しいカウンターを作成
            let counter = counter_map.entry(mac_address).or_insert(0);
            *counter = counter.wrapping_add(1); // オーバーフロー対策
            *counter
        }
    } else {
        0 // 万が一ロックが取得できない場合のフォールバック
    }
}

/// ESP-NOWのコールバックから受信データをキューに入れる処理
///
/// # 安全性
///
/// この関数はESP-NOWのCコールバックから呼び出されるため、
/// 非同期コンテキストで実行されます。堅牢なエラーハンドリングが必要です。
///
/// # 引数
///
/// * `producer` - データ生成者キュー
/// * `info` - ESP-NOW受信情報構造体
/// * `data` - 受信データポインタ
/// * `data_len` - データ長
pub fn process_esp_now_data<P>(
    producer: &mut P,
    info: *const esp_now_recv_info_t,
    data: *const u8,
    data_len: i32,
) -> bool
where
    P: FnMut(ReceivedData) -> bool,
{
    // 引数の検証
    if info.is_null() || (data.is_null() && data_len > 0) || data_len < 0 {
        error!("ESP-NOW CB: Invalid arguments received.");
        return false;
    }

    // 送信元MACアドレスの取得
    let src_mac_ptr = unsafe { (*info).src_addr };
    if src_mac_ptr.is_null() {
        error!("ESP-NOW CB: Source MAC address pointer is null.");
        return false;
    }

    // MACアドレスをバイト配列に変換
    let mac_array: [u8; ESP_NOW_ETH_ALEN as usize] = unsafe {
        match slice::from_raw_parts(src_mac_ptr, ESP_NOW_ETH_ALEN as usize).try_into() {
            Ok(arr) => arr,
            Err(_) => {
                error!("ESP-NOW CB: Failed to convert MAC address slice to array.");
                return false;
            }
        }
    };

    // ログ用MACアドレス文字列を作成
    let mac_str = format_mac_address(&mac_array);

    // データスライスの取得
    let data_slice = unsafe { slice::from_raw_parts(data, data_len as usize) };

    // フレームタイプの検出
    let frame_type = detect_frame_type(data_slice);
    let is_eof = frame_type == FrameType::Eof;
    let is_hash = frame_type == FrameType::Hash;

    // 特殊フレーム（EOFやHASH）のログ出力
    if is_eof {
        warn!("ESP-NOW CB [{}]: Received EOF marker (b\"EOF!\").", mac_str);
    } else if is_hash {
        warn!("ESP-NOW CB [{}]: Received HASH marker.", mac_str);
    }

    // シーケンス番号の取得（EOFまたはHASHでリセット）
    let seq_num = get_sequence_number(mac_array, is_eof || is_hash);

    // データをフレーム化
    let framed_data = create_frame(mac_array, data_slice, frame_type, seq_num);

    // デバッグログ
    debug!(
        "ESP-NOW CB [{}]: Received chunk ({} bytes, type={}, seq={}). Framed: {} bytes.",
        mac_str,
        data_len,
        frame_type.as_str(),
        seq_num,
        framed_data.len()
    );

    // フレーム化されたデータをキューに追加
    let received_data = ReceivedData {
        mac: mac_array,
        data: framed_data,
    };

    // 生産者関数を呼び出して、キューへの追加を試みる
    let success = producer(received_data);

    if !success {
        // キューへの追加が失敗した場合
        warn!(
            "ESP-NOW CB [{}]: Data queue full! Dropping {} frame (seq={}).",
            mac_str,
            frame_type.as_str(),
            seq_num
        );

        // EOFフレームが落とされた場合は特に重要なので強調
        if is_eof {
            error!(
                "ESP-NOW CB [{}]: CRITICAL! EOF frame dropped due to queue full!",
                mac_str
            );
        }
    }

    success
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn test_get_sequence_number() {
        // テスト前にカウンターをリセット
        {
            let mut counters = SEQUENCE_COUNTERS.lock().unwrap();
            *counters = Some(HashMap::new());
        }

        let mac1 = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66];
        let mac2 = [0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff];

        // 初期値は0
        assert_eq!(get_sequence_number(mac1, false), 1);
        assert_eq!(get_sequence_number(mac1, false), 2);

        // 別のMACアドレスは独立したカウンター
        assert_eq!(get_sequence_number(mac2, false), 1);

        // リセット後は0に戻る
        assert_eq!(get_sequence_number(mac1, true), 0);
        assert_eq!(get_sequence_number(mac1, false), 1);
    }

    #[test]
    fn test_process_esp_now_data() {
        // mock_info と mock_data は実際のテストでは使わない
        let mock_info: *const esp_now_recv_info_t = std::ptr::null();
        let mock_data: *const u8 = std::ptr::null();

        // テスト用の受信データ保存変数
        let received = RefCell::new(None);

        // 成功ケース用の生産者関数
        let mut success_producer = |data: ReceivedData| {
            *received.borrow_mut() = Some(data);
            true
        };

        // 失敗ケース用の生産者関数
        let _fail_producer = |_: ReceivedData| false;

        // null引数のエラーケース
        assert_eq!(
            process_esp_now_data(&mut success_producer, mock_info, mock_data, 10),
            false
        );

        // 成功と失敗のケースは、実際のESP-NOWハードウェアが必要なため、
        // 統合テスト環境またはモックを使って別途テストすることが望ましい
    }
}
