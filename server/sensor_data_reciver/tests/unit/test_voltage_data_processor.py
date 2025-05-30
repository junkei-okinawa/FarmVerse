import os
import sys

import pytest

# テストファイルから見た app.py への正しいパス
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', '..'))

from app import VoltageDataProcessor


def test_parse_voltage_data_valid():
    payload = "TEMP:25.5,VOLT:12.3,OTHER:data"
    voltage = VoltageDataProcessor.parse_voltage_data(payload)
    assert voltage == 12.3

def test_parse_voltage_data_not_found():
    payload = "TEMP:25.5,OTHER:data"
    voltage = VoltageDataProcessor.parse_voltage_data(payload)
    assert voltage is None

def test_parse_voltage_data_invalid_format():
    payload = "TEMP:25.5,VOLT:abc,OTHER:data"
    voltage = VoltageDataProcessor.parse_voltage_data(payload)
    assert voltage is None

def test_parse_temperature_data_valid():
    payload = "TEMP:25.5,VOLT:12.3,OTHER:data"
    temperature = VoltageDataProcessor.parse_temperature_data(payload)
    assert temperature == 25.5

def test_parse_temperature_data_not_found():
    payload = "VOLT:12.3,OTHER:data"
    temperature = VoltageDataProcessor.parse_temperature_data(payload)
    assert temperature is None

def test_parse_temperature_data_invalid_format():
    payload = "TEMP:xyz,VOLT:12.3,OTHER:data"
    temperature = VoltageDataProcessor.parse_temperature_data(payload)
    assert temperature is None
