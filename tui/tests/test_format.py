from jawa_tui.format import category_icon, human_bytes, size_bar, tag_label, used_percent


def test_human_bytes_matches_rust_output() -> None:
    assert human_bytes(0) == "0 B"
    assert human_bytes(999) == "999 B"
    assert human_bytes(1_500_000) == "1,5 MB"
    assert human_bytes(3_844_000_000) == "3,8 GB"
    assert human_bytes(999_950) == "1,0 MB"
    assert human_bytes(49_900_000_000) == "49,9 GB"


def test_size_bar_is_proportional_to_largest() -> None:
    assert size_bar(0, 100, width=8) == ""
    assert size_bar(100, 100, width=8) == "█" * 8
    assert size_bar(50, 100, width=8) == "████"
    assert size_bar(1, 100, width=8) == "▏"
    assert size_bar(5, 0, width=8) == ""


def test_tag_label_texts() -> None:
    assert tag_label("safe") == "● Seguro"
    assert tag_label("review") == "● Revisar"
    assert tag_label("sudo") == "🔒 Só com sudo"
    assert tag_label("unavailable") == "● Indisponível"
    assert tag_label("empty") == "· Nada a limpar"


def test_category_icon_falls_back_for_unknown_ids() -> None:
    assert category_icon("npm-cache") == "📦"
    assert category_icon("docker-volumes") == "🐳"
    assert category_icon("desconhecida") == "•"


def test_used_percent_handles_empty_disk() -> None:
    assert used_percent(49_900_000_000, 249_800_000_000) == 80
    assert used_percent(0, 0) == 0
