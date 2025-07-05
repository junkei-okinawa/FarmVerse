"""Sleep control logic module."""

import logging
from datetime import datetime
from typing import Optional

import sys
import os
sys.path.append(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from config import config

logger = logging.getLogger(__name__)


def format_sleep_command_to_gateway(sender_mac: str, sleep_duration_s: int) -> str:
    """Formats the sleep command string to be sent to the gateway."""
    return f"CMD_SEND_ESP_NOW:{sender_mac}:{sleep_duration_s}\n"


def determine_sleep_duration(voltage_percent: Optional[float]) -> int:
    """
    Determine sleep duration based on battery voltage percentage and current time.
    
    Args:
        voltage_percent: Battery voltage as percentage (0-100), or None if unknown
        
    Returns:
        Sleep duration in seconds
    """
    if voltage_percent is None:
        # Unknown voltage, use default
        return config.DEFAULT_SLEEP_DURATION_S
    
    if voltage_percent < config.LOW_VOLTAGE_THRESHOLD_PERCENT:
        # Low voltage: use time-based long sleep to conserve power
        current_time = datetime.now()
        current_hour = current_time.hour
        
        if current_hour >= 12:
            # 12:00以降（午後・夜間）: 9時間スリープ
            # 夜間は暗くなるためカメラ画像撮影は行わない想定
            logger.info(f"Low voltage ({voltage_percent}% < {config.LOW_VOLTAGE_THRESHOLD_PERCENT}%) + afternoon/night ({current_hour}:xx >= 12:00), using 9-hour sleep: {config.LONG_SLEEP_DURATION_S}s")
            return config.LONG_SLEEP_DURATION_S
        else:
            # 12:00未満（午前中）: 1時間スリープ
            # 夜明け前にロングスリープから覚めてしまった場合を想定
            logger.info(f"Low voltage ({voltage_percent}% < {config.LOW_VOLTAGE_THRESHOLD_PERCENT}%) + morning ({current_hour}:xx < 12:00), using 1-hour sleep: {config.MEDIUM_SLEEP_DURATION_S}s")
            return config.MEDIUM_SLEEP_DURATION_S
    else:
        # Normal battery: use normal sleep interval (10 minutes)
        logger.info(f"Normal voltage ({voltage_percent}% >= {config.LOW_VOLTAGE_THRESHOLD_PERCENT}%), using normal sleep: {config.NORMAL_SLEEP_DURATION_S}s")
        return config.NORMAL_SLEEP_DURATION_S
