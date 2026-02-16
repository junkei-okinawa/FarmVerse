# Python Image Viewer Service - systemd Setup

## Prerequisites
Before setting up as a service, make sure the application works by running the following commands in the repository root:

```bash
uv venv # Create virtual environment
source .venv/bin/activate # Activate virtual environment
uv sync # Install dependencies
.venv/bin/gunicorn main:app --bind 0.0.0.0:8000 --keyfile key.pem --certfile cert.pem --reload
```

## How to Run as a systemd Service

1. Copy the service file
   
   Copy `systemd/image_viewer.service` to `/etc/systemd/system/`:
   
   ```bash
   sudo cp systemd/image_viewer.service /etc/systemd/system/
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
   sudo systemctl enable image_viewer
   sudo systemctl start image_viewer
   ```

5. Check service status
   
   ```bash
   sudo systemctl status image_viewer
   ```

---

- To view logs: `journalctl -u image_viewer`
- To stop the service: `sudo systemctl stop image_viewer`
