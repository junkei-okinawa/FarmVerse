import os
import sys

import pytest

# テストファイルから見た app.py への正しいパス
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', '..'))

from app import (CHECKSUM_LENGTH, END_MARKER, FRAME_TYPE_DATA, FRAME_TYPE_EOF,
                 FRAME_TYPE_HASH, FRAME_TYPE_LENGTH, LENGTH_FIELD_BYTES,
                 MAC_ADDRESS_LENGTH, SEQUENCE_NUM_LENGTH, START_MARKER,
                 FrameParser)


def test_parse_header_valid():
    mac_bytes = b"\x01\x02\x03\x04\x05\x06"
    sender_mac = "01:02:03:04:05:06"
    frame_type = FRAME_TYPE_DATA
    seq_num = 1234
    data_len = 500

    header_bytes = (
        START_MARKER +
        mac_bytes +
        bytes([frame_type]) +
        seq_num.to_bytes(SEQUENCE_NUM_LENGTH, byteorder="little") +
        data_len.to_bytes(LENGTH_FIELD_BYTES, byteorder="little")
    )

    parsed_mac, parsed_type, parsed_seq, parsed_len = FrameParser.parse_header(header_bytes, 0)

    assert parsed_mac == sender_mac
    assert parsed_type == frame_type
    assert parsed_seq == seq_num
    assert parsed_len == data_len

def test_parse_header_invalid_mac_length():
    mac_bytes = b"\x01\x02\x03\x04\x05"  # Too short
    data_len = 500

    # parse_headerではなく、validate_frame_dataでエラーが発生するはず
    with pytest.raises(ValueError):
        FrameParser.validate_frame_data(data_len, mac_bytes)

def test_validate_frame_data_valid():
    mac_bytes = b"\x01\x02\x03\x04\x05\x06"
    data_len = 500
    assert FrameParser.validate_frame_data(data_len, mac_bytes) is True

def test_validate_frame_data_too_long_data():
    mac_bytes = b"\x01\x02\x03\x04\x05\x06"
    data_len = 1000 # Assuming MAX_DATA_LEN is 512
    with pytest.raises(ValueError):
        FrameParser.validate_frame_data(data_len, mac_bytes)

def test_validate_frame_data_invalid_mac_length():
    mac_bytes = b"\x01\x02\x03\x04\x05"
    data_len = 500
    with pytest.raises(ValueError):
        FrameParser.validate_frame_data(data_len, mac_bytes)

def test_sanitize_filename_basic():
    mac_str = "01:02:03:04:05:06"
    timestamp = "20231027_103000_123456"
    expected_filename = "010203040506_20231027_103000_123456.jpg"
    assert FrameParser.sanitize_filename(mac_str, timestamp) == expected_filename

def test_sanitize_filename_special_chars():
    mac_str = "01:02-03:04-05:06"
    timestamp = "2023/10/27_10:30:00_123456"
    expected_filename = "0102-0304-0506_20231027_103000_123456.jpg"
    assert FrameParser.sanitize_filename(mac_str, timestamp) == expected_filename
