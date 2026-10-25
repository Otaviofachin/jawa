from textual.app import ComposeResult
from textual.containers import Horizontal
from textual.widgets import Button, Label

from jawa_tui.format import count_items, human_bytes


class ActionBar(Horizontal):
    def __init__(self) -> None:
        super().__init__(id="actions")
        self.plain = ""

    def compose(self) -> ComposeResult:
        yield Label("", id="counter")
        yield Button("Limpar marcados", id="clean", variant="primary", compact=True)
        yield Button("Medir de novo", id="remeasure", compact=True)
        yield Button("Ajuda", id="help", compact=True)

    def show_selection(self, count: int, size: int) -> None:
        self.plain = f"☑ {count_items(count)} · {human_bytes(size)}"
        self.query_one("#counter", Label).update(self.plain)
        self.query_one("#clean", Button).disabled = count == 0

    def disable_cleaning(self) -> None:
        self.query_one("#clean", Button).disabled = True
