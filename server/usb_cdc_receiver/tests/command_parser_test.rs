// Command Parser Unit Tests
// これらのテストはホストマシンで実行されます

use usb_cdc_receiver::command::{parse_command, Command};

#[test]
fn test_valid_esp_now_command() {
    let command = "CMD_SEND_ESP_NOW:34:ab:95:fb:3f:c4:60";
    let result = parse_command(command).unwrap();
    
    match result {
        Command::SendEspNow { mac_address, sleep_seconds } => {
            assert_eq!(mac_address, "34:ab:95:fb:3f:c4");
            assert_eq!(sleep_seconds, 60);
        }
        _ => panic!("Expected SendEspNow command"),
    }
}

#[test]
fn test_valid_esp_now_command_with_uppercase_mac() {
    let command = "CMD_SEND_ESP_NOW:AA:BB:CC:DD:EE:FF:120";
    let result = parse_command(command).unwrap();
    
    match result {
        Command::SendEspNow { mac_address, sleep_seconds } => {
            assert_eq!(mac_address, "AA:BB:CC:DD:EE:FF");
            assert_eq!(sleep_seconds, 120);
        }
        _ => panic!("Expected SendEspNow command"),
    }
}

#[test]
fn test_valid_esp_now_command_with_mixed_case_mac() {
    let command = "CMD_SEND_ESP_NOW:aA:Bb:Cc:Dd:Ee:Ff:300";
    let result = parse_command(command).unwrap();
    
    match result {
        Command::SendEspNow { mac_address, sleep_seconds } => {
            assert_eq!(mac_address, "aA:Bb:Cc:Dd:Ee:Ff");
            assert_eq!(sleep_seconds, 300);
        }
        _ => panic!("Expected SendEspNow command"),
    }
}

#[test]
fn test_esp_now_command_with_whitespace() {
    let command = "  CMD_SEND_ESP_NOW:34:ab:95:fb:3f:c4:60  ";
    let result = parse_command(command).unwrap();
    
    match result {
        Command::SendEspNow { mac_address, sleep_seconds } => {
            assert_eq!(mac_address, "34:ab:95:fb:3f:c4");
            assert_eq!(sleep_seconds, 60);
        }
        _ => panic!("Expected SendEspNow command"),
    }
}

#[test]
fn test_invalid_mac_address_too_few_parts() {
    let command = "CMD_SEND_ESP_NOW:34:ab:95:fb:3f:60"; // 6パーツのMAC必要だが5つしかない
    let result = parse_command(command);
    assert!(result.is_err());
}

#[test]
fn test_invalid_mac_address_too_many_parts() {
    let command = "CMD_SEND_ESP_NOW:34:ab:95:fb:3f:c4:d5:60"; // 6パーツ必要だが7つある
    let result = parse_command(command);
    assert!(result.is_err());
}

#[test]
fn test_invalid_mac_address_invalid_hex() {
    let command = "CMD_SEND_ESP_NOW:34:ab:95:fb:3f:ZZ:60"; // ZZは無効な16進数
    let result = parse_command(command);
    assert!(result.is_err());
}

#[test]
fn test_invalid_mac_address_wrong_length() {
    let command = "CMD_SEND_ESP_NOW:3:ab:95:fb:3f:c4:60"; // 1桁のみ
    let result = parse_command(command);
    assert!(result.is_err());
}

#[test]
fn test_invalid_sleep_time_zero() {
    let command = "CMD_SEND_ESP_NOW:34:ab:95:fb:3f:c4:0"; // スリープ時間が0
    let result = parse_command(command);
    assert!(result.is_err());
}

#[test]
fn test_invalid_sleep_time_negative() {
    let command = "CMD_SEND_ESP_NOW:34:ab:95:fb:3f:c4:-10";
    let result = parse_command(command);
    assert!(result.is_err()); // u32のパースエラーになる
}

#[test]
fn test_invalid_sleep_time_too_large() {
    let command = "CMD_SEND_ESP_NOW:34:ab:95:fb:3f:c4:86401"; // 24時間+1秒
    let result = parse_command(command);
    assert!(result.is_err());
}

#[test]
fn test_invalid_sleep_time_not_a_number() {
    let command = "CMD_SEND_ESP_NOW:34:ab:95:fb:3f:c4:abc";
    let result = parse_command(command);
    assert!(result.is_err());
}

#[test]
fn test_max_valid_sleep_time() {
    let command = "CMD_SEND_ESP_NOW:34:ab:95:fb:3f:c4:86400"; // 24時間ちょうど
    let result = parse_command(command).unwrap();
    
    match result {
        Command::SendEspNow { sleep_seconds, .. } => {
            assert_eq!(sleep_seconds, 86400);
        }
        _ => panic!("Expected SendEspNow command"),
    }
}

#[test]
fn test_min_valid_sleep_time() {
    let command = "CMD_SEND_ESP_NOW:34:ab:95:fb:3f:c4:1"; // 1秒
    let result = parse_command(command).unwrap();
    
    match result {
        Command::SendEspNow { sleep_seconds, .. } => {
            assert_eq!(sleep_seconds, 1);
        }
        _ => panic!("Expected SendEspNow command"),
    }
}

#[test]
fn test_unknown_command() {
    let command = "UNKNOWN_COMMAND:param1:param2";
    let result = parse_command(command).unwrap();
    
    match result {
        Command::Unknown(cmd) => {
            assert_eq!(cmd, "UNKNOWN_COMMAND:param1:param2");
        }
        _ => panic!("Expected Unknown command"),
    }
}

#[test]
fn test_empty_command() {
    let command = "";
    let result = parse_command(command).unwrap();
    
    match result {
        Command::Unknown(cmd) => {
            assert_eq!(cmd, "");
        }
        _ => panic!("Expected Unknown command for empty string"),
    }
}

#[test]
fn test_whitespace_only_command() {
    let command = "   ";
    let result = parse_command(command).unwrap();
    
    match result {
        Command::Unknown(cmd) => {
            assert_eq!(cmd, "");
        }
        _ => panic!("Expected Unknown command for whitespace"),
    }
}

#[test]
fn test_command_case_sensitivity() {
    // コマンドは大文字小文字を区別する
    let command = "cmd_send_esp_now:34:ab:95:fb:3f:c4:60";
    let result = parse_command(command).unwrap();
    
    match result {
        Command::Unknown(_) => {
            // 小文字なので不明なコマンドとして扱われる
        }
        _ => panic!("Expected Unknown command for lowercase"),
    }
}

#[test]
fn test_partial_command() {
    // コロンありの場合
    let command = "CMD_SEND_ESP_NOW:";
    let result = parse_command(command);
    assert!(result.is_err(), "Partial command with colon should fail"); // パーツ不足でエラー
    
    // コロンなしの場合
    let command_no_colon = "CMD_SEND_ESP_NOW";
    let result_no_colon = parse_command(command_no_colon).unwrap();
    match result_no_colon {
        Command::Unknown(_) => {}, // Unknownコマンドとして扱われる
        _ => panic!("Expected Unknown command for command without colon"),
    }
}

#[test]
fn test_command_with_extra_colons() {
    let command = "CMD_SEND_ESP_NOW:34:ab:95:fb:3f:c4:60:extra";
    let result = parse_command(command);
    assert!(result.is_err()); // パーツ過多
}
