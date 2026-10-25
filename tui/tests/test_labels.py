from rich.cells import cell_len

from jawa_tui.widgets.labels import category_label
from tests.factories import category, item


def tag_column(label: str) -> int:
    start = next(index for index, char in enumerate(label) if char in "●·🔒")
    return cell_len(label[:start])


def test_category_label_keeps_long_titles_without_items() -> None:
    empty = str(category_label(category("caches-ferramentas", ()), 10))
    assert "Caches de ferramentas" in empty
    assert "Caches de aplicativos" in str(category_label(category("caches-apps", ()), 10))


def test_category_label_aligns_tag_with_and_without_items() -> None:
    empty = str(category_label(category("caches-ferramentas", ()), 10))
    full = str(category_label(category("caches-ferramentas", (item("a"),)), 10))
    assert tag_column(empty) == tag_column(full) + 2
