"""Unit tests for InfluxDB client async task tracking"""

import asyncio
import contextlib
import time
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
            mock_config.DRY_RUN = False
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
            
            # Call write_sensor_data with TDS voltage
            result = client.write_sensor_data("aa:bb:cc:dd:ee:ff", 85.5, 22.3, 1.5)
            
            # Should return True for successful initiation
            assert result is True
            
            # Give a moment for the task to be created
            await asyncio.sleep(0.01)
            
            # Check that a task was added to active tasks
            assert len(client._active_tasks) == 1
            
            # Wait for the task to complete
            await asyncio.gather(*client._active_tasks, return_exceptions=True)
            client._active_tasks.clear()
            
            # Verify the async methods were called with TDS voltage
            mock_write_async.assert_called_once_with("aa:bb:cc:dd:ee:ff", 85.5, 22.3, 1.5)
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

    def test_initialize_client_recovers_after_initial_failure(self, mock_config):
        """Test that the client can recover after an initial health-check failure"""
        with patch('storage.influxdb_client.influxdb_client.InfluxDBClient') as mock_client_cls:
            first_instance = MagicMock()
            first_write_api = MagicMock()
            first_instance.write_api.return_value = first_write_api
            first_instance.health.return_value.status = "fail"

            second_instance = MagicMock()
            second_write_api = MagicMock()
            second_instance.write_api.return_value = second_write_api
            second_instance.health.return_value.status = "pass"

            mock_client_cls.side_effect = [first_instance, second_instance]

            client = InfluxDBClient()

            assert client.client is None
            assert client.write_api is None

            client._last_init_failure_at = 0.0
            ready = client._ensure_client_ready_sync("aa:bb:cc:dd:ee:ff")

            assert ready is True
            assert client.client is second_instance
            assert client.write_api is second_write_api
            assert mock_client_cls.call_count == 2
    
    @pytest.mark.asyncio
    async def test_write_sensor_data_in_test_env_skips_write(self, mock_config, mock_influxdb_client):
        """Test that write operations are skipped in test environment"""
        mock_config.IS_TEST_ENV = True
        client = InfluxDBClient()

        result = client.write_sensor_data("aa:bb:cc:dd:ee:ff", 85.5, 22.3, 2.1)

        # Should return False when skipping in test env
        assert result is False

        # No tasks should be created
        assert len(client._active_tasks) == 0

    @pytest.mark.asyncio
    async def test_write_sensor_data_in_dry_run_skips_write(self, mock_config, mock_influxdb_client):
        """Test that write operations are skipped in DRY_RUN mode"""
        mock_config.DRY_RUN = True
        client = InfluxDBClient()

        result = client.write_sensor_data("aa:bb:cc:dd:ee:ff", 85.5, 22.3, 2.1)

        # Should return False when DRY_RUN is enabled
        assert result is False

        # No tasks should be created
        assert len(client._active_tasks) == 0

    @pytest.mark.asyncio
    async def test_write_sensor_data_recovers_after_initial_failure(self, mock_config):
        """Test that write_sensor_data can recover from an initial init failure"""
        with patch('storage.influxdb_client.influxdb_client.InfluxDBClient') as mock_client_cls:
            first_instance = MagicMock()
            first_write_api = MagicMock()
            first_instance.write_api.return_value = first_write_api
            first_instance.health.return_value.status = "fail"

            second_instance = MagicMock()
            second_write_api = MagicMock()
            second_instance.write_api.return_value = second_write_api
            second_instance.health.return_value.status = "pass"

            mock_client_cls.side_effect = [first_instance, second_instance]

            client = InfluxDBClient()
            client._last_init_failure_at = 0.0

            result = client.write_sensor_data("aa:bb:cc:dd:ee:ff", 85.5, 22.3, 4.4)

            assert result is True

            await asyncio.gather(*client._active_tasks, return_exceptions=True)
            client._active_tasks.clear()

            second_write_api.write.assert_called_once()
            assert client.client is second_instance
            assert client.write_api is second_write_api

    @pytest.mark.asyncio
    async def test_write_sensor_data_with_tds_voltage(self, mock_config, mock_influxdb_client):
        """Test that write_sensor_data works with TDS voltage parameter"""
        mock_instance, mock_write_api = mock_influxdb_client
        
        # Mock health check to return success
        mock_instance.health.return_value.status = "pass"
        
        client = InfluxDBClient()
        
        # Mock the async write method to return immediately
        with patch.object(client, '_write_sensor_data_async', new_callable=AsyncMock) as mock_write_async, \
             patch.object(client, '_cleanup_completed_tasks', new_callable=AsyncMock) as mock_cleanup:
            
            # Call write_sensor_data with TDS voltage
            result = client.write_sensor_data("aa:bb:cc:dd:ee:ff", 85.5, 22.3, 3.2)
            
            # Should return True for successful initiation
            assert result is True
            
            # Give a moment for the task to be created
            await asyncio.sleep(0.01)
            
            # Check that a task was added to active tasks
            assert len(client._active_tasks) == 1
            
            # Wait for the task to complete
            await asyncio.gather(*client._active_tasks, return_exceptions=True)
            client._active_tasks.clear()
            
            # Verify the async methods were called with TDS voltage
            mock_write_async.assert_called_once_with("aa:bb:cc:dd:ee:ff", 85.5, 22.3, 3.2)
            mock_cleanup.assert_called_once()

    @pytest.mark.asyncio
    async def test_write_sensor_data_without_tds_voltage(self, mock_config, mock_influxdb_client):
        """Test that write_sensor_data works without TDS voltage parameter (backwards compatibility)"""
        mock_instance, mock_write_api = mock_influxdb_client
        
        # Mock health check to return success
        mock_instance.health.return_value.status = "pass"
        
        client = InfluxDBClient()
        
        # Mock the async write method to return immediately
        with patch.object(client, '_write_sensor_data_async', new_callable=AsyncMock) as mock_write_async, \
             patch.object(client, '_cleanup_completed_tasks', new_callable=AsyncMock) as mock_cleanup:
            
            # Call write_sensor_data without TDS voltage (backwards compatibility)
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
            
            # Verify the async methods were called with None for TDS voltage
            mock_write_async.assert_called_once_with("aa:bb:cc:dd:ee:ff", 85.5, 22.3, None)
            mock_cleanup.assert_called_once()

    @pytest.mark.asyncio
    async def test_timeout_does_not_close_client_resources(self, mock_config, mock_influxdb_client):
        """Test that a write timeout only records failure and does not close the shared client."""
        mock_instance, mock_write_api = mock_influxdb_client
        mock_instance.health.return_value.status = "pass"

        client = InfluxDBClient()

        created_tasks = []

        def fake_to_thread(*args, **kwargs):
            task = asyncio.create_task(asyncio.sleep(10))
            created_tasks.append(task)
            return task

        with patch('storage.influxdb_client.asyncio.to_thread', new=fake_to_thread), \
             patch('storage.influxdb_client.asyncio.wait_for', side_effect=asyncio.TimeoutError), \
             patch.object(client, '_disable_client', wraps=client._disable_client) as mock_disable:
            await client._write_sensor_data_async("aa:bb:cc:dd:ee:ff", 85.5, 22.3, 4.4)

        mock_disable.assert_not_called()
        mock_write_api.write.assert_not_called()
        assert client.client is mock_instance
        assert client.write_api is mock_write_api

        for task in created_tasks:
            task.cancel()
            with contextlib.suppress(asyncio.CancelledError):
                await task

    @pytest.mark.asyncio
    async def test_recent_write_failure_skips_new_write(self, mock_config, mock_influxdb_client):
        """Test that a recent write failure activates cooldown and suppresses new writes."""
        mock_instance, mock_write_api = mock_influxdb_client
        mock_instance.health.return_value.status = "pass"

        client = InfluxDBClient()
        client._last_write_failure_at = time.monotonic()

        with patch.object(client, '_ensure_client_ready_async', new_callable=AsyncMock) as mock_ready, \
             patch('storage.influxdb_client.asyncio.wait_for') as mock_wait_for:
            mock_ready.return_value = True
            await client._write_sensor_data_async("aa:bb:cc:dd:ee:ff", 85.5, 22.3, 4.4)

        mock_wait_for.assert_not_called()
        mock_write_api.write.assert_not_called()
        mock_ready.assert_called_once()

    @pytest.mark.asyncio
    async def test_async_ready_check_skips_duplicate_initialization_attempts(self, mock_config):
        """Test that the async readiness check does not queue duplicate init work."""
        with patch('storage.influxdb_client.influxdb_client.InfluxDBClient') as mock_client_cls:
            first_instance = MagicMock()
            first_write_api = MagicMock()
            first_instance.write_api.return_value = first_write_api
            first_instance.health.return_value.status = "fail"

            mock_client_cls.return_value = first_instance

            client = InfluxDBClient()
            client._last_init_failure_at = 0.0

            with patch.object(client, '_claim_initialization_slot', return_value=False), \
                 patch('storage.influxdb_client.asyncio.to_thread') as mock_to_thread:
                ready = await client._ensure_client_ready_async("aa:bb:cc:dd:ee:ff")

            assert ready is False
            mock_to_thread.assert_not_called()
