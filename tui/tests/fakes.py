from collections.abc import Callable, Sequence

from jawa_tui.core import CoreError
from jawa_tui.models import CleanResult, Disk, Measurement, Plan, Progress
from tests.factories import category, item

NPM_FIRST = item("npm1", label="_cacache", size=2_000_000_000)
NPM_SECOND = item("npm2", label="_npx", size=120_000_000)
APP_SAFE = item("app1", label="Cache do Spotify", size=350_000_000)
APP_BLOCKED = item(
    "app2",
    label="Cache do Google Chrome",
    size=3_200_000_000,
    block="em uso por chrome (12 processos)",
)
IDLE_PROJECT = item("idle1", label="isengard", risk="review", size=3_600_000_000)
VOLUME = item("vol1", label="dados (projeto loja)", risk="review", reversibility="irreversible")
CRASH = item(
    "crash1",
    label="Relatórios de falha",
    risk="guided",
    reversibility="irreversible",
    command="sudo rm -f /var/crash/*.crash",
)


def sample_measurement() -> Measurement:
    return Measurement(
        disk=Disk(free=49_900_000_000, total=249_800_000_000),
        categories=(
            category("npm-cache", (NPM_FIRST, NPM_SECOND), title="Cache do npm"),
            category("caches-apps", (APP_BLOCKED, APP_SAFE), title="Caches de aplicativos"),
            category(
                "node-modules-parados",
                (IDLE_PROJECT,),
                title="Dependências de projetos parados",
                footnote="11 projetos com atividade nos últimos 30 dias ficaram de fora.",
            ),
            category(
                "docker-imagens",
                (),
                title="Docker: imagens e cache de build",
                problem="o serviço não está rodando: desktop-linux",
            ),
            category("docker-volumes", (VOLUME,), title="Docker: volumes e containers parados"),
            category("sistema", (CRASH,), title="Sistema (modo guiado)"),
        ),
    )


class FakeCore:
    def __init__(
        self,
        measurement: Measurement | None = None,
        clean_result: CleanResult | None = None,
        clean_error: str | None = None,
        warnings: tuple[str, ...] = (),
    ) -> None:
        self.measurement = measurement or sample_measurement()
        self.clean_result = clean_result
        self.clean_error = clean_error
        self.warnings = warnings
        self.measure_calls = 0
        self.planned: list[list[str]] = []
        self.cleaned: list[tuple[list[str], bool]] = []

    def measure(self) -> Measurement:
        self.measure_calls += 1
        return self.measurement

    def plan(self, ids: Sequence[str]) -> Plan:
        self.planned.append(list(ids))
        return Plan(count=len(ids), estimated=0, warnings=self.warnings)

    def clean(
        self,
        ids: Sequence[str],
        allow_irreversible: bool,
        on_progress: Callable[[Progress], None],
    ) -> CleanResult:
        self.cleaned.append((list(ids), allow_irreversible))
        on_progress(Progress(done=0, total=len(ids), current="Cache do npm"))
        if self.clean_error is not None:
            raise CoreError(self.clean_error)
        if self.clean_result is None:
            raise CoreError("FakeCore sem resultado configurado")
        return self.clean_result


def sample_result() -> CleanResult:
    return CleanResult(
        estimated=6_000_000_000,
        freed=2_000_000_000,
        trash=1_000_000_000,
        docker_vm=3_000_000_000,
        maybe_stuck=False,
        items=(),
        warnings=(),
    )
