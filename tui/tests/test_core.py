import json
import stat
from pathlib import Path

import pytest

from jawa_tui.core import CoreError, JawaCli, find_binary
from jawa_tui.models import Progress

FIXTURE = Path(__file__).parent / "fixtures" / "measurement.json"


def fake_binary(directory: Path, body: str) -> Path:
    program = directory / "jawa"
    program.write_text(f"#!/bin/sh\n{body}\n")
    program.chmod(program.stat().st_mode | stat.S_IEXEC)
    return program


def test_measure_parses_binary_output(tmp_path: Path) -> None:
    binary = fake_binary(tmp_path, f"cat '{FIXTURE}'")
    measurement = JawaCli(binary).measure()
    assert measurement.categories[0].id == "npm-cache"


def test_clean_streams_progress_and_passes_flags(tmp_path: Path) -> None:
    lines = [
        {"type": "progress", "done": 0, "total": 2, "current": "a"},
        {"type": "progress", "done": 1, "total": 2, "current": "b"},
        {
            "type": "result",
            "estimated": 10,
            "freed": 10,
            "trash": 0,
            "docker_vm": 0,
            "maybe_stuck": False,
            "items": [],
            "warnings": [],
        },
    ]
    script = "\n".join(f"echo '{json.dumps(line)}'" for line in lines)
    args_file = tmp_path / "args"
    binary = fake_binary(tmp_path, f"echo \"$@\" > '{args_file}'\n{script}")
    seen: list[Progress] = []
    result = JawaCli(binary).clean(["a1b2c3d4e5"], allow_irreversible=True, on_progress=seen.append)
    assert [progress.done for progress in seen] == [0, 1]
    assert result.freed == 10
    assert args_file.read_text().split() == [
        "limpar",
        "a1b2c3d4e5",
        "--sim",
        "--json",
        "--confirmar-irreversivel",
    ]


def test_failures_raise_core_error_with_stderr(tmp_path: Path) -> None:
    binary = fake_binary(
        tmp_path, "echo 'a medição venceu: rode `jawa medir` de novo.' >&2\nexit 1"
    )
    with pytest.raises(CoreError, match="jawa medir"):
        JawaCli(binary).clean(["a1b2c3d4e5"], allow_irreversible=False, on_progress=print)
    with pytest.raises(CoreError, match="jawa medir"):
        JawaCli(binary).measure()


def test_missing_binary_raises_core_error(tmp_path: Path) -> None:
    with pytest.raises(CoreError, match="não foi possível rodar"):
        JawaCli(tmp_path / "nao-existe").measure()


def test_find_binary_prefers_env_then_path(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    binary = fake_binary(tmp_path, "exit 0")
    monkeypatch.setenv("JAWA_BIN", str(binary))
    assert find_binary() == binary
    monkeypatch.delenv("JAWA_BIN")
    monkeypatch.setenv("PATH", str(tmp_path))
    assert find_binary() == binary
    monkeypatch.setenv("PATH", str(tmp_path / "empty"))
    assert find_binary() is None


def test_plan_runs_simulate_json(tmp_path: Path) -> None:
    args_file = tmp_path / "args"
    line = json.dumps({"type": "plan", "count": 1, "estimated": 9, "warnings": ["x"]})
    binary = fake_binary(tmp_path, f"echo \"$@\" > '{args_file}'\necho '{line}'")
    plan = JawaCli(binary).plan(["a1b2c3d4e5"])
    assert plan.warnings == ("x",)
    assert args_file.read_text().split() == ["limpar", "a1b2c3d4e5", "--simular", "--json"]
