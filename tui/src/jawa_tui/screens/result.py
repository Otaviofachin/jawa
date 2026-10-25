from rich.text import Text
from textual.app import ComposeResult
from textual.binding import Binding
from textual.containers import Vertical, VerticalScroll
from textual.screen import ModalScreen
from textual.widgets import Button, Static

from jawa_tui.format import human_bytes
from jawa_tui.models import CleanResult, Outcome

RECLAIM_COMMAND = "docker run --privileged --pid=host docker/desktop-reclaim-space"
SNAPSHOT_HINT = "Parte do espaço pode estar presa em snapshots (Btrfs/Timeshift)."
OUTCOME_TEXT: dict[Outcome, str] = {"done": "feito", "skipped": "pulado", "failed": "falhou"}


def result_text(result: CleanResult) -> Text:
    text = Text()
    text.append(f"Liberado no disco: {human_bytes(result.freed)}\n", style="bold green")
    text.append(f"Estimado: {human_bytes(result.estimated)}\n\n", style="grey62")
    if result.trash:
        text.append(f"Na Lixeira: {human_bytes(result.trash)}\n", style="bold")
        text.append("Volta ao disco quando você esvaziar a Lixeira.\n\n")
    if result.docker_vm:
        text.append(f"Na VM do Docker Desktop: {human_bytes(result.docker_vm)}\n", style="bold")
        text.append("Com o Docker Desktop ligado, recupere com:\n")
        text.append(f"{RECLAIM_COMMAND}\n\n", style="dodger_blue1")
    if result.maybe_stuck:
        text.append(f"{SNAPSHOT_HINT}\n\n", style="yellow")
    done = sum(1 for item in result.items if item.outcome == "done")
    text.append(f"Feitos: {done} de {len(result.items)}\n")
    for item in result.items:
        if item.outcome != "done":
            outcome = OUTCOME_TEXT[item.outcome]
            text.append(f"• {item.label}: {outcome}, {item.detail or ''}\n", style="yellow")
    for warning in result.warnings:
        text.append(f"Aviso: {warning}\n", style="yellow")
    return text


class ResultScreen(ModalScreen[None]):
    BINDINGS = [
        Binding("escape", "close", "Fechar"),
        Binding("enter", "close", "Fechar", show=False),
    ]

    def __init__(self, result: CleanResult) -> None:
        super().__init__()
        self.text = result_text(result)
        self.plain = self.text.plain

    def compose(self) -> ComposeResult:
        with Vertical(id="result-box") as box:
            box.border_title = "Resultado"
            with VerticalScroll():
                yield Static(self.text)
            yield Button("Fechar e medir de novo", id="close", variant="primary", compact=True)

    def on_button_pressed(self) -> None:
        self.action_close()

    def action_close(self) -> None:
        self.dismiss(None)
