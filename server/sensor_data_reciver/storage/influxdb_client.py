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
        self._active_tasks = set()  # アクティブタスクの追跡
        
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
        if config.IS_TEST_ENV:
            logger.info(f"Test environment detected, skipping InfluxDB write for {sender_mac}")
            return False
            
        # InfluxDBクライアントが初期化されていない場合はスキップ
        if not self.client or not self.write_api:
            logger.warning(f"InfluxDB client not initialized, skipping write for {sender_mac}")
            return False
            
        # InfluxDBへの書き込みを非同期で実行し、エラーが発生しても処理を継続する
        # asyncio.gatherを使用した構造化タスク管理
        write_task = self._write_sensor_data_async(sender_mac, voltage, temperature)
        cleanup_task = self._cleanup_completed_tasks()
        
        # 両方のタスクを同時実行し、例外を適切に処理
        async def execute_tasks():
            return await asyncio.gather(write_task, cleanup_task, return_exceptions=True)
        
        main_task = asyncio.create_task(execute_tasks())
        
        # 作成したタスクをアクティブタスクとして追跡
        self._active_tasks.add(main_task)
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
        """バックグラウンドで完了したタスクをクリーンアップ"""
        try:
            # 完了したタスクを特定してセットから削除
            completed_tasks = {task for task in self._active_tasks if task.done()}
            for task in completed_tasks:
                self._active_tasks.discard(task)
                # 例外が発生したタスクをログに記録
                if task.exception():
                    logger.warning(f"Task completed with exception: {task.exception()}")
            
            if completed_tasks:
                logger.debug(f"Cleaned up {len(completed_tasks)} completed tasks")
            
            # 他のタスクに実行権を譲る
            await asyncio.sleep(0.01)
        except Exception as e:
            logger.error(f"Error during task cleanup: {e}")
        
        logger.debug("Background cleanup completed")
    
    async def close(self):
        """リソースのクリーンアップ - 全てのアクティブタスクを待機"""
        try:
            # 全てのアクティブタスクが完了するまで待機
            if self._active_tasks:
                logger.info(f"Waiting for {len(self._active_tasks)} active tasks to complete...")
                await asyncio.gather(*self._active_tasks, return_exceptions=True)
                self._active_tasks.clear()
                logger.info("All active tasks completed")
            
            # InfluxDBクライアントのクリーンアップ
            if hasattr(self, 'write_api') and self.write_api:
                self.write_api.close()
            if hasattr(self, 'client') and self.client:
                self.client.close()
        except Exception as e:
            logger.error(f"Error during InfluxDB client cleanup: {e}")

    def close_sync(self):
        """同期的なクリーンアップメソッド（後方互換性のため）"""
        try:
            if hasattr(self, 'write_api') and self.write_api:
                self.write_api.close()
            if hasattr(self, 'client') and self.client:
                self.client.close()
        except Exception as e:
            logger.error(f"Error during InfluxDB client cleanup: {e}")


# グローバルInfluxDBクライアントインスタンス
influx_client = InfluxDBClient()
