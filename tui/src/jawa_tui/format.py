from jawa_tui.models import Tag

UNITS = ("B", "kB", "MB", "GB", "TB")
THOUSAND = 1000
BAR_EIGHTHS = " ▏▎▍▌▋▊▉"
FULL_BLOCK = "█"

TAG_LABELS: dict[Tag, str] = {
    "safe": "● Seguro",
    "review": "● Revisar",
    "sudo": "🔒 Só com sudo",
    "unavailable": "● Indisponível",
    "empty": "· Nada a limpar",
}

CATEGORY_ICONS = {
    "npm-cache": "📦",
    "caches-ferramentas": "🧰",
    "caches-apps": "🧩",
    "saidas-build": "🔨",
    "rust-target": "🦀",
    "node-modules-parados": "💤",
    "nvm-versoes": "🟩",
    "docker-imagens": "🐳",
    "docker-volumes": "🐳",
    "docker-desktop-disco": "🐳",
    "downloads": "📥",
    "sistema": "💻",
}
UNKNOWN_ICON = "•"

SHORT_TITLES = {
    "npm-cache": "Cache do npm",
    "caches-ferramentas": "Caches de ferramentas",
    "caches-apps": "Caches de aplicativos",
    "saidas-build": "Saídas de build",
    "rust-target": "Builds do Rust",
    "node-modules-parados": "Projetos parados",
    "nvm-versoes": "Versões do Node",
    "docker-imagens": "Docker: imagens",
    "docker-volumes": "Docker: volumes",
    "docker-desktop-disco": "Docker Desktop",
    "downloads": "Downloads",
    "sistema": "Sistema (guiado)",
}


def human_bytes(size: int) -> str:
    if size < THOUSAND:
        return f"{size} B"
    unit = 1
    while unit < len(UNITS) - 1 and _rounded_tenths(size, unit) >= THOUSAND * 10:
        unit += 1
    tenths = _rounded_tenths(size, unit)
    return f"{tenths // 10},{tenths % 10} {UNITS[unit]}"


def _rounded_tenths(size: int, unit: int) -> int:
    divisor = int(THOUSAND**unit)
    return (size * 10 + divisor // 2) // divisor


def size_bar(size: int, largest: int, width: int = 8) -> str:
    if size <= 0 or largest <= 0:
        return ""
    eighths = max(1, size * width * 8 // largest)
    full, rest = divmod(min(eighths, width * 8), 8)
    return FULL_BLOCK * full + (BAR_EIGHTHS[rest] if rest else "")


def tag_label(tag: Tag) -> str:
    return TAG_LABELS[tag]


def category_icon(category_id: str) -> str:
    return CATEGORY_ICONS.get(category_id, UNKNOWN_ICON)


def used_percent(free: int, total: int) -> int:
    if total <= 0:
        return 0
    return round((total - free) * 100 / total)


def short_title(category_id: str, title: str) -> str:
    return SHORT_TITLES.get(category_id, title)
