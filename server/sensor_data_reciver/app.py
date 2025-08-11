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
import io
import os
import serial
import serial_asyncio
from datetime import datetime
from PIL import Image
import influxdb_client
from dotenv import load_dotenv
from influxdb_client.client.write_api import SYNCHRONOUS

from config import config
from processors import ImageReceiver, ensure_dir_exists
from protocol import SerialProtocol
from utils import setup_logging

# Backward compatibility imports for existing tests
from processors.voltage_processor import VoltageDataProcessor
from processors.sleep_controller import determine_sleep_duration, format_sleep_command_to_gateway
# Note: save_image is defined as a wrapper function below
from protocol.frame_parser import FrameParser as _FrameParser
from protocol.constants import (
    MAC_ADDRESS_LENGTH, FRAME_TYPE_LENGTH, SEQUENCE_NUM_LENGTH, 
    LENGTH_FIELD_BYTES, CHECKSUM_LENGTH, START_MARKER, END_MARKER,
    FRAME_TYPE_HASH, FRAME_TYPE_DATA, FRAME_TYPE_EOF, HEADER_LENGTH, FOOTER_LENGTH
)

# Backward compatibility wrapper for FrameParser
class FrameParser(_FrameParser):
    @staticmethod
    def validate_frame_data(data_len: int, mac_bytes: bytes) -> bool:
        """Backward compatibility wrapper for existing tests"""
        return _FrameParser.validate_frame_data(data_len, mac_bytes, config.MAX_DATA_LEN)

# Backward compatibility function for tests
def write_file_sync(filename: str, data: bytes) -> None:
    """Synchronous helper function to write file data."""
    with open(filename, "wb") as f:
        f.write(data)

    # 回転画像保存
    try:
        # バイト列から Image オブジェクト生成
        im = Image.open(io.BytesIO(data))
        # 左90度回転
        rotated = im.rotate(90, expand=True)
        # ファイル名から MAC 部分だけ取り出し
        base = os.path.splitext(os.path.basename(filename))[0].split("_")[0]
        rotated_filename = os.path.join(config.IMAGE_DIR, f"{base}.jpg")
        rotated.save(rotated_filename)
        logger.info(f"Saved rotated image: {rotated_filename}")
    except Exception as e:
        logger.error(f"Error saving rotated image: {e}")

# Backward compatibility wrapper for save_image
async def save_image(sender_mac_str: str, image_data: bytes, stats: dict = None) -> None:
    """Backward compatibility wrapper for save_image function."""
    from processors.image_processor import save_image as _save_image
    if stats is None:
        stats = {}
    await _save_image(sender_mac_str, image_data, stats)

# Setup logging
logger = setup_logging()

# Backward compatibility: InfluxDB client for tests
load_dotenv()
token = os.environ.get("INFLUXDB_TOKEN")
client = influxdb_client.InfluxDBClient(url=config.INFLUXDB_URL, token=token, org=config.INFLUXDB_ORG)
write_api = client.write_api(write_options=SYNCHRONOUS)

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
    parser.add_argument(
        "--streaming", action="store_true",
        help="Enable streaming mode (experimental)"
    )
    args = parser.parse_args()

    if args.streaming:
        logger.info("Starting in STREAMING mode")
        from protocol import StreamingSerialProtocol
        
        async def main_streaming(port: str, baud: int) -> None:
            """ストリーミングモードのメイン関数"""
            ensure_dir_exists()
            logger.info("Starting Streaming Sensor Data Receiver")
            logger.info(f"Images will be saved to: {config.IMAGE_DIR}")

            loop = asyncio.get_running_loop()

            while True:  # 再接続ループ
                transport = None
                connection_lost_future = loop.create_future()

                try:
                    logger.info(f"Attempting streaming connection to {port} at {baud} baud...")

                    def streaming_protocol_factory():
                        # 共有統計情報
                        stats = {"received_images": 0, "total_bytes": 0}
                        return StreamingSerialProtocol(connection_lost_future, stats)

                    transport, protocol = await serial_asyncio.create_serial_connection(
                        loop, streaming_protocol_factory, port, baudrate=baud
                    )
                    logger.info("Streaming connection established.")

                    await connection_lost_future

                except serial.SerialException as e:
                    logger.error(f"Streaming serial connection error: {e}")
                    if not connection_lost_future.done():
                        connection_lost_future.set_exception(e)
                        
                except asyncio.CancelledError:
                    logger.info("Streaming task cancelled.")
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
                        transport.close()
                    transport = None

                logger.info("Waiting 5 seconds before retrying streaming connection...")
                try:
                    await asyncio.sleep(5)
                except asyncio.CancelledError:
                    break

            logger.info("Streaming application finished.")
        
        try:
            asyncio.run(main_streaming(args.port, args.baud))
        except KeyboardInterrupt:
            logger.info("Exiting streaming mode due to KeyboardInterrupt.")
    else:
        logger.info("Starting in LEGACY mode")
        try:
            asyncio.run(main(args.port, args.baud))
        except KeyboardInterrupt:
            logger.info("Exiting due to KeyboardInterrupt.")
