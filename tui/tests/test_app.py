from textual.pilot import Pilot
from textual.widgets import Button

from jawa_tui.app import JawaApp
from jawa_tui.widgets.action_bar import ActionBar
from jawa_tui.widgets.category_tree import CategoryTree, Node
from jawa_tui.widgets.details import Details
from jawa_tui.widgets.disk_header import DiskHeader
from tests.fakes import (
    APP_BLOCKED,
    APP_SAFE,
    CRASH,
    IDLE_PROJECT,
    NPM_FIRST,
    NPM_SECOND,
    FakeCore,
)

WIDE = (120, 40)


async def ready(app: JawaApp, pilot: Pilot[None]) -> CategoryTree:
    await app.workers.wait_for_complete()
    await pilot.pause()
    return app.query_one(CategoryTree)


async def goto(pilot: Pilot[None], tree: CategoryTree, node: Node) -> None:
    tree.reveal(node)
    await pilot.pause()
    assert tree.cursor_node is node


def labels(tree: CategoryTree) -> list[str]:
    return [str(node.label) for node in tree.root.children]


async def test_main_screen_shows_disk_and_categories() -> None:
    app = JawaApp(FakeCore())
    async with app.run_test(size=WIDE) as pilot:
        tree = await ready(app, pilot)
        header = app.query_one(DiskHeader).plain
        assert "49,9 GB livres de 249,8 GB" in header
        assert "80% usado" in header
        rows = labels(tree)
        assert any("Cache do npm" in row and "● Seguro" in row for row in rows), rows
        assert any("Sistema" in row and "🔒 Só com sudo" in row for row in rows), rows
        assert any("Projetos parados" in row and "● Revisar" in row for row in rows), rows


async def test_tree_starts_with_only_safe_items_checked() -> None:
    app = JawaApp(FakeCore())
    async with app.run_test(size=WIDE) as pilot:
        tree = await ready(app, pilot)
        assert tree.marked == {NPM_FIRST.id, NPM_SECOND.id, APP_SAFE.id}
        assert IDLE_PROJECT.id not in tree.marked


async def test_tree_refuses_to_check_guided_and_blocked_items() -> None:
    app = JawaApp(FakeCore())
    async with app.run_test(size=WIDE) as pilot:
        tree = await ready(app, pilot)
        for blocked in (CRASH, APP_BLOCKED):
            await goto(pilot, tree, tree.node_for_item(blocked.id))
            await pilot.press("space")
            assert blocked.id not in tree.marked
        details = app.query_one(Details).plain
        assert "em uso por chrome (12 processos)" in details


async def test_tree_category_toggle_marks_selectable_children() -> None:
    app = JawaApp(FakeCore())
    async with app.run_test(size=WIDE) as pilot:
        tree = await ready(app, pilot)
        await goto(pilot, tree, tree.node_for_category("npm-cache"))
        await pilot.press("space")
        assert not {NPM_FIRST.id, NPM_SECOND.id} & tree.marked
        await pilot.press("space")
        assert {NPM_FIRST.id, NPM_SECOND.id} <= tree.marked
        await goto(pilot, tree, tree.node_for_category("caches-apps"))
        await pilot.press("space")
        await pilot.press("space")
        assert APP_SAFE.id in tree.marked
        assert APP_BLOCKED.id not in tree.marked


async def test_tree_enter_on_item_toggles_mark() -> None:
    app = JawaApp(FakeCore())
    async with app.run_test(size=WIDE) as pilot:
        tree = await ready(app, pilot)
        await goto(pilot, tree, tree.node_for_item(IDLE_PROJECT.id))
        await pilot.press("enter")
        assert IDLE_PROJECT.id in tree.marked


async def test_details_panel_follows_cursor() -> None:
    app = JawaApp(FakeCore())
    async with app.run_test(size=WIDE) as pilot:
        tree = await ready(app, pilot)
        await goto(pilot, tree, tree.node_for_category("node-modules-parados"))
        await pilot.pause()
        details = app.query_one(Details).plain
        assert "Resumo de node-modules-parados." in details
        assert "11 projetos com atividade" in details
        await goto(pilot, tree, tree.node_for_item(CRASH.id))
        await pilot.pause()
        details = app.query_one(Details).plain
        assert "sudo rm -f /var/crash/*.crash" in details
        assert "~/crash1" in details


async def test_tree_shows_reason_for_unavailable_category() -> None:
    app = JawaApp(FakeCore())
    async with app.run_test(size=WIDE) as pilot:
        tree = await ready(app, pilot)
        docker = tree.node_for_category("docker-imagens")
        assert "● Indisponível" in str(docker.label)
        await goto(pilot, tree, docker)
        await pilot.press("space")
        await pilot.pause()
        assert "o serviço não está rodando: desktop-linux" in app.query_one(Details).plain


async def test_main_screen_fits_80x24() -> None:
    app = JawaApp(FakeCore())
    async with app.run_test(size=(80, 24)) as pilot:
        await ready(app, pilot)
        screen = app.screen.region
        for widget in (
            app.query_one(DiskHeader),
            app.query_one(CategoryTree),
            app.query_one(ActionBar),
            app.query_one("#clean", Button),
            app.query_one("#help", Button),
        ):
            region = widget.region
            assert region.height > 0 and region.width > 0, widget
            assert screen.contains_region(region), (widget, region)


async def test_app_explains_missing_binary() -> None:
    app = JawaApp(None)
    async with app.run_test(size=WIDE) as pilot:
        await pilot.pause()
        details = app.query_one(Details).plain
        assert "Não encontrei o programa jawa" in details
        assert app.query_one("#clean", Button).disabled


async def test_action_bar_counter_follows_marks() -> None:
    app = JawaApp(FakeCore())
    async with app.run_test(size=WIDE) as pilot:
        tree = await ready(app, pilot)
        bar = app.query_one(ActionBar)
        assert bar.plain.startswith("☑ 3 itens · 2,5 GB"), bar.plain
        await goto(pilot, tree, tree.node_for_item(NPM_FIRST.id))
        await pilot.press("space")
        assert bar.plain.startswith("☑ 2 itens · 470,0 MB"), bar.plain
        assert not app.query_one("#clean", Button).disabled


async def test_remeasure_key_measures_again() -> None:
    core = FakeCore()
    app = JawaApp(core)
    async with app.run_test(size=WIDE) as pilot:
        await ready(app, pilot)
        await pilot.press("m")
        await ready(app, pilot)
        assert core.measure_calls == 2


async def test_clean_flow_shows_result_and_remeasures() -> None:
    from jawa_tui.screens.result import ResultScreen
    from tests.fakes import sample_result

    core = FakeCore(clean_result=sample_result())
    app = JawaApp(core)
    async with app.run_test(size=WIDE) as pilot:
        await ready(app, pilot)
        await pilot.press("l")
        await app.workers.wait_for_complete()
        await pilot.pause()
        await pilot.press("enter")
        await app.workers.wait_for_complete()
        await pilot.pause()
        assert isinstance(app.screen, ResultScreen)
        text = app.screen.plain
        assert "Liberado no disco: 2,0 GB" in text
        assert "Na Lixeira: 1,0 GB" in text
        assert "Na VM do Docker Desktop: 3,0 GB" in text
        assert "desktop-reclaim-space" in text
        await pilot.press("escape")
        await ready(app, pilot)
        assert core.measure_calls == 2


async def test_clean_failure_focuses_remeasure_button() -> None:
    message = "a medição venceu (31 min): rode `jawa medir` de novo."
    app = JawaApp(FakeCore(clean_error=message))
    async with app.run_test(size=WIDE) as pilot:
        await ready(app, pilot)
        await pilot.press("l")
        await app.workers.wait_for_complete()
        await pilot.pause()
        await pilot.press("enter")
        await app.workers.wait_for_complete()
        await pilot.pause()
        assert message in app.query_one(Details).plain
        assert app.focused is app.query_one("#remeasure", Button)


async def test_help_screen_lists_keys_and_tags() -> None:
    from jawa_tui.screens.help import HelpScreen

    app = JawaApp(FakeCore())
    async with app.run_test(size=WIDE) as pilot:
        await ready(app, pilot)
        await pilot.press("question_mark")
        await pilot.pause()
        assert isinstance(app.screen, HelpScreen)
        assert "Espaço" in app.screen.plain
        assert "🔒 Só com sudo" in app.screen.plain
        await pilot.press("escape")
        await pilot.pause()
        assert not isinstance(app.screen, HelpScreen)
