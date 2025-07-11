"""Frame parsing utilities."""

import re
from typing import Tuple

from .constants import START_MARKER, MAC_ADDRESS_LENGTH, FRAME_TYPE_LENGTH, SEQUENCE_NUM_LENGTH, LENGTH_FIELD_BYTES


class FrameParser:
    """フレーム解析クラス"""
    
    @staticmethod
    def parse_header(buffer: bytearray, start_pos: int) -> Tuple[str, int, int, int]:
        """ヘッダー部分の解析を分離"""
        header_start = start_pos + len(START_MARKER)
        mac_bytes = buffer[header_start : header_start + MAC_ADDRESS_LENGTH]
        sender_mac = ":".join(f"{b:02x}" for b in mac_bytes)
        
        frame_type_pos = header_start + MAC_ADDRESS_LENGTH
        frame_type = buffer[frame_type_pos]
        
        seq_num_pos = frame_type_pos + FRAME_TYPE_LENGTH
        seq_bytes = buffer[seq_num_pos:seq_num_pos + SEQUENCE_NUM_LENGTH]
        seq_num = int.from_bytes(seq_bytes, byteorder="big")
        
        data_len_pos = seq_num_pos + SEQUENCE_NUM_LENGTH
        len_bytes = buffer[data_len_pos:data_len_pos + LENGTH_FIELD_BYTES]
        data_len = int.from_bytes(len_bytes, byteorder="big")
        
        return sender_mac, frame_type, seq_num, data_len

    @staticmethod
    def validate_frame_data(data_len: int, mac_bytes: bytes, max_data_len: int = 512) -> bool:
        """フレームデータの検証"""
        if data_len > max_data_len:
            raise ValueError(f"Data length {data_len} exceeds maximum {max_data_len}")
        
        if len(mac_bytes) != MAC_ADDRESS_LENGTH:
            raise ValueError(f"Invalid MAC address length: {len(mac_bytes)}")
        
        return True

    @staticmethod
    def sanitize_filename(sender_mac_str: str, timestamp: str) -> str:
        """ファイル名のサニタイズ"""
        safe_mac = re.sub(r'[^\w\-_]', '', sender_mac_str.replace(':', ''))
        safe_timestamp = re.sub(r'[^\w\-_]', '', timestamp)
        return f"{safe_mac}_{safe_timestamp}.jpg"
