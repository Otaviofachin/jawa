import json
import os
import shutil
import subprocess
import tempfile
from collections.abc import Callable, Sequence
from pathlib import Path
from typing import IO, Protocol

from jawa_tui.models import (
    CleanResult,
    ContractError,
    Measurement,
    Plan,
    Progress,
    parse_event,
    parse_measurement,
)

BINARY_ENV = "JAWA_BIN"
BINARY_NAME = "jawa"


class CoreError(Exception): ...


class Core(Protocol):
    def measure(self) -> Measurement: ...

    def plan(self, ids: Sequence[str]) -> Plan: ...

    def clean(
        self,
        ids: Sequence[str],
        allow_irreversible: bool,
        on_progress: Callable[[Progress], None],
    ) -> CleanResult: ...


class JawaCli:
    def __init__(self, binary: Path) -> None:
        self.binary = binary

    def measure(self) -> Measurement:
        return parse_measurement(_json(self._run(["medir", "--json"])))

    def plan(self, ids: Sequence[str]) -> Plan:
        output = self._run(["limpar", *ids, "--simular", "--json"])
        event = parse_event(_json(output.strip()))
        if not isinstance(event, Plan):
            raise ContractError("o --simular não devolveu o plano")
        return event

    def clean(
        self,
        ids: Sequence[str],
        allow_irreversible: bool,
        on_progress: Callable[[Progress], None],
    ) -> CleanResult:
        args = ["limpar", *ids, "--sim", "--json"]
        if allow_irreversible:
            args.append("--confirmar-irreversivel")
        with tempfile.TemporaryFile("w+") as errors:
            process = self._start(args, stderr=errors)
            result = self._read_events(process, on_progress)
            failed = process.wait() != 0
            errors.seek(0)
            if failed or result is None:
                raise CoreError(_failure_message(errors.read()))
        return result

    def _run(self, args: list[str]) -> str:
        process = self._start(args)
        stdout, stderr = process.communicate()
        if process.returncode != 0:
            raise CoreError(_failure_message(stderr))
        return stdout

    def _start(
        self, args: list[str], stderr: IO[str] | int = subprocess.PIPE
    ) -> "subprocess.Popen[str]":
        try:
            return subprocess.Popen(
                [str(self.binary), *args],
                stdin=subprocess.DEVNULL,
                stdout=subprocess.PIPE,
                stderr=stderr,
                text=True,
            )
        except OSError as error:
            raise CoreError(f"não foi possível rodar {self.binary}: {error}") from error

    def _read_events(
        self,
        process: "subprocess.Popen[str]",
        on_progress: Callable[[Progress], None],
    ) -> CleanResult | None:
        result: CleanResult | None = None
        for line in process.stdout or ():
            event = parse_event(_json(line))
            if isinstance(event, Progress):
                on_progress(event)
            elif isinstance(event, CleanResult):
                result = event
        return result


def _json(text: str) -> object:
    try:
        parsed: object = json.loads(text)
    except json.JSONDecodeError as error:
        raise ContractError(f"o jawa não respondeu em JSON: {error}") from error
    return parsed


def _failure_message(stderr: str) -> str:
    message = stderr.strip()
    return message or "o jawa terminou com erro sem dizer o motivo; veja o jawa.log"


def find_binary() -> Path | None:
    from_env = os.environ.get(BINARY_ENV)
    if from_env:
        return Path(from_env)
    found = shutil.which(BINARY_NAME)
    return Path(found) if found else None
