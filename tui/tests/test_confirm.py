from textual.pilot import Pilot
from textual.widgets import Button, Input

from jawa_tui.app import JawaApp
from jawa_tui.screens.confirm import ConfirmScreen
from jawa_tui.screens.result import ResultScreen
from tests.fakes import NPM_FIRST, VOLUME, FakeCore, sample_result
from tests.test_app import WIDE, goto, ready


async def open_confirm(app: JawaApp, pilot: Pilot[None], *, irreversible: bool) -> None:
    tree = await ready(app, pilot)
    if irreversible:
        await goto(pilot, tree, tree.node_for_item(VOLUME.id))
        await pilot.press("space")
    await pilot.press("l")
    await app.workers.wait_for_complete()
    await pilot.pause()
    assert isinstance(app.screen, ConfirmScreen)


def confirm_text(app: JawaApp) -> str:
    screen = app.screen
    assert isinstance(screen, ConfirmScreen)
    return screen.plain


async def test_confirm_enter_without_typed_word_does_nothing() -> None:
    core = FakeCore(clean_result=sample_result())
    app = JawaApp(core)
    async with app.run_test(size=WIDE) as pilot:
        await open_confirm(app, pilot, irreversible=True)
        assert "apagar" in confirm_text(app)
        await pilot.press("enter")
        await pilot.pause()
        assert isinstance(app.screen, ConfirmScreen)
        assert core.cleaned == []


async def test_confirm_rejects_word_with_spaces() -> None:
    app = JawaApp(FakeCore(clean_result=sample_result()))
    async with app.run_test(size=WIDE) as pilot:
        await open_confirm(app, pilot, irreversible=True)
        app.screen.query_one(Input).value = " apagar "
        await pilot.pause()
        assert app.screen.query_one("#confirm", Button).disabled


async def test_confirm_typed_word_cleans_with_irreversible_flag() -> None:
    core = FakeCore(clean_result=sample_result())
    app = JawaApp(core)
    async with app.run_test(size=WIDE) as pilot:
        await open_confirm(app, pilot, irreversible=True)
        await pilot.press(*"apagar")
        await pilot.press("enter")
        await app.workers.wait_for_complete()
        await pilot.pause()
        ids, allow_irreversible = core.cleaned[0]
        assert VOLUME.id in ids and NPM_FIRST.id in ids
        assert allow_irreversible
        assert isinstance(app.screen, ResultScreen)


async def test_confirm_without_irreversible_accepts_enter() -> None:
    core = FakeCore(clean_result=sample_result())
    app = JawaApp(core)
    async with app.run_test(size=WIDE) as pilot:
        await open_confirm(app, pilot, irreversible=False)
        assert "apagar" not in confirm_text(app)
        await pilot.press("enter")
        await app.workers.wait_for_complete()
        await pilot.pause()
        assert core.cleaned and core.cleaned[0][1] is False


async def test_confirm_shows_total_and_warnings() -> None:
    core = FakeCore(warnings=("O espaço só volta quando você esvaziar a Lixeira.",))
    app = JawaApp(core)
    async with app.run_test(size=WIDE) as pilot:
        await open_confirm(app, pilot, irreversible=False)
        text = confirm_text(app)
        assert "Total estimado: 2,5 GB em 3 itens" in text
        assert "esvaziar a Lixeira" in text
        await pilot.press("escape")
        await pilot.pause()
        assert not isinstance(app.screen, ConfirmScreen)
        assert core.cleaned == []
