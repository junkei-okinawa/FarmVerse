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


def test_systemd_units_are_wired_together() -> None:
    systemd = Path(__file__).resolve().parents[2] / "systemd"
    timer = (systemd / "host_storage_monitor.timer").read_text(encoding="utf-8")
    service = (systemd / "host_storage_monitor.service").read_text(encoding="utf-8")

    assert "Unit=host_storage_monitor.service" in timer
    assert "OnCalendar=" in timer
    assert "Type=oneshot" in service
    assert "network-online.target" in service
    assert "monitoring/host_storage_monitor.py" in service
