"""Voltage and temperature data processing."""

import sys
import os
from typing import Optional

sys.path.append(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from utils.data_parser import DataParser


class VoltageDataProcessor:
    """電圧データ処理クラス
    
    Note: このクラスは後方互換性のために維持されていますが、
    新しいコードでは utils.data_parser.DataParser を直接使用することを推奨します。
    """
    
    @staticmethod
    def parse_voltage_data(payload: str) -> Optional[float]:
        """電圧データの解析（後方互換性メソッド）"""
        return DataParser.parse_voltage_data(payload)
    
    @staticmethod
    def parse_temperature_data(payload: str) -> Optional[float]:
        """温度データの解析（後方互換性メソッド）"""
        return DataParser.parse_temperature_data(payload)

    @staticmethod
    def parse_tds_voltage_data(payload: str) -> Optional[float]:
        """TDS電圧データの解析（後方互換性メソッド）"""
        return DataParser.parse_tds_voltage_data(payload)
