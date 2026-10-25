from dataclasses import dataclass
from typing import Literal, TypeGuard, TypeVar

CONTRACT_VERSION = 1

Risk = Literal["safe", "review", "guided"]
Reversibility = Literal["regenerable", "trash", "irreversible"]
Outcome = Literal["done", "skipped", "failed"]
Tag = Literal["safe", "review", "sudo", "unavailable", "empty"]

RISKS: tuple[Risk, ...] = ("safe", "review", "guided")
REVERSIBILITIES: tuple[Reversibility, ...] = ("regenerable", "trash", "irreversible")
OUTCOMES: tuple[Outcome, ...] = ("done", "skipped", "failed")

JsonObject = dict[str, object]
Choice = TypeVar("Choice", bound=str)


class ContractError(Exception): ...


@dataclass(frozen=True)
class Disk:
    free: int
    total: int


@dataclass(frozen=True)
class Explanation:
    what_it_is: str
    why_it_grows: str
    effect: str
    reversibility: str
    recreate_cost: str
    recommendation: str


@dataclass(frozen=True)
class Item:
    id: str
    label: str
    location: str
    bytes: int
    risk: Risk
    reversibility: Reversibility
    note: str | None
    block: str | None
    command: str | None

    @property
    def selectable(self) -> bool:
        return self.risk != "guided" and self.block is None

    @property
    def starts_selected(self) -> bool:
        return self.selectable and self.risk == "safe"


@dataclass(frozen=True)
class Category:
    id: str
    title: str
    summary: str
    explanation: Explanation
    footnote: str | None
    problem: str | None
    items: tuple[Item, ...]

    @property
    def total_bytes(self) -> int:
        return sum(item.bytes for item in self.items)

    @property
    def tag(self) -> Tag:
        if self.problem is not None:
            return "unavailable"
        if not self.items:
            return "empty"
        if all(item.risk == "guided" for item in self.items):
            return "sudo"
        if any(item.risk == "review" for item in self.items):
            return "review"
        return "safe"


@dataclass(frozen=True)
class Measurement:
    disk: Disk
    categories: tuple[Category, ...]


@dataclass(frozen=True)
class Progress:
    done: int
    total: int
    current: str | None


@dataclass(frozen=True)
class Plan:
    count: int
    estimated: int
    warnings: tuple[str, ...]


@dataclass(frozen=True)
class ItemOutcome:
    id: str
    label: str
    outcome: Outcome
    detail: str | None


@dataclass(frozen=True)
class CleanResult:
    estimated: int
    freed: int
    trash: int
    docker_vm: int
    maybe_stuck: bool
    items: tuple[ItemOutcome, ...]
    warnings: tuple[str, ...]


def parse_measurement(raw: object) -> Measurement:
    data = _object(raw, "medição")
    if data.get("version") != CONTRACT_VERSION:
        raise ContractError("versão do jawa incompatível com esta interface; atualize os dois")
    disk = _object(data.get("disk"), "disco")
    return Measurement(
        disk=Disk(free=_int(disk, "free"), total=_int(disk, "total")),
        categories=tuple(_category(entry) for entry in _list(data, "categories")),
    )


def parse_event(raw: object) -> Progress | CleanResult | Plan:
    data = _object(raw, "evento")
    kind = data.get("type")
    if kind == "progress":
        return Progress(
            done=_int(data, "done"),
            total=_int(data, "total"),
            current=_optional_str(data, "current"),
        )
    if kind == "result":
        return _clean_result(data)
    if kind == "plan":
        return Plan(
            count=_int(data, "count"),
            estimated=_int(data, "estimated"),
            warnings=_strings(data, "warnings"),
        )
    raise ContractError(f"evento desconhecido do jawa: {kind!r}")


def _category(raw: object) -> Category:
    data = _object(raw, "categoria")
    return Category(
        id=_str(data, "id"),
        title=_str(data, "title"),
        summary=_str(data, "summary"),
        explanation=_explanation(data.get("explanation")),
        footnote=_optional_str(data, "footnote"),
        problem=_optional_str(data, "problem"),
        items=tuple(_item(entry) for entry in _list(data, "items")),
    )


def _explanation(raw: object) -> Explanation:
    data = _object(raw, "explicação")
    return Explanation(
        what_it_is=_str(data, "what_it_is"),
        why_it_grows=_str(data, "why_it_grows"),
        effect=_str(data, "effect"),
        reversibility=_str(data, "reversibility"),
        recreate_cost=_str(data, "recreate_cost"),
        recommendation=_str(data, "recommendation"),
    )


def _item(raw: object) -> Item:
    data = _object(raw, "item")
    return Item(
        id=_str(data, "id"),
        label=_str(data, "label"),
        location=_str(data, "location"),
        bytes=_int(data, "bytes"),
        risk=_choice(data, "risk", RISKS),
        reversibility=_choice(data, "reversibility", REVERSIBILITIES),
        note=_optional_str(data, "note"),
        block=_optional_str(data, "block"),
        command=_optional_str(data, "command"),
    )


def _clean_result(data: JsonObject) -> CleanResult:
    return CleanResult(
        estimated=_int(data, "estimated"),
        freed=_int(data, "freed"),
        trash=_int(data, "trash"),
        docker_vm=_int(data, "docker_vm"),
        maybe_stuck=_bool(data, "maybe_stuck"),
        items=tuple(_outcome(entry) for entry in _list(data, "items")),
        warnings=_strings(data, "warnings"),
    )


def _outcome(raw: object) -> ItemOutcome:
    data = _object(raw, "desfecho")
    return ItemOutcome(
        id=_str(data, "id"),
        label=_str(data, "label"),
        outcome=_choice(data, "outcome", OUTCOMES),
        detail=_optional_str(data, "detail"),
    )


def _is_object(raw: object) -> TypeGuard[JsonObject]:
    return isinstance(raw, dict) and all(isinstance(key, str) for key in raw)


def _object(raw: object, where: str) -> JsonObject:
    if not _is_object(raw):
        raise ContractError(f"{where}: esperava um objeto JSON")
    return raw


def _list(data: JsonObject, key: str) -> list[object]:
    value = data.get(key)
    if not isinstance(value, list):
        raise ContractError(f"{key}: esperava uma lista")
    return [entry for entry in value]


def _as_str(value: object, key: str) -> str:
    if not isinstance(value, str):
        raise ContractError(f"{key}: esperava um texto")
    return value


def _strings(data: JsonObject, key: str) -> tuple[str, ...]:
    return tuple(_as_str(entry, key) for entry in _list(data, key))


def _str(data: JsonObject, key: str) -> str:
    return _as_str(data.get(key), key)


def _optional_str(data: JsonObject, key: str) -> str | None:
    value = data.get(key)
    return None if value is None else _as_str(value, key)


def _int(data: JsonObject, key: str) -> int:
    value = data.get(key)
    if not isinstance(value, int) or isinstance(value, bool) or value < 0:
        raise ContractError(f"{key}: esperava um número inteiro")
    return value


def _bool(data: JsonObject, key: str) -> bool:
    value = data.get(key)
    if not isinstance(value, bool):
        raise ContractError(f"{key}: esperava verdadeiro ou falso")
    return value


def _choice(data: JsonObject, key: str, choices: tuple[Choice, ...]) -> Choice:
    value = data.get(key)
    for choice in choices:
        if value == choice:
            return choice
    raise ContractError(f"{key}: valor inesperado {value!r}")
