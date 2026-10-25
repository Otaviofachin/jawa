from dataclasses import dataclass

from textual.binding import Binding
from textual.message import Message
from textual.widgets import Tree
from textual.widgets.tree import TreeNode

from jawa_tui.models import Category, Item, Measurement
from jawa_tui.widgets.labels import category_label, item_label


@dataclass(frozen=True)
class CategoryRef:
    category_id: str


@dataclass(frozen=True)
class ItemRef:
    item_id: str
    category_id: str


NodeRef = CategoryRef | ItemRef
Node = TreeNode[NodeRef]


class CategoryTree(Tree[NodeRef]):
    BINDINGS = [
        Binding("space", "toggle_mark", "Marca", show=False),
        Binding("j", "cursor_down", "Desce", show=False),
        Binding("k", "cursor_up", "Sobe", show=False),
    ]

    class MarksChanged(Message): ...

    def __init__(self) -> None:
        super().__init__("O que dá para limpar", id="tree")
        self.show_root = False
        self.border_title = "O que dá para limpar"
        self.marked: set[str] = set()
        self.categories: dict[str, Category] = {}
        self.items: dict[str, Item] = {}
        self._category_nodes: dict[str, Node] = {}
        self._item_nodes: dict[str, Node] = {}
        self._largest = 0

    def load(self, measurement: Measurement) -> None:
        self.clear()
        self.categories = {category.id: category for category in measurement.categories}
        self.items = {item.id: item for c in measurement.categories for item in c.items}
        self.marked = {item.id for item in self.items.values() if item.starts_selected}
        self._largest = max((c.total_bytes for c in measurement.categories), default=0)
        self._category_nodes.clear()
        self._item_nodes.clear()
        for category in measurement.categories:
            self._add_category(category)
        first = next((node for node in self._category_nodes.values() if node.children), None)
        if first is not None:
            first.expand()
            self.call_after_refresh(self.move_cursor, first)
        self.post_message(self.MarksChanged())

    def _add_category(self, category: Category) -> None:
        node = self.root.add(
            category_label(category, self._largest),
            data=CategoryRef(category.id),
            allow_expand=bool(category.items),
        )
        self._category_nodes[category.id] = node
        for item in category.items:
            leaf = node.add_leaf(
                item_label(item, item.id in self.marked), data=ItemRef(item.id, category.id)
            )
            self._item_nodes[item.id] = leaf

    def node_for_category(self, category_id: str) -> Node:
        return self._category_nodes[category_id]

    def node_for_item(self, item_id: str) -> Node:
        return self._item_nodes[item_id]

    def reveal(self, node: Node) -> None:
        parent = node.parent
        if parent is not None and parent is not self.root:
            parent.expand()
        self.call_after_refresh(self.move_cursor, node)

    def marked_items(self) -> list[Item]:
        return [item for item in self.items.values() if item.id in self.marked]

    def action_toggle_mark(self) -> None:
        node = self.cursor_node
        if node is None or node.data is None:
            return
        if isinstance(node.data, ItemRef):
            self._toggle_item(self.items[node.data.item_id])
        else:
            self._toggle_category(self.categories[node.data.category_id])

    def on_tree_node_selected(self, event: Tree.NodeSelected[NodeRef]) -> None:
        if isinstance(event.node.data, ItemRef):
            self._toggle_item(self.items[event.node.data.item_id])

    def _toggle_item(self, item: Item) -> None:
        if not item.selectable:
            return
        self.marked ^= {item.id}
        self._refresh_item(item)
        self.post_message(self.MarksChanged())

    def _toggle_category(self, category: Category) -> None:
        selectable = {item.id for item in category.items if item.selectable}
        if not selectable:
            return
        if selectable <= self.marked:
            self.marked -= selectable
        else:
            self.marked |= selectable
        for item in category.items:
            self._refresh_item(item)
        self.post_message(self.MarksChanged())

    def _refresh_item(self, item: Item) -> None:
        self._item_nodes[item.id].set_label(item_label(item, item.id in self.marked))
