# jawa-tui

Interface de terminal do [Jawa](../README.md), feita com [Textual](https://textual.textualize.io). Ela só desenha: medir e limpar ficam com o binário `jawa` (núcleo em Rust, com todas as travas de segurança), chamado pelos subcomandos com `--json`.

```bash
uv sync
uv run pytest
uv run mypy src tests
uv run ruff check
```
