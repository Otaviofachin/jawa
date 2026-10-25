from rich.cells import cell_len
from rich.text import Text

from jawa_tui.format import category_icon, human_bytes, short_title, size_bar, tag_label
from jawa_tui.models import Category, Item, Tag

TITLE_WIDTH = 25
SIZE_WIDTH = 9
BAR_WIDTH = 8
ITEM_LABEL_WIDTH = TITLE_WIDTH - 3
NO_ARROW_PAD = "  "

TAG_STYLES: dict[Tag, str] = {
    "safe": "bold green",
    "review": "bold yellow",
    "sudo": "bold dodger_blue1",
    "unavailable": "grey50",
    "empty": "grey50",
}

MARK_CHECKED = "☑"
MARK_UNCHECKED = "☐"
MARK_GUIDED = "🔒"
MARK_BLOCKED = "⛔"


def fit(text: str, width: int) -> str:
    if cell_len(text) > width:
        while cell_len(text + "…") > width:
            text = text[:-1]
        return text + "…"
    return text + " " * (width - cell_len(text))


def category_label(category: Category, largest: int) -> Text:
    name = short_title(category.id, category.title)
    pad = "" if category.items else NO_ARROW_PAD
    title = pad + fit(f"{category_icon(category.id)} {name}", TITLE_WIDTH)
    size = (
        human_bytes(category.total_bytes).rjust(SIZE_WIDTH) if category.items else " " * SIZE_WIDTH
    )
    bar = fit(size_bar(category.total_bytes, largest, BAR_WIDTH), BAR_WIDTH)
    label = Text(title)
    label.append(f" {size} ")
    label.append(bar, style="bright_blue")
    label.append(" ")
    label.append(tag_label(category.tag), style=TAG_STYLES[category.tag])
    return label


def item_mark(item: Item, marked: bool) -> str:
    if item.risk == "guided":
        return MARK_GUIDED
    if item.block is not None:
        return MARK_BLOCKED
    return MARK_CHECKED if marked else MARK_UNCHECKED


def item_label(item: Item, marked: bool) -> Text:
    style = "" if item.selectable else "grey50"
    label = Text(f"{item_mark(item, marked)} ", style=style)
    label.append(fit(item.label, ITEM_LABEL_WIDTH), style=style)
    label.append(human_bytes(item.bytes).rjust(SIZE_WIDTH), style=style)
    return label
