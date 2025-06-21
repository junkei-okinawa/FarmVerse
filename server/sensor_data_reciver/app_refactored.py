"""
Async Sensor Data Receiver Application

Refactored sensor data receiver with modular architecture:
- Configuration management
- Protocol handling (frame parsing, serial communication)
- Data processing (image, voltage, temperature)
- Storage (InfluxDB)
- Utilities (logging)

Note: This file maintains backward compatibility with existing tests
by re-exporting necessary classes and functions.
"""

import argparse
import asyncio
import serial
import serial_asyncio

from config import config
from processors import ImageReceiver, ensure_dir_exists
from protocol import SerialProtocol
from utils import setup_logging

# Backward compatibility imports for existing tests
from processors.voltage_processor import VoltageDataProcessor
from processors.sleep_controller import determine_sleep_duration, format_sleep_command_to_gateway
from protocol.frame_parser import FrameParser

# Setup logging
logger = setup_logging()

# Global image receiver instance
image_receiver = ImageReceiver()


async def check_timeouts() -> None:
    """Periodically check for timed out image buffers."""
    while True:
        try:
            await asyncio.sleep(config.IMAGE_TIMEOUT)
            current_time = asyncio.get_event_loop().time()
            
            timed_out_macs = [
                mac for mac, last_time in list(image_receiver.last_receive_time.items())
                if current_time - last_time > config.IMAGE_TIMEOUT
            ]
            
            for mac in timed_out_macs:
                logger.warning(
                    f"Timeout waiting for data from {mac}. "
                    f"Discarding buffer ({len(image_receiver.image_buffers.get(mac, b''))} bytes)."
                )
                image_receiver._cleanup_buffer(mac)
                
            # メモリ使用量チェック
            image_receiver.check_memory_usage()
            
        except asyncio.CancelledError:
            logger.info("Timeout checker task cancelled.")
            break
        except Exception as e:
            logger.exception(f"Error in timeout checker: {e}")


async def main(port: str, baud: int) -> None:
    """Main asynchronous function."""
    ensure_dir_exists()
    logger.info("Starting Async USB CDC Image Receiver (Refactored)")
    logger.info(f"Images will be saved to: {config.IMAGE_DIR}")

    loop = asyncio.get_running_loop()
    timeout_task = loop.create_task(check_timeouts())

    while True:  # Reconnection loop
        transport = None
        connection_lost_future = loop.create_future()

        try:
            logger.info(f"Attempting to connect to {port} at {baud} baud...")

            def protocol_factory():
                return SerialProtocol(
                    connection_lost_future,
                    image_receiver.image_buffers,
                    image_receiver.last_receive_time,
                    image_receiver.stats
                )

            transport, protocol = await serial_asyncio.create_serial_connection(
                loop, protocol_factory, port, baudrate=baud
            )
            logger.info("Connection established.")

            logger.info("Monitoring connection (awaiting future)...")
            await connection_lost_future
            logger.info("Connection lost signaled (future completed).")

        except serial.SerialException as e:
            logger.error(f"Serial connection error: {e}")
            if not connection_lost_future.done():
                logger.warning("Setting future exception due to SerialException during connection.")
                connection_lost_future.set_exception(e)
                
        except asyncio.CancelledError:
            logger.info("Main task cancelled during connection/monitoring.")
            if connection_lost_future and not connection_lost_future.done():
                connection_lost_future.cancel("Main task cancelled")
            break
            
        except Exception as e:
            logger.exception(f"Error during connection or monitoring: {e}")
            if connection_lost_future and not connection_lost_future.done():
                try:
                    logger.warning(f"Setting future exception due to unexpected error: {e}")
                    connection_lost_future.set_exception(e)
                except asyncio.InvalidStateError:
                    pass
                    
        finally:
            if transport and not transport.is_closing():
                logger.info("Closing transport in finally block.")
                transport.close()
            transport = None

        if not loop.is_running():
            logger.warning("Event loop is not running. Exiting reconnection loop.")
            break

        logger.info("Waiting 5 seconds before retrying connection...")
        try:
            if connection_lost_future.done() and connection_lost_future.exception():
                logger.info(f"Previous connection ended with error: {connection_lost_future.exception()}")
            await asyncio.sleep(5)
        except asyncio.CancelledError:
            logger.info("Retry delay cancelled. Exiting reconnection loop.")
            break

    # Cleanup
    logger.info("Shutting down timeout task...")
    timeout_task.cancel()
    try:
        await timeout_task
    except asyncio.CancelledError:
        pass
    
    # Cleanup resources
    await image_receiver.cleanup_resources()
    logger.info("Application finished.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Async receive images via USB CDC (Refactored).")
    parser.add_argument(
        "-p", "--port", default=config.SERIAL_PORT,
        help=f"Serial port (default: {config.SERIAL_PORT})"
    )
    parser.add_argument(
        "-b", "--baud", type=int, default=config.BAUD_RATE,
        help=f"Baud rate (default: {config.BAUD_RATE})"
    )
    args = parser.parse_args()

    try:
        asyncio.run(main(args.port, args.baud))
    except KeyboardInterrupt:
        logger.info("Exiting due to KeyboardInterrupt.")
