import os
from pathlib import Path

import pytest

from jawa_tui.core import JawaCli
from jawa_tui.models import Progress

BINARY = os.environ.get("JAWA_TEST_BIN")
TWO_MIB = 2 * 1024 * 1024

pytestmark = pytest.mark.skipif(not BINARY, reason="defina JAWA_TEST_BIN para rodar")


@pytest.fixture
def sandbox(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Path:
    for name, relative in [
        ("HOME", ""),
        ("XDG_CACHE_HOME", ".cache"),
        ("XDG_CONFIG_HOME", ".config"),
        ("XDG_DATA_HOME", ".local/share"),
        ("XDG_STATE_HOME", ".local/state"),
        ("NVM_DIR", ".nvm"),
        ("JAWA_SYSTEM_ROOT", "sistema"),
    ]:
        monkeypatch.setenv(name, str(tmp_path / relative))
    monkeypatch.setenv("PATH", str(tmp_path / "bin"))
    cache = tmp_path / ".npm/_cacache"
    cache.mkdir(parents=True)
    (cache / "blob").write_bytes(os.urandom(TWO_MIB))
    return tmp_path


def test_real_binary_measures_plans_and_cleans(sandbox: Path) -> None:
    core = JawaCli(Path(str(BINARY)))
    measurement = core.measure()
    npm = next(category for category in measurement.categories if category.id == "npm-cache")
    ids = [item.id for item in npm.items]
    assert core.plan(ids).count == 1
    seen: list[Progress] = []
    result = core.clean(ids, allow_irreversible=False, on_progress=seen.append)
    assert seen
    assert [item.outcome for item in result.items] == ["done"]
    assert not (sandbox / ".npm/_cacache").exists()
