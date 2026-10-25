from textual.app import ComposeResult
from textual.containers import Vertical
from textual.screen import ModalScreen
from textual.widgets import Label, ProgressBar

from jawa_tui.models import Progress

WAIT_NOTE = "Limpando… não feche o terminal."


class ProgressScreen(ModalScreen[None]):
    def compose(self) -> ComposeResult:
        with Vertical(id="progress-box") as box:
            box.border_title = "Limpando"
            yield ProgressBar(id="clean-bar", show_eta=False)
            yield Label("", id="clean-current")
            yield Label(WAIT_NOTE, classes="confirm-note")

    def show(self, progress: Progress) -> None:
        bar = self.query_one("#clean-bar", ProgressBar)
        bar.update(total=max(progress.total, 1), progress=progress.done)
        self.query_one("#clean-current", Label).update(progress.current or "")
