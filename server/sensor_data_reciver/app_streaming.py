"""
Streaming-enabled Sensor Data Receiver Application

This is the streaming-aware version of the sensor data receiver that uses
StreamingImageProcessor and StreamingSerialProtocol for real-time processing.
"""

import argparse
import asyncio
import serial
import serial_asyncio

from config import config
from processors import ensure_dir_exists
from protocol import StreamingSerialProtocol
from utils import setup_logging

# Setup logging
logger = setup_logging()


async def main_streaming(port: str, baud: int) -> None:
    """メインのストリーミング処理関数"""
    ensure_dir_exists()
    logger.info("Starting Streaming Sensor Data Receiver")
    logger.info(f"Images will be saved to: {config.IMAGE_DIR}")

    loop = asyncio.get_running_loop()

    while True:  # 再接続ループ
        transport = None
        connection_lost_future = loop.create_future()

        try:
            logger.info(f"Attempting to connect to {port} at {baud} baud...")

            def protocol_factory():
                # 共有統計情報
                stats = {"received_images": 0, "total_bytes": 0}
                return StreamingSerialProtocol(connection_lost_future, stats)

            transport, protocol = await serial_asyncio.create_serial_connection(
                loop, protocol_factory, port, baudrate=baud
            )
            logger.info("Streaming connection established.")

            logger.info("Monitoring streaming connection...")
            await connection_lost_future
            logger.info("Streaming connection lost.")

        except serial.SerialException as e:
            logger.error(f"Serial connection error: {e}")
            if not connection_lost_future.done():
                connection_lost_future.set_exception(e)
                
        except asyncio.CancelledError:
            logger.info("Streaming task cancelled.")
            if connection_lost_future and not connection_lost_future.done():
                connection_lost_future.cancel("Streaming task cancelled")
            break
            
        except Exception as e:
            logger.exception(f"Error during streaming connection: {e}")
            if connection_lost_future and not connection_lost_future.done():
                try:
                    connection_lost_future.set_exception(e)
                except asyncio.InvalidStateError:
                    pass
                    
        finally:
            if transport and not transport.is_closing():
                logger.info("Closing streaming transport.")
                transport.close()
            transport = None

        if not loop.is_running():
            logger.warning("Event loop not running. Exiting.")
            break

        logger.info("Waiting 5 seconds before retrying streaming connection...")
        try:
            await asyncio.sleep(5)
        except asyncio.CancelledError:
            logger.info("Retry delay cancelled.")
            break

    logger.info("Streaming application finished.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Streaming Sensor Data Receiver")
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
        asyncio.run(main_streaming(args.port, args.baud))
    except KeyboardInterrupt:
        logger.info("Exiting due to KeyboardInterrupt.")
