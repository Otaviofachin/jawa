from rich.text import Text
from textual.widgets import Static

from jawa_tui.format import category_icon, human_bytes, tag_label
from jawa_tui.models import Category, Item, Reversibility, Risk, Tag
from jawa_tui.widgets.labels import TAG_STYLES

REVERSIBILITY_TEXT: dict[Reversibility, str] = {
    "regenerable": "Volta sozinho quando você precisar.",
    "trash": "Vai para a Lixeira: dá para restaurar.",
    "irreversible": "Apagar é definitivo.",
}


class Details(Static):
    def __init__(self) -> None:
        super().__init__(id="details")
        self.border_title = "Detalhes"
        self.plain = ""

    def show_text(self, text: Text) -> None:
        self.plain = text.plain
        self.update(text)

    def show_message(self, message: str, style: str = "") -> None:
        self.show_text(Text(message, style=style))

    def show_category(self, category: Category) -> None:
        text = Text()
        text.append(f"{category_icon(category.id)} {category.title}\n\n", style="bold")
        text.append(f"{category.summary}\n\n")
        text.append(tag_label(category.tag), style=TAG_STYLES[category.tag])
        text.append(f"\n{category.explanation.recommendation}\n")
        if category.items:
            text.append(f"\n{len(category.items)} item(ns) · {human_bytes(category.total_bytes)}\n")
        if category.footnote:
            text.append(f"\n{category.footnote}\n", style="grey62")
        if category.problem:
            text.append(f"\n{category.problem}\n", style="yellow")
        self.show_text(text)

    def show_item(self, item: Item) -> None:
        text = Text()
        text.append(f"{item.label}\n", style="bold")
        text.append(f"{item.location}\n\n", style="grey62")
        text.append(f"{human_bytes(item.bytes)}  ")
        tag = _tag_for(item.risk)
        text.append(tag_label(tag), style=TAG_STYLES[tag])
        text.append(f"\n{REVERSIBILITY_TEXT[item.reversibility]}\n")
        if item.note:
            text.append(f"\n{item.note}\n")
        if item.block:
            text.append(f"\nBloqueado: {item.block}\n", style="red")
        if item.command:
            text.append("\nRode no terminal:\n", style="bold")
            text.append(f"{item.command}\n", style="dodger_blue1")
        self.show_text(text)


def _tag_for(risk: Risk) -> Tag:
    return "sudo" if risk == "guided" else risk
