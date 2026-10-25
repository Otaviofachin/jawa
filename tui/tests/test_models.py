import json
from pathlib import Path

import pytest

from jawa_tui.models import (
    Category,
    CleanResult,
    ContractError,
    Item,
    Plan,
    Progress,
    parse_event,
    parse_measurement,
)
from tests.factories import category, item

FIXTURE = Path(__file__).parent / "fixtures" / "measurement.json"


def test_parse_measurement_reads_real_output() -> None:
    measurement = parse_measurement(json.loads(FIXTURE.read_text()))
    assert measurement.disk.free == 49_900_000_000
    assert measurement.disk.total == 249_800_000_000
    assert len(measurement.categories) == 12
    npm = measurement.categories[0]
    assert npm.id == "npm-cache"
    assert npm.title == "Cache do npm"
    assert npm.explanation.what_it_is
    assert npm.items[0].risk == "safe"
    assert npm.items[0].reversibility == "regenerable"
    assert len(npm.items[0].id) == 10
    system = measurement.categories[-1]
    assert system.items[0].command == "sudo rm -f /var/crash/*.crash"
    assert measurement.categories[3].problem == "a ferramenta git não foi encontrada"


def test_parsers_reject_unexpected_json() -> None:
    with pytest.raises(ContractError):
        parse_measurement({"versao": 1, "disco": "cheio"})
    with pytest.raises(ContractError):
        parse_measurement({"versao": 99, "disco": {}, "categorias": []})
    with pytest.raises(ContractError):
        parse_event({"type": "outra-coisa"})


def test_parse_event_reads_progress_and_result() -> None:
    progress = parse_event({"type": "progress", "done": 1, "total": 3, "current": "npm"})
    assert progress == Progress(done=1, total=3, current="npm")
    result = parse_event(
        {
            "type": "result",
            "estimated": 10,
            "freed": 4,
            "trash": 3,
            "docker_vm": 3,
            "maybe_stuck": False,
            "items": [{"id": "a1b2c3d4e5", "label": "x", "outcome": "done", "detail": None}],
            "warnings": ["cuidado"],
        }
    )
    assert isinstance(result, CleanResult)
    assert result.freed == 4
    assert result.trash == 3
    assert result.docker_vm == 3
    assert result.items[0].outcome == "done"
    assert result.warnings == ("cuidado",)


def test_parse_event_reads_plan() -> None:
    plan = parse_event({"type": "plan", "count": 2, "estimated": 5, "warnings": ["a"]})
    assert plan == Plan(count=2, estimated=5, warnings=("a",))


def test_category_tag_follows_risk_rules() -> None:
    safe = item("a", risk="safe")
    review = item("b", risk="review")
    guided = item("c", risk="guided", command="sudo apt-get clean")
    assert category("x", (safe,)).tag == "safe"
    assert category("x", (safe, review)).tag == "review"
    assert category("x", (guided,)).tag == "sudo"
    assert category("x", ()).tag == "empty"
    assert category("x", (), problem="docker parado").tag == "unavailable"


def test_item_selection_follows_risk_and_block() -> None:
    assert item("a", risk="safe").starts_selected
    assert not item("b", risk="review").starts_selected
    assert item("b", risk="review").selectable
    assert not item("c", risk="guided").selectable
    blocked = item("d", risk="safe", block="em uso por chrome (12 processos)")
    assert not blocked.selectable
    assert not blocked.starts_selected


def test_category_total_sums_items() -> None:
    cat: Category = category("x", (item("a", size=2), item("b", size=3)))
    assert cat.total_bytes == 5
    assert isinstance(cat.items[0], Item)
