"""InfluxDB client for sensor data storage."""

import asyncio
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
        self.client = None
        self.write_api = None
        self._tasks = []  # 実行中の非同期タスクを追跡
        
        try:
            self.client = influxdb_client.InfluxDBClient(
                url=config.INFLUXDB_URL, 
                token=self.token, 
                org=config.INFLUXDB_ORG
            )
            self.write_api = self.client.write_api(write_options=SYNCHRONOUS)
            
            # 接続テスト
            try:
                # シンプルな ping クエリで接続を確認
                health = self.client.health()
                if health.status == "pass":
                    logger.info(f"InfluxDB client initialized successfully: {config.INFLUXDB_URL}")
                else:
                    logger.warning(f"InfluxDB health check failed: {health.status}")
                    self._disable_client()
            except Exception as e:
                logger.warning(f"InfluxDB connection test failed: {e} - InfluxDB writes will be disabled")
                self._disable_client()
                
        except Exception as e:
            logger.error(f"Failed to initialize InfluxDB client: {e} - InfluxDB writes will be disabled")
            self._disable_client()
    
    def _disable_client(self):
        """InfluxDBクライアントを無効化"""
        try:
            if self.write_api:
                self.write_api.close()
            if self.client:
                self.client.close()
        except:
            pass
        self.client = None
        self.write_api = None
    
    def write_sensor_data(self, sender_mac: str, voltage: float = None, temperature: float = None) -> bool:
        """センサーデータをInfluxDBに書き込み（非同期実行・エラー耐性付き）"""
        # テスト環境ではInfluxDB書き込みをスキップ
        if os.environ.get('PYTEST_CURRENT_TEST'):
            logger.info(f"Test environment detected, skipping InfluxDB write for {sender_mac}")
            return True
            
        # InfluxDBクライアントが初期化されていない場合はスキップ
        if not self.client or not self.write_api:
            logger.warning(f"InfluxDB client not initialized, skipping write for {sender_mac}")
            return False
            
        # InfluxDBへの書き込みを非同期で実行し、エラーが発生しても処理を継続する
        task = asyncio.create_task(self._write_sensor_data_async(sender_mac, voltage, temperature))
        self._tasks.append(task)
        
        # 完了したタスクのクリーンアップを非同期で実行
        asyncio.create_task(self._cleanup_completed_tasks())
        return True  # 非同期実行のため、即座にTrueを返す
    
    async def _write_sensor_data_async(self, sender_mac: str, voltage: float = None, temperature: float = None):
        """非同期でInfluxDBにデータを書き込み"""
        try:
            # クライアントが初期化されていない場合はスキップ
            if not self.client or not self.write_api:
                logger.warning(f"InfluxDB client not available for {sender_mac}")
                return
                
            point = Point("data").tag("mac_address", sender_mac)
            
            if voltage is not None:
                point.field("voltage", float(voltage))
            
            if temperature is not None:
                point.field("temperature", float(temperature))
            
            if voltage is not None or temperature is not None:
                logger.info(f"Writing data to InfluxDB for {sender_mac}: voltage={voltage}, temperature={temperature}")
                # タイムアウトを設定して書き込み実行
                await asyncio.wait_for(
                    asyncio.to_thread(
                        self.write_api.write, 
                        bucket=config.INFLUXDB_BUCKET, 
                        org=config.INFLUXDB_ORG, 
                        record=point
                    ),
                    timeout=3.0  # 3秒でタイムアウト（10→3秒に短縮）
                )
                logger.info(f"Successfully wrote data to InfluxDB for {sender_mac}")
            else:
                logger.warning(f"No valid data to write for {sender_mac}")
                
        except asyncio.TimeoutError:
            logger.error(f"Timeout writing to InfluxDB for {sender_mac} (continuing with other operations)")
        except ConnectionError as e:
            logger.error(f"Connection error writing to InfluxDB for {sender_mac}: {e} (continuing with other operations)")
        except Exception as e:
            logger.error(f"Unexpected error writing to InfluxDB for {sender_mac}: {e}")
            
    async def _cleanup_completed_tasks(self):
        """完了したタスクをクリーンアップし、例外をログに記録"""
        if not self._tasks:
            return
            
        # 完了したタスクを特定
        completed_tasks = [task for task in self._tasks if task.done()]
        
        # 完了したタスクの例外をチェック
        for task in completed_tasks:
            try:
                # タスクの結果を取得（例外があれば発生）
                await task
            except Exception as e:
                logger.error(f"Exception in InfluxDB write task: {e}")
                
        # 完了したタスクを削除
        self._tasks = [task for task in self._tasks if not task.done()]
        
        # タスクリストが大きくなりすぎないよう制限
        if len(self._tasks) > 100:
            logger.warning(f"Too many pending InfluxDB tasks: {len(self._tasks)}")
            # 古いタスクを一部キャンセル
            for task in self._tasks[:50]:
                if not task.done():
                    task.cancel()
            self._tasks = self._tasks[50:]
    
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
