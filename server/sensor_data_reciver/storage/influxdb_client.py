"""InfluxDB client for sensor data storage."""

import logging
import os

import influxdb_client
from influxdb_client import Point
from influxdb_client.client.write_api import SYNCHRONOUS

import sys
sys.path.append(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from config import config

logger = logging.getLogger(__name__)


class InfluxDBClient:
    """InfluxDB クライアント管理クラス"""
    
    def __init__(self):
        self.token = os.environ.get("INFLUXDB_TOKEN")
        self.client = influxdb_client.InfluxDBClient(
            url=config.INFLUXDB_URL, 
            token=self.token, 
            org=config.INFLUXDB_ORG
        )
        self.write_api = self.client.write_api(write_options=SYNCHRONOUS)
    
    def write_sensor_data(self, sender_mac: str, voltage: float = None, temperature: float = None) -> bool:
        """センサーデータをInfluxDBに書き込み"""
        try:
            point = Point("data").tag("mac_address", sender_mac)
            
            if voltage is not None:
                point.field("voltage", float(voltage))
            
            if temperature is not None:
                point.field("temperature", float(temperature))
            
            if voltage is not None or temperature is not None:
                logger.info(f"Writing data to InfluxDB for {sender_mac}: voltage={voltage}, temperature={temperature}")
                self.write_api.write(bucket=config.INFLUXDB_BUCKET, org=config.INFLUXDB_ORG, record=point)
                return True
            else:
                logger.warning(f"No valid data to write for {sender_mac}")
                return False
                
        except Exception as e:
            logger.error(f"Error writing to InfluxDB: {e}")
            return False
    
    def close(self):
        """リソースのクリーンアップ"""
        try:
            if hasattr(self, 'write_api') and self.write_api:
                self.write_api.close()
            if hasattr(self, 'client') and self.client:
                self.client.close()
        except Exception as e:
            logger.error(f"Error during InfluxDB client cleanup: {e}")


# グローバルInfluxDBクライアントインスタンス
influx_client = InfluxDBClient()
