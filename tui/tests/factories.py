from jawa_tui.models import Category, Explanation, Item, Reversibility, Risk

EXPLANATION = Explanation(
    what_it_is="O que é.",
    why_it_grows="Por que cresce.",
    effect="Efeito.",
    reversibility="Volta sozinho.",
    recreate_cost="Pouco.",
    recommendation="Pode limpar.",
)


def item(
    item_id: str,
    *,
    risk: Risk = "safe",
    size: int = 2_000_000_000,
    reversibility: Reversibility = "regenerable",
    block: str | None = None,
    command: str | None = None,
    label: str | None = None,
) -> Item:
    return Item(
        id=item_id.ljust(10, "0")[:10],
        label=label or f"item {item_id}",
        location=f"~/{item_id}",
        bytes=size,
        risk=risk,
        reversibility=reversibility,
        note=None,
        block=block,
        command=command,
    )


def category(
    category_id: str,
    items: tuple[Item, ...],
    *,
    title: str | None = None,
    problem: str | None = None,
    footnote: str | None = None,
) -> Category:
    return Category(
        id=category_id,
        title=title or category_id,
        summary=f"Resumo de {category_id}.",
        explanation=EXPLANATION,
        footnote=footnote,
        problem=problem,
        items=items,
    )
