"""Voltage and temperature data processing."""

from typing import Optional


class VoltageDataProcessor:
    """電圧データ処理クラス"""
    
    @staticmethod
    def parse_voltage_data(payload: str) -> Optional[float]:
        """電圧データの解析"""
        try:
            payload_split = payload.split(",")
            for part in payload_split:
                if part.startswith("VOLT:"):
                    volt_value = part.split(":")[1]
                    return float(volt_value)
            return None
        except (ValueError, IndexError):
            return None
    
    @staticmethod
    def parse_temperature_data(payload: str) -> Optional[float]:
        """温度データの解析"""
        try:
            payload_split = payload.split(",")
            for part in payload_split:
                if part.startswith("TEMP:"):
                    temp_value = part.split(":")[1]
                    return float(temp_value)
            return None
        except (ValueError, IndexError):
            return None
