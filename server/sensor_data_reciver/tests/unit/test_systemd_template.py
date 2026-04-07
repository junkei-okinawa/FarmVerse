"""Tests for the sensor_data_reciver systemd template."""

from pathlib import Path


def test_systemd_template_declares_boot_dependencies() -> None:
    template_path = (
        Path(__file__).resolve().parents[2]
        / "systemd"
        / "sensor_data_reciver.service"
    )
    content = template_path.read_text(encoding="utf-8")

    after_lines = [
        line.lstrip()
        for line in content.splitlines()
        if line.lstrip().startswith("After=")
    ]
    wants_lines = [
        line.lstrip()
        for line in content.splitlines()
        if line.lstrip().startswith("Wants=")
    ]

    after_value = " ".join(line.split("=", 1)[1] for line in after_lines)
    wants_value = " ".join(line.split("=", 1)[1] for line in wants_lines)

    assert "network-online.target" in after_value
    assert "influxdb.service" in after_value
    assert "network-online.target" in wants_value
    assert "influxdb.service" in wants_value
