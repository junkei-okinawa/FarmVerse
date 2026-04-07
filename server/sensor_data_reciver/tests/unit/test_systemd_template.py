"""Tests for the sensor_data_reciver systemd template."""

from pathlib import Path


def test_systemd_template_declares_boot_dependencies() -> None:
    template_path = (
        Path(__file__).resolve().parents[2]
        / "systemd"
        / "sensor_data_reciver.service"
    )
    content = template_path.read_text(encoding="utf-8")

    assert "After=" in content
    assert "Wants=" in content

    after_line = next(
        line for line in content.splitlines() if line.startswith("After=")
    )
    wants_line = next(
        line for line in content.splitlines() if line.startswith("Wants=")
    )

    assert "network-online.target" in after_line
    assert "influxdb.service" in after_line
    assert "network-online.target" in wants_line
    assert "influxdb.service" in wants_line
