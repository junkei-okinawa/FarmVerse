"""Tests for monitoring.host_storage_monitor."""

import os
import sys
from pathlib import Path

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", ".."))

from monitoring import host_storage_monitor as m


def test_dir_size_counts_files_recursively(tmp_path: Path) -> None:
    (tmp_path / "a.jpg").write_bytes(b"x" * 10)
    (tmp_path / "sub").mkdir()
    (tmp_path / "sub" / "b.jpg").write_bytes(b"y" * 5)

    assert m.dir_size(tmp_path) == (15, 2)


def test_dir_size_missing_dir_returns_zero(tmp_path: Path) -> None:
    assert m.dir_size(tmp_path / "missing") == (0, 0)


def test_collect_reports_all_fields(tmp_path: Path) -> None:
    images = tmp_path / "images"
    images.mkdir()
    (images / "a.jpg").write_bytes(b"x" * 7)
    home = tmp_path / "home"
    home.mkdir()
    (home / ".xsession-errors").write_bytes(b"a" * 3)
    (home / ".xsession-errors.old").write_bytes(b"b" * 4)

    fields = m.collect(str(tmp_path), images, home)

    assert fields["images_bytes"] == 7
    assert fields["images_files"] == 1
    assert fields["xsession_errors_bytes"] == 7
    assert 0 <= fields["used_percent"] <= 100
    assert fields["total_bytes"] >= fields["used_bytes"]


def test_collect_zero_total_does_not_raise(tmp_path: Path, monkeypatch) -> None:
    import shutil
    from collections import namedtuple

    UsageTuple = namedtuple("usage", ["total", "used", "free"])
    monkeypatch.setattr(shutil, "disk_usage", lambda _mount: UsageTuple(0, 0, 0))

    fields = m.collect(str(tmp_path), tmp_path / "images", tmp_path / "home")

    assert fields["used_percent"] == 0.0


def test_write_preserves_int_field_types(monkeypatch) -> None:
    recorded: dict[str, object] = {}

    class FakePoint:
        def __init__(self, _measurement: str) -> None:
            pass

        def tag(self, *_args, **_kwargs):
            return self

        def field(self, key, value):
            recorded[key] = value
            return self

    class FakeWriteApi:
        def write(self, **_kwargs) -> None:
            pass

    class FakeClient:
        def __init__(self, **_kwargs) -> None:
            pass

        def __enter__(self):
            return self

        def __exit__(self, *_args) -> None:
            pass

        def write_api(self, **_kwargs):
            return FakeWriteApi()

    import influxdb_client

    monkeypatch.setattr(influxdb_client, "Point", FakePoint)
    monkeypatch.setattr(influxdb_client, "InfluxDBClient", FakeClient)

    m.write({"images_bytes": 7, "used_percent": 12.5}, "/")

    assert recorded["images_bytes"] == 7
    assert isinstance(recorded["images_bytes"], int)
    assert recorded["used_percent"] == 12.5


def test_systemd_units_are_wired_together() -> None:
    systemd = Path(__file__).resolve().parents[2] / "systemd"
    timer = (systemd / "host_storage_monitor.timer").read_text(encoding="utf-8")
    service = (systemd / "host_storage_monitor.service").read_text(encoding="utf-8")

    assert "Unit=host_storage_monitor.service" in timer
    assert "OnCalendar=" in timer
    assert "Type=oneshot" in service
    assert "network-online.target" in service
    assert "monitoring/host_storage_monitor.py" in service
