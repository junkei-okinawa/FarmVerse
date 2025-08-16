"""Application configuration settings."""

import os
from dataclasses import dataclass
from dotenv import load_dotenv

load_dotenv()


@dataclass
class Config:
    """アプリケーション設定"""
    # Serial communication settings
    SERIAL_PORT: str = "/dev/ttyACM0"
    BAUD_RATE: int = 115200
    
    # Image processing settings
    IMAGE_DIR: str = "images"
    IMAGE_TIMEOUT: float = 20.0
    MAX_BUFFER_SIZE: int = 10 * 1024 * 1024  # 10MB
    MAX_DATA_LEN: int = 512
    
    # InfluxDB settings
    INFLUXDB_URL: str = os.environ.get("INFLUXDB_URL", "http://localhost:8086")
    INFLUXDB_ORG: str = "agri"
    INFLUXDB_BUCKET: str = "balcony"
    INFLUXDB_TOKEN: str = os.environ.get("INFLUXDB_TOKEN", "")
    INFLUXDB_TIMEOUT_SECONDS: int = 3
    
    # Test environment detection
    IS_TEST_ENV: bool = os.environ.get("PYTEST_CURRENT_TEST") is not None
    
    # Debug settings
    DEBUG_FRAME_PARSING: bool = os.environ.get("DEBUG_FRAME_PARSING", "true").lower() == "true"
    LOG_LEVEL: str = os.environ.get("LOG_LEVEL", "DEBUG")  # DEBUG, INFO, WARNING, ERROR, CRITICAL
    SUPPRESS_SYNC_ERRORS: bool = os.environ.get("SUPPRESS_SYNC_ERRORS", "false").lower() == "true"
    
    # Sleep duration configuration
    DEFAULT_SLEEP_DURATION_S: int = 60  # Default sleep duration for ESP32-CAM in seconds
    
    # voltage-based sleep duration configuration
    LOW_VOLTAGE_THRESHOLD_PERCENT: int = 8  # Same as device-side threshold
    LONG_SLEEP_DURATION_S: int = 3600 * 9  # 9 hours for low voltage (12:00以降)
    MEDIUM_SLEEP_DURATION_S: int = 3600  # 1 hour for low voltage (12:00未満)
    NORMAL_SLEEP_DURATION_S: int = 600  # 10 minutes for normal voltage


# Global configuration instance
config = Config()
