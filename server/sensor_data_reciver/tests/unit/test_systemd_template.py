"""Tests for the sensor_data_reciver systemd template."""

from pathlib import Path


def test_systemd_template_declares_boot_dependencies() -> None:
    template_path = (
        Path(__file__).resolve().parents[2]
        / "systemd"
        / "sensor_data_reciver.service"
    )
    content = template_path.read_text(encoding="utf-8")

    assert "After=network-online.target influxdb.service" in content
    assert "Wants=network-online.target influxdb.service" in content
