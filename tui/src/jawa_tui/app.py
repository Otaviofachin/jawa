from textual import on, work
from textual.app import App, ComposeResult
from textual.binding import Binding
from textual.containers import Horizontal
from textual.widgets import Button, Tree

from jawa_tui.core import Core, CoreError, JawaCli, find_binary
from jawa_tui.models import CleanResult, ContractError, Item, Measurement, Plan, Progress
from jawa_tui.screens.confirm import ConfirmScreen, needs_typed_confirmation
from jawa_tui.screens.help import HelpScreen
from jawa_tui.screens.progress import ProgressScreen
from jawa_tui.screens.result import ResultScreen
from jawa_tui.widgets.action_bar import ActionBar
from jawa_tui.widgets.category_tree import CategoryTree, ItemRef, NodeRef
from jawa_tui.widgets.details import Details
from jawa_tui.widgets.disk_header import DiskHeader

MISSING_BINARY = (
    "Não encontrei o programa jawa.\n\n"
    "Instale o Jawa (pacote .deb ou cargo install) ou defina JAWA_BIN "
    "com o caminho do binário."
)

NOTHING_MARKED = "Marque pelo menos um item com Espaço antes de limpar."


class JawaApp(App[None]):
    CSS_PATH = "jawa.tcss"
    TITLE = "Jawa"
    BINDINGS = [
        Binding("l", "clean", "Limpar"),
        Binding("m", "remeasure", "Medir de novo"),
        Binding("question_mark", "help", "Ajuda"),
        Binding("q", "quit", "Sair"),
    ]

    def __init__(self, core: Core | None) -> None:
        super().__init__()
        self.core = core

    def compose(self) -> ComposeResult:
        yield DiskHeader()
        with Horizontal(id="main"):
            yield CategoryTree()
            yield Details()
        yield ActionBar()

    def on_mount(self) -> None:
        self.query_one(CategoryTree).focus()
        if self.core is None:
            self.query_one(Details).show_message(MISSING_BINARY, style="yellow")
            self.query_one(ActionBar).disable_cleaning()
            return
        self.action_remeasure()

    def action_remeasure(self) -> None:
        if self.core is None or not self._on_main_screen():
            return
        self.query_one(DiskHeader).measuring()
        self.query_one(CategoryTree).loading = True
        self.query_one(ActionBar).disable_cleaning()
        self._measure(self.core)

    @work(thread=True, exclusive=True, group="measure")
    def _measure(self, core: Core) -> None:
        try:
            measurement = core.measure()
        except (CoreError, ContractError) as error:
            self.call_from_thread(self._show_failure, str(error))
            return
        self.call_from_thread(self._show_measurement, measurement)

    def _show_measurement(self, measurement: Measurement) -> None:
        tree = self.query_one(CategoryTree)
        tree.load(measurement)
        tree.loading = False
        tree.focus()
        self.query_one(DiskHeader).show(measurement.disk)
        self._show_details(tree.cursor_node.data if tree.cursor_node else None)

    def _show_failure(self, message: str) -> None:
        self.query_one(CategoryTree).loading = False
        self.query_one(Details).show_message(message, style="red")
        self.notify(message, title="Não deu certo", severity="error")

    @on(CategoryTree.MarksChanged)
    def _update_counter(self) -> None:
        marked = self.query_one(CategoryTree).marked_items()
        self.query_one(ActionBar).show_selection(len(marked), sum(i.bytes for i in marked))

    @on(Tree.NodeHighlighted)
    def _follow_cursor(self, event: Tree.NodeHighlighted[NodeRef]) -> None:
        self._show_details(event.node.data)

    def _show_details(self, ref: NodeRef | None) -> None:
        tree = self.query_one(CategoryTree)
        details = self.query_one(Details)
        if isinstance(ref, ItemRef):
            details.show_item(tree.items[ref.item_id])
        elif ref is not None:
            details.show_category(tree.categories[ref.category_id])

    @on(Button.Pressed, "#remeasure")
    def _remeasure_pressed(self) -> None:
        self.action_remeasure()

    @on(Button.Pressed, "#clean")
    def _clean_pressed(self) -> None:
        self.action_clean()

    @on(Button.Pressed, "#help")
    def _help_pressed(self) -> None:
        self.action_help()

    def _on_main_screen(self) -> bool:
        return self.screen is self.screen_stack[0]

    def action_clean(self) -> None:
        items = self.query_one(CategoryTree).marked_items()
        if self.core is None or not self._on_main_screen():
            return
        if not items:
            self.notify(NOTHING_MARKED, severity="warning")
            return
        self._plan(self.core, items)

    @work(thread=True, exclusive=True, group="clean")
    def _plan(self, core: Core, items: list[Item]) -> None:
        try:
            plan = core.plan([item.id for item in items])
        except (CoreError, ContractError) as error:
            self.call_from_thread(self._clean_failed, str(error))
            return
        self.call_from_thread(self._confirm, core, items, plan)

    def _confirm(self, core: Core, items: list[Item], plan: Plan) -> None:
        def on_answer(confirmed: bool | None) -> None:
            if confirmed:
                self._start_clean(core, items)

        self.push_screen(ConfirmScreen(items, plan), on_answer)

    def _start_clean(self, core: Core, items: list[Item]) -> None:
        progress = ProgressScreen()
        self.push_screen(progress)
        ids = [item.id for item in items]
        self._clean(core, ids, needs_typed_confirmation(items), progress)

    @work(thread=True, exclusive=True, group="clean")
    def _clean(self, core: Core, ids: list[str], allow: bool, progress: ProgressScreen) -> None:
        def on_progress(step: Progress) -> None:
            self.call_from_thread(progress.show, step)

        try:
            result = core.clean(ids, allow, on_progress)
        except (CoreError, ContractError) as error:
            self.call_from_thread(self._clean_failed, str(error))
            return
        self.call_from_thread(self._show_result, result)

    def _show_result(self, result: CleanResult) -> None:
        if isinstance(self.screen, ProgressScreen):
            self.pop_screen()
        self.push_screen(ResultScreen(result), lambda _: self.action_remeasure())

    def _clean_failed(self, message: str) -> None:
        if isinstance(self.screen, ProgressScreen):
            self.pop_screen()
        self._show_failure(message)
        self.query_one("#remeasure", Button).focus()

    def action_help(self) -> None:
        if self._on_main_screen():
            self.push_screen(HelpScreen())


def main() -> None:
    binary = find_binary()
    JawaApp(JawaCli(binary) if binary else None).run()
