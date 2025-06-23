"""Unit tests for InfluxDB client async task tracking"""

import asyncio
import pytest
from unittest.mock import AsyncMock, MagicMock, patch

import sys
import os
sys.path.append(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))

from storage.influxdb_client import InfluxDBClient


class TestInfluxDBClientAsyncTasks:
    """Test async task tracking in InfluxDB client"""
    
    @pytest.fixture
    def mock_config(self):
        """Mock configuration"""
        with patch('storage.influxdb_client.config') as mock_config:
            mock_config.INFLUXDB_URL = "http://localhost:8086"
            mock_config.INFLUXDB_TOKEN = "test-token"
            mock_config.INFLUXDB_ORG = "test-org"
            mock_config.INFLUXDB_BUCKET = "test-bucket"
            mock_config.INFLUXDB_TIMEOUT_SECONDS = 3
            mock_config.IS_TEST_ENV = False
            yield mock_config
    
    @pytest.fixture
    def mock_influxdb_client(self):
        """Mock InfluxDB client"""
        with patch('storage.influxdb_client.influxdb_client.InfluxDBClient') as mock_client:
            mock_instance = MagicMock()
            mock_write_api = MagicMock()
            mock_instance.write_api.return_value = mock_write_api
            mock_client.return_value = mock_instance
            yield mock_instance, mock_write_api
    
    def test_init_creates_active_tasks_set(self, mock_config, mock_influxdb_client):
        """Test that initialization creates the active tasks set"""
        client = InfluxDBClient()
        assert hasattr(client, '_active_tasks')
        assert isinstance(client._active_tasks, set)
        assert len(client._active_tasks) == 0
    
    @pytest.mark.asyncio
    async def test_write_sensor_data_adds_task_to_active_set(self, mock_config, mock_influxdb_client):
        """Test that write_sensor_data adds task to active tasks set"""
        mock_instance, mock_write_api = mock_influxdb_client
        
        # Mock health check to return success
        mock_instance.health.return_value.status = "pass"
        
        client = InfluxDBClient()
        
        # Mock the async write method to return immediately
        with patch.object(client, '_write_sensor_data_async', new_callable=AsyncMock) as mock_write_async, \
             patch.object(client, '_cleanup_completed_tasks', new_callable=AsyncMock) as mock_cleanup:
            
            # Call write_sensor_data
            result = client.write_sensor_data("aa:bb:cc:dd:ee:ff", 85.5, 22.3)
            
            # Should return True for successful initiation
            assert result is True
            
            # Give a moment for the task to be created
            await asyncio.sleep(0.01)
            
            # Check that a task was added to active tasks
            assert len(client._active_tasks) == 1
            
            # Wait for the task to complete
            await asyncio.gather(*client._active_tasks, return_exceptions=True)
            client._active_tasks.clear()
            
            # Verify the async methods were called
            mock_write_async.assert_called_once_with("aa:bb:cc:dd:ee:ff", 85.5, 22.3)
            mock_cleanup.assert_called_once()
    
    @pytest.mark.asyncio
    async def test_cleanup_completed_tasks_removes_done_tasks(self, mock_config, mock_influxdb_client):
        """Test that cleanup removes completed tasks from the active set"""
        client = InfluxDBClient()
        
        # Create a completed task
        async def dummy_task():
            return "completed"
        
        task = asyncio.create_task(dummy_task())
        client._active_tasks.add(task)
        
        # Wait for the task to complete
        await task
        
        # Call cleanup
        await client._cleanup_completed_tasks()
        
        # The completed task should be removed
        assert len(client._active_tasks) == 0
    
    @pytest.mark.asyncio
    async def test_close_waits_for_all_active_tasks(self, mock_config, mock_influxdb_client):
        """Test that close() waits for all active tasks to complete"""
        mock_instance, mock_write_api = mock_influxdb_client
        client = InfluxDBClient()
        
        # Create some long-running tasks
        task_completed = []
        
        async def long_task(task_id):
            await asyncio.sleep(0.1)
            task_completed.append(task_id)
            return f"task_{task_id}_completed"
        
        # Add multiple tasks to active set
        for i in range(3):
            task = asyncio.create_task(long_task(i))
            client._active_tasks.add(task)
        
        # Tasks should not be completed yet
        assert len(task_completed) == 0
        assert len(client._active_tasks) == 3
        
        # Call close() - should wait for all tasks
        await client.close()
        
        # All tasks should be completed
        assert len(task_completed) == 3
        assert len(client._active_tasks) == 0
        
        # Verify cleanup was called on the client
        mock_write_api.close.assert_called_once()
        mock_instance.close.assert_called_once()
    
    def test_close_sync_backwards_compatibility(self, mock_config, mock_influxdb_client):
        """Test that close_sync() provides backwards compatibility"""
        mock_instance, mock_write_api = mock_influxdb_client
        client = InfluxDBClient()
        
        # Call the synchronous close method
        client.close_sync()
        
        # Verify cleanup was called
        mock_write_api.close.assert_called_once()
        mock_instance.close.assert_called_once()
    
    @pytest.mark.asyncio
    async def test_write_sensor_data_in_test_env_skips_write(self, mock_config, mock_influxdb_client):
        """Test that write operations are skipped in test environment"""
        mock_config.IS_TEST_ENV = True
        client = InfluxDBClient()
        
        result = client.write_sensor_data("aa:bb:cc:dd:ee:ff", 85.5, 22.3)
        
        # Should return False when skipping in test env
        assert result is False
        
        # No tasks should be created
        assert len(client._active_tasks) == 0
