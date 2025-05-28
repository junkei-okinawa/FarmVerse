# Python USB CDC Sensor Data Receiver Service - systemd Setup

## Prerequisites
Before setting up as a service, make sure the application works by running the following commands in the repository root:

```bash
uv venv # Create virtual environment
source .venv/bin/activate # Activate virtual environment
uv sync # Install dependencies
.venv/bin/python app.py
```

## How to Run as a systemd Service

1. Copy the service file
   
   Copy `systemd/sensor_data_reciver.service` to `/etc/systemd/system/`:
   
   ```bash
   sudo cp systemd/sensor_data_reciver.service /etc/systemd/system/
   ```

2. Edit the service file
   
   - Replace `<user_name>` with the user that should run the service.
   - If you need to specify a group, replace `<group_name>` with the group name. If not needed, remove the `Group` line.

3. Reload systemd
   
   ```bash
   sudo systemctl daemon-reload
   ```

4. Enable and start the service
   
   ```bash
   sudo systemctl enable sensor_data_reciver
   sudo systemctl start sensor_data_reciver
   ```

5. Check service status
   
   ```bash
   sudo systemctl status sensor_data_reciver
   ```

---

- To view logs: `journalctl -u sensor_data_reciver`
- To stop the service: `sudo systemctl stop sensor_data_reciver`
