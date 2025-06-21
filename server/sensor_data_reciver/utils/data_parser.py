"""Shared data parsing utilities to avoid duplication across modules."""

import logging
from typing import Optional

logger = logging.getLogger(__name__)


class DataParser:
    """共通データ解析ユーティリティクラス"""
    
    @staticmethod
    def extract_value_from_payload(payload: str, prefix: str) -> Optional[str]:
        """
        ペイロードから指定されたプレフィックスの値を抽出
        
        Args:
            payload: 解析対象のペイロード文字列
            prefix: 検索するプレフィックス (例: "VOLT:", "TEMP:")
            
        Returns:
            プレフィックス後の値文字列、見つからない場合はNone
        """
        try:
            if prefix in payload:
                # コンマ区切りで分割して該当部分を探す
                payload_split = payload.split(",")
                for part in payload_split:
                    if part.startswith(prefix):
                        return part.split(":", 1)[1]  # プレフィックス後の値を取得
                        
                # コンマ区切りで見つからない場合は直接置換を試す
                if payload.startswith(prefix):
                    return payload.replace(prefix, "")
            return None
        except (ValueError, IndexError):
            return None
    
    @staticmethod
    def parse_voltage_data(payload: str) -> Optional[float]:
        """
        電圧データの解析
        
        Args:
            payload: 解析対象のペイロード文字列
            
        Returns:
            電圧値（float）、解析できない場合はNone
        """
        try:
            volt_str = DataParser.extract_value_from_payload(payload, "VOLT:")
            if volt_str is not None:
                return float(volt_str)
            return None
        except (ValueError, TypeError):
            return None
    
    @staticmethod
    def parse_temperature_data(payload: str) -> Optional[float]:
        """
        温度データの解析
        
        Args:
            payload: 解析対象のペイロード文字列
            
        Returns:
            温度値（float）、解析できない場合はNone
        """
        try:
            temp_str = DataParser.extract_value_from_payload(payload, "TEMP:")
            if temp_str is not None:
                return float(temp_str)
            return None
        except (ValueError, TypeError):
            return None
    
    @staticmethod
    def extract_voltage_with_validation(payload: str, sender_mac: str) -> Optional[float]:
        """
        電圧情報を抽出（バリデーション付き）
        
        Args:
            payload: 解析対象のペイロード文字列
            sender_mac: 送信元MACアドレス（ログ用）
            
        Returns:
            電圧値（float）、無効な場合はNone
        """
        volt_str = DataParser.extract_value_from_payload(payload, "VOLT:")
        if volt_str is not None:
            if volt_str != "100":  # 100%の時は初回起動またはデバッグ時のため記録しない
                try:
                    return float(volt_str)
                except ValueError:
                    logger.warning(f"Invalid VOLT value from {sender_mac}: {volt_str}")
            return None  # 100%の場合は記録しない
        else:
            logger.warning(f"VOLT not found in HASH payload from {sender_mac}")
        return None
    
    @staticmethod
    def extract_temperature_with_validation(payload: str, sender_mac: str) -> Optional[float]:
        """
        温度情報を抽出（バリデーション付き）
        
        Args:
            payload: 解析対象のペイロード文字列
            sender_mac: 送信元MACアドレス（ログ用）
            
        Returns:
            温度値（float）、無効な場合はNone
        """
        temp_str = DataParser.extract_value_from_payload(payload, "TEMP:")
        if temp_str is not None:
            if "-999" not in temp_str:
                try:
                    return float(temp_str)
                except ValueError:
                    logger.warning(f"Invalid TEMP value from {sender_mac}: {temp_str}")
            return None  # -999の場合は無効値
        elif payload:  # 空文字列でない場合のみ警告
            logger.warning(f"TEMP not found in HASH payload from {sender_mac}")
        return None
