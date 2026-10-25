from textual.app import ComposeResult
from textual.containers import Horizontal
from textual.widgets import Label, ProgressBar

from jawa_tui.format import human_bytes, used_percent
from jawa_tui.models import Disk

MEASURING = "Medindo…"


class DiskHeader(Horizontal):
    def __init__(self) -> None:
        super().__init__(id="disk")
        self.border_title = "🪐 Jawa"
        self.border_subtitle = MEASURING
        self.plain = MEASURING

    def compose(self) -> ComposeResult:
        yield ProgressBar(total=100, show_percentage=False, show_eta=False, id="disk-bar")
        yield Label("", id="disk-used")

    def measuring(self) -> None:
        self.border_subtitle = MEASURING
        self.plain = MEASURING

    def show(self, disk: Disk) -> None:
        percent = used_percent(disk.free, disk.total)
        free = f"{human_bytes(disk.free)} livres de {human_bytes(disk.total)}"
        used = f"{percent}% usado"
        self.border_subtitle = free
        self.query_one("#disk-bar", ProgressBar).update(progress=percent)
        self.query_one("#disk-used", Label).update(used)
        self.plain = f"{free} · {used}"
