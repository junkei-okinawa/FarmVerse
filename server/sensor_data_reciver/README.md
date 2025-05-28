# Sensor Data Receiver Python Server

This Python script is an asynchronous server that receives image data transmitted from ESP32-C3 (usb_cdc_receiver) via USB CDC (Communications Device Class) and saves them as JPEG files on the host PC.

## Project Overview

This project works in conjunction with the `usb_cdc_receiver` project to receive and process image data transmitted from multiple ESP32 cameras via ESP-NOW and relayed by ESP32-C3.

## Key Features

-   **Asynchronous Serial Communication**: Uses `serial_asyncio` to efficiently receive data from serial ports.
-   **Custom Frame Protocol Parsing**:
    -   Frame synchronization using `START_MARKER` and `END_MARKER`.
    -   Extracts source MAC address, frame type, sequence number, and data length from headers.
    -   Footer checksum (validation not currently implemented).
-   **Image Data Reconstruction**: Buffers payloads of data frames (`FRAME_TYPE_DATA`) for each source MAC address.
-   **Image File Saving**: When an end frame (`FRAME_TYPE_EOF`) is received, combines the buffer for the corresponding MAC address and saves it as a timestamped JPEG file in the `images_usb_async/` directory.
-   **Timeout Handling**: Discards buffers for MAC addresses that haven't received data for a certain period to prevent resource leaks.
-   **Statistics**: Periodically outputs the number of received images and total bytes to the log.
-   **Configurability**: Allows specifying serial port and baud rate via command line arguments.

## Usage

### Requirements

-   Python 3.11 or higher
-   `pyserial-asyncio` library (`pyserial` is also automatically installed)

### Setup

1.  **Install Dependencies**:
    Navigate to the project directory (`examples/python_server`) and install dependencies using `uv` or `pip`.
    ```bash
    cd examples/python_server
    # Using uv (recommended)
    uv sync
    # Using pip
    # pip install .
    ```
    This will install the necessary libraries (`pyserial` and `pyserial-asyncio`) based on `pyproject.toml`.

2.  **Image Storage Directory**:
    The script automatically creates the `images_usb_async` directory when executed.

### Execution

Start the server with the following command. Specify the serial port where your ESP32-C3 device is connected.

If you installed dependencies using `uv`, it's recommended to run with the following command:

```bash
uv run python app.py [options]
```

If you're not using `uv` or want to use the Python interpreter directly, you can also run:

```bash
python app.py [options]
```

**Options:**

-   `-p`, `--port`: Serial port path (default: `/dev/ttyACM0`)
-   `-b`, `--baud`: Baud rate (default: 115200)

**Examples:**

```bash
# Run with default settings (using uv)
uv run python app.py

# Run with specified serial port (using uv)
uv run python app.py -p /dev/ttyUSB0

# Run with specified port and baud rate (using uv)
uv run python app.py -p /dev/cu.usbmodem12341 -b 115200

# Run with default settings (using python directly)
# python app.py
```

Once started, the server begins receiving data from the specified serial port. Received images are saved to the `images_usb_async` directory. You can stop the server with Ctrl+C.

## Data Protocol

This server expects the following custom frame format transmitted from `usb_cdc_receiver`.

```
[START_MARKER (4B)] [MAC Address (6B)] [Frame Type (1B)] [Sequence Num (4B)] [Data Length (4B)] [Data (variable)] [Checksum (4B)] [END_MARKER (4B)]
```

-   **START_MARKER**: `0xfa 0xce 0xaa 0xbb`
-   **MAC Address**: MAC address of the source camera
-   **Frame Type**:
    -   `1`: HASH (currently unused)
    -   `2`: DATA (part of image data)
    -   `3`: EOF (final frame of the image)
-   **Sequence Num**: Frame sequence number (big-endian)
-   **Data Length**: Byte length of the `Data` field (big-endian)
-   **Data**: Payload according to frame type (image data fragment for DATA frames)
-   **Checksum**: Checksum of the data portion (currently not validated on the server side)
-   **END_MARKER**: `0xcd 0xef 0x56 0x78`

## Configuration

The following constants can be modified directly in the `app.py` script.

-   `DEFAULT_SERIAL_PORT`: Default serial port
-   `BAUD_RATE`: Default baud rate
-   `IMAGE_DIR`: Directory name for saving images
-   `IMAGE_TIMEOUT`: Timeout period for image data reception (seconds)

## Debugging

Setting the `DEBUG_FRAME_PARSING` flag to `True` in `app.py` will output detailed logs related to frame parsing.

```python
# app.py
# ...
DEBUG_FRAME_PARSING = True # Enable detailed logging
# ...
```

Logs are displayed on standard output.

## License

This project is based on the [LICENSE](../../LICENSE) file in the repository root.

## Contributing

Bug reports and improvement suggestions are welcome through Issues and Pull Requests on the GitHub repository.
