from rich.text import Text
from textual import on
from textual.app import ComposeResult
from textual.binding import Binding
from textual.containers import Horizontal, Vertical, VerticalScroll
from textual.screen import ModalScreen
from textual.widgets import Button, Input, Label, Static

from jawa_tui.format import count_items, human_bytes
from jawa_tui.models import Item, Plan

CONFIRM_WORD = "apagar"


def needs_typed_confirmation(items: list[Item]) -> bool:
    return any(item.reversibility == "irreversible" for item in items)


class ConfirmScreen(ModalScreen[bool]):
    BINDINGS = [
        Binding("escape", "cancel", "Voltar"),
        Binding("enter", "confirm", "Limpar", show=False),
    ]

    def __init__(self, items: list[Item], plan: Plan) -> None:
        super().__init__()
        self.items = items
        self.plan = plan
        self.typed = needs_typed_confirmation(items)
        self.plain = ""

    def compose(self) -> ComposeResult:
        total = sum(item.bytes for item in self.items)
        notes = [f"Total estimado: {human_bytes(total)} em {count_items(len(self.items))}"]
        notes += [f"Aviso: {warning}" for warning in self.plan.warnings]
        if self.typed:
            notes.append(f"Tem item irreversível: digite {CONFIRM_WORD} para liberar a limpeza.")
        lines = [_item_line(item) for item in self.items]
        self.plain = "\n".join([*(line.plain for line in lines), *notes])
        with Vertical(id="confirm-box") as box:
            box.border_title = "Confirmar limpeza"
            with VerticalScroll(id="confirm-list"):
                for line in lines:
                    yield Static(line)
            for note in notes:
                yield Label(note, classes="confirm-note")
            if self.typed:
                yield Input(placeholder=CONFIRM_WORD, id="confirm-word")
            with Horizontal(id="confirm-buttons"):
                yield Button("Limpar", id="confirm", variant="error", compact=True)
                yield Button("Voltar", id="cancel", compact=True)

    def on_mount(self) -> None:
        self.query_one("#confirm", Button).disabled = self.typed
        if self.typed:
            self.query_one(Input).focus()

    @on(Input.Changed)
    def _word_changed(self, event: Input.Changed) -> None:
        self.query_one("#confirm", Button).disabled = event.value != CONFIRM_WORD

    @on(Input.Submitted)
    def _word_submitted(self) -> None:
        self.action_confirm()

    @on(Button.Pressed, "#confirm")
    def _confirm_pressed(self) -> None:
        self.action_confirm()

    @on(Button.Pressed, "#cancel")
    def _cancel_pressed(self) -> None:
        self.action_cancel()

    def action_confirm(self) -> None:
        if not self.query_one("#confirm", Button).disabled:
            self.dismiss(True)

    def action_cancel(self) -> None:
        self.dismiss(False)


def _item_line(item: Item) -> Text:
    line = Text(f"{human_bytes(item.bytes):>9}  ")
    line.append(item.label, style="bold")
    line.append(f"  {item.location}", style="grey62")
    if item.reversibility == "irreversible":
        line.append("  irreversível", style="bold red")
    return line
