# Jawa v0.2: TUI em Textual (layout da Opção 1)

> **Para agentes:** use superpowers:executing-plans (decisão: execução nativa, na sessão principal). Passos em checkbox (`- [ ]`).

**Objetivo:** substituir a cara da interface de terminal pelo layout da "Opção 1" (cabeçalho com o disco, árvore de categorias com etiqueta de risco, painel de detalhes e barra de ações), feita em Python com Textual, sem mexer em nenhuma regra de medição ou limpeza.

**Arquitetura:** o núcleo Rust continua sendo o único que mede e apaga (travas S1–S19). A interface Python roda o binário `jawa` pelos subcomandos escondidos com `--json` (`medir --json` e `limpar ... --json`) e só desenha. O `jawa` sem argumentos passa a abrir a interface nova quando o comando `jawa-tui` estiver instalado; sem ele, cai na TUI ratatui atual.

**Stack:** Rust (workspace atual) · Python ≥ 3.11 · Textual (versão travada no `uv.lock`) · uv (ambiente e dependências) · pytest + pytest-asyncio · mypy `--strict` · ruff.

**Origem:** decisões tomadas na conversa de 2026-10-25 (o usuário dispensou a especificação separada; este plano carrega as decisões).

## Layout aprovado (Opção 1, "exatamente como desenhou")

```
╭─ 🪐 Jawa ───────────────────────────────────── 49,9 GB livres de 249,8 GB ─╮
│ ███████████████████████████████████████████░░░░░░░░░░░  80% usado         │
╰────────────────────────────────────────────────────────────────────────────╯
╭─ O que dá para limpar ─────────────────────────╮╭─ Detalhes ──────────────╮
│ ▾ 📦 Cache do npm             2,4 GB  ██▌      ││ 📦 Cache do npm         │
│     ☑ _cacache                2,3 GB           ││                         │
│     ☑ _npx                  120,0 MB           ││ Cópia de cada pacote    │
│ ▸ 🧰 Caches de ferramentas    5,5 GB  █████    ││ baixado pelo npm.       │
│ ▸ 🏗  Saídas de build         11,5 GB  ██████▌  ││                         │
│ ▸ 💤 Projetos parados        19,7 GB  ████████ ││ ● Seguro                │
│ ▸ 🐳 Docker                  29,3 GB  ████████ ││ Volta sozinho no próximo│
│ ▸ 🖥  Sistema (guiado)         4,7 GB  ██▌      ││ npm install.            │
╰────────────────────────────────────────────────╯╰─────────────────────────╯
 ☑ 49 itens · 52,7 GB      [ Limpar marcados ]  [ Medir de novo ]  [ Ajuda ]
```

Acréscimo pedido pelo usuário: a etiqueta de risco à mostra em cada categoria, como nos cartões da Opção 3: `● Seguro` (verde), `● Revisar` (amarelo), `🔒 Só com sudo` (azul).

## Global Constraints

- Contrato JSON entre o Rust e o Python com chaves e valores em inglês (atualizado na revisão de 2026-10-25); `--simular --json` devolve `{"type":"plan","count":…,"estimated":…,"warnings":[…]}`.
- O Python **nunca** apaga, mede ou decide trava: só chama `jawa medir --json` e `jawa limpar <ids> --sim --json [--confirmar-irreversivel]`.
- Texto ao usuário em PT-BR com acentos; identificadores em inglês; números no formato do `human_bytes` (decimal, vírgula: `2,4 GB`).
- Python: tipagem completa, `mypy --strict` limpo, **sem `Any`** (o JSON entra como `object` e é estreitado com `isinstance`); `ruff check` limpo; funções com ~30 linhas no máximo.
- Rust: as regras atuais (fmt, clippy `-D warnings`, sem `unwrap`/`expect` fora de testes, testes herméticos com `HOME`/`XDG_*` em tempdir).
- Testes da interface nunca chamam o `jawa` real: usam um `FakeCore` ou um script falso no lugar do binário.
- Regras de seleção iguais às da TUI atual: `Seguro` sem bloqueio vem marcado; `Revisar` vem desmarcado; `Guiado` e bloqueados não podem ser marcados; com item irreversível marcado, a confirmação exige digitar exatamente `apagar`.
- Docker: sem mudança de comportamento nesta versão.

## Review Focus

1. Binário `jawa` ausente ou que falha → a interface mostra a mensagem de erro em PT-BR com o que fazer, sem traceback (teste em `test_app.py`).
2. Medição vencida (mais de 30 min com a tela aberta) → o `limpar` falha; a interface mostra a mensagem do Rust e sugere "Medir de novo" (teste em `test_app.py`).
3. Terminal pequeno (80×24) → cabeçalho, árvore e barra de ações continuam visíveis (teste com `size=(80, 24)`).
4. Categoria com problema (Docker parado, ferramenta ausente) ou vazia → aparece com o motivo em cinza, sem caixa de marcar e sem quebrar a soma (teste em `test_app.py`).
5. Item irreversível marcado → `Enter` sem digitar `apagar` não limpa; `" apagar "` com espaços também não (teste em `test_confirm.py`).

## Mapa de arquivos

| Arquivo | Responsabilidade |
|---|---|
| `crates/jawa/src/cli/json.rs` (novo) | Monta o JSON da medição e as linhas JSON da limpeza (`MeasureView`, `CleanEvent`) |
| `crates/jawa/src/cli/measure.rs` | `--json` passa a usar `json::measure_view` |
| `crates/jawa/src/cli/clean.rs` | flag `--json`: progresso e resultado em JSON Lines no stdout |
| `crates/jawa/src/cli/mod.rs` | `jawa` sem argumentos abre `jawa-tui` se existir, senão a TUI ratatui |
| `crates/jawa/tests/cli.rs` | testes do contrato JSON e do lançador |
| `tui/pyproject.toml`, `tui/uv.lock` | pacote `jawa-tui`, script `jawa-tui = "jawa_tui.app:main"` |
| `tui/src/jawa_tui/models.py` | dataclasses tipadas e o parse do JSON |
| `tui/src/jawa_tui/core.py` | `Core` (Protocol), `JawaCli` (chama o binário), `find_binary()` |
| `tui/src/jawa_tui/format.py` | `human_bytes`, barra de tamanho, etiqueta de risco, ícone por categoria |
| `tui/src/jawa_tui/widgets/disk_header.py` | cabeçalho com livre/total e barra de uso |
| `tui/src/jawa_tui/widgets/category_tree.py` | árvore com caixas de marcar, tamanho, barra e etiqueta |
| `tui/src/jawa_tui/widgets/details.py` | painel de detalhes do item/categoria sob o cursor |
| `tui/src/jawa_tui/widgets/action_bar.py` | contador de marcados e os três botões |
| `tui/src/jawa_tui/screens/confirm.py` | janela de confirmação (total, avisos, campo `apagar`) |
| `tui/src/jawa_tui/screens/progress.py` | janela de progresso da limpeza com cancelar |
| `tui/src/jawa_tui/screens/result.py` | janela de resultado (disco, Lixeira, VM do Docker, pulados) |
| `tui/src/jawa_tui/screens/help.py` | janela de ajuda com as teclas |
| `tui/src/jawa_tui/app.py`, `tui/src/jawa_tui/jawa.tcss` | `JawaApp`, teclas, workers e o estilo |
| `tui/tests/*.py` | testes de modelos, core, formato, app e confirmação |
| `.github/workflows/ci.yml`, `README.md` | job de Python no CI; instalação e uso da nova interface |

---

## Lote 1: contrato JSON e lançador (Rust)

### Task 1: `jawa medir --json` com tudo que a tela precisa

**Files:** Create `crates/jawa/src/cli/json.rs`; Modify `crates/jawa/src/cli/measure.rs`, `crates/jawa/src/cli/mod.rs`; Test `crates/jawa/tests/cli.rs`

**Interfaces:**
- Produces (formato do stdout, objeto único):
```json
{
  "version": 1,
  "disk": { "free": 49900000000, "total": 249800000000 },
  "categories": [
    {
      "id": "npm-cache",
      "title": "Cache do npm",
      "summary": "…",
      "explanation": { "what_it_is": "…", "why_it_grows": "…", "effect": "…",
                      "reversibility": "…", "recreate_cost": "…", "recommendation": "…" },
      "footnote": null,
      "problem": null,
      "items": [
        { "id": "a1b2c3d4e5", "label": "_cacache", "location": "~/.npm/_cacache",
          "bytes": 2300000000, "risk": "safe", "reversibility": "regenerable",
          "note": "…", "block": null, "command": null }
      ]
    }
  ]
}
```
- `risk` ∈ `safe | review | guided`; `reversibility` ∈ `regenerable | trash | irreversible`; `bloqueio` é o `Display` do `BlockReason` (já resumido por processo); `comando` é o `command_line` dos itens `Guide`, senão `null`; `problema` é o `Display` do `ScanProblem`. Todas as categorias do registro aparecem, na ordem do registro, mesmo vazias.
- `pub fn measure_view(scans: &[CategoryScan], disk: (u64, u64)) -> serde_json::Value` em `json.rs`.

- [ ] **Step 1: teste que falha** `measure_json_has_disk_categories_and_items`: no sandbox com o cache do npm semeado, `medir --json` → `versao == 1`; `disco.total >= disco.livre > 0`; `categorias.len() == 12`; a categoria `npm-cache` tem `titulo == "Cache do npm"`, `explicacao.o_que_e` não vazio e um item com `risco == "seguro"`, `reversibilidade == "regeneravel"` e `id` de 10 caracteres; a categoria `docker-imagens` tem `problema` não nulo (sem docker no PATH). Ajustar `measured_ids()` para ler `categorias[*].itens[*].id`.
- [ ] **Step 2:** `cargo test -p jawa --test cli measure_json` → FAIL.
- [ ] **Step 3:** implementar `measure_view`; o disco vem de `free_bytes`/`total_bytes` da home.
- [ ] **Step 4:** PASS, e todos os testes da CLI verdes.

### Task 2: `jawa limpar --json` em JSON Lines

**Files:** Modify `crates/jawa/src/cli/clean.rs`, `crates/jawa/src/cli/json.rs`; Test `crates/jawa/tests/cli.rs`

**Interfaces:**
- Flag nova `--json` (escondida como as demais). Com `--json`, nada de texto humano no stdout; cada evento é uma linha JSON, com flush a cada linha:
  - `{"type":"progress","done":0,"total":3,"current":"Cache do npm"}` (`current` pode ser `null` no fim)
  - `{"type":"result","estimated":…,"freed":…,"trash":…,"docker_vm":…,"maybe_stuck":false,"items":[{"id":"…","label":"…","outcome":"done|skipped|failed","detail":"…"}],"warnings":["…"]}`
- Erros (medição vencida, id desconhecido, lock ocupado, irreversível sem flag) continuam saindo com código 1 e mensagem PT-BR no stderr; nenhuma linha `resultado` é escrita.
- `--json` exige `--sim` (sem TTY não há pergunta); sem `--sim` → código 1 com a mensagem atual.
- `pub fn progress_line(progress: &Progress) -> String` e `pub fn result_line(report: &CleanReport) -> String` em `json.rs`.

- [ ] **Step 1: testes que falham:** `clean_json_streams_progress_then_result` (medir, `limpar <id> --sim --json` → ao menos uma linha `progresso`, a última linha é `resultado` com `itens[0].desfecho == "feito"`, a pasta sumiu); `clean_json_without_measure_fails_on_stderr` (código 1, stdout sem `resultado`, stderr com `jawa medir`).
- [ ] **Step 2:** FAIL. **Step 3:** implementar. **Step 4:** PASS.

### Task 3: `jawa` sem argumentos abre a interface nova quando instalada

**Files:** Modify `crates/jawa/src/cli/mod.rs`; Test `crates/jawa/tests/cli.rs`

**Interfaces:**
- Sem subcomando: procura `jawa-tui` no `PATH`; se achar, roda-o com `JAWA_BIN=<caminho do próprio executável>` no ambiente, herda o terminal e devolve o código de saída dele; se não achar, abre a TUI ratatui como hoje.

- [ ] **Step 1: testes que falham:** `no_args_launches_jawa_tui_when_installed` (script `jawa-tui` no `bin` do sandbox que grava `$JAWA_BIN` num arquivo e sai com 0 → `jawa` sai com 0 e o arquivo contém o caminho do binário testado); o teste atual "sem terminal" continua passando sem o script.
- [ ] **Step 2:** FAIL. **Step 3:** implementar com `which::which("jawa-tui")` e `std::env::current_exe()`. **Step 4:** PASS. Gate completo (`cargo fmt --check`, clippy `-D warnings`, `cargo test --all-features`).

---

## Lote 2: interface Textual (Python)

**Pré-requisito:** `uv` instalado (`curl -LsSf https://astral.sh/uv/install.sh | sh`, rodado pelo usuário).

### Task 4: projeto Python, modelos e cliente do núcleo

**Files:** Create `tui/pyproject.toml`, `tui/src/jawa_tui/{__init__,models,core,format}.py`, `tui/tests/{test_models,test_core,test_format}.py`, `tui/tests/fixtures/measurement.json`

**Interfaces:**
- `models.py`: `@dataclass(frozen=True)` `Disk(free: int, total: int)`, `Explanation(...)` (6 campos), `Item(id, label, location, bytes, risk: Risk, reversibility: Reversibility, note: str | None, block: str | None, command: str | None)`, `Category(id, title, summary, explanation, footnote, problem, items: tuple[Item, ...])`, `Measurement(disk, categories)`, `Progress(done, total, current)`, `ItemOutcome(id, label, outcome: Literal["feito","pulado","falhou"], detail)`, `CleanResult(estimated, freed, trash, docker_vm, maybe_stuck, items, warnings)`. `Risk = Literal["seguro","revisar","guiado"]`, `Reversibility = Literal["regeneravel","lixeira","irreversivel"]`. `parse_measurement(raw: object) -> Measurement` e `parse_event(raw: object) -> Progress | CleanResult`, com `ContractError` em PT-BR para JSON inesperado. Propriedades: `Item.selectable` (não guiado e sem bloqueio), `Item.starts_selected` (seguro e selecionável), `Category.total_bytes`, `Category.tag -> Literal["seguro","revisar","sudo","problema","vazio"]` (problema → `problema`; sem itens → `vazio`; todos guiados → `sudo`; algum revisar → `revisar`; senão `seguro`).
- `core.py`: `class Core(Protocol)` com `measure() -> Measurement` e `clean(ids: Sequence[str], allow_irreversible: bool, on_progress: Callable[[Progress], None]) -> CleanResult`; `class JawaCli(Core)` roda o binário com `subprocess`, lê JSON Lines do stdout e, em erro, levanta `CoreError(mensagem do stderr)`; `find_binary() -> Path | None` (ordem: `$JAWA_BIN`, `which("jawa")`).
- `format.py`: `human_bytes(int) -> str` (mesma saída do Rust: `0 B`, `1,5 MB`, `3,8 GB`), `size_bar(bytes, largest, width=8) -> str` com blocos `▏▎▍▌▋▊▉█`, `tag_label(tag) -> str` (`● Seguro`, `● Revisar`, `🔒 Só com sudo`, `● Indisponível`, `· Nada a limpar`), `category_icon(id) -> str` (📦 npm, 🧰 ferramentas, 🧩 apps, 🏗 builds, 🦀 rust, 💤 projetos parados, 🟩 nvm, 🐳 docker ×3, 📥 downloads, 🖥 sistema).

- [ ] **Step 1:** `uv init --package tui` ajustado; `uv add textual`; `uv add --dev pytest pytest-asyncio mypy ruff`.
- [ ] **Step 2: testes que falham:** `test_parse_fixture` (fixture real gerada pelo Task 1 → 12 categorias, tipos certos); `test_rejects_unexpected_json` (`ContractError`); `test_category_tag_rules` (os 5 casos); `test_selection_rules`; `test_human_bytes_matches_rust` (`0 → "0 B"`, `1_500_000 → "1,5 MB"`, `3_844_000_000 → "3,8 GB"`); `test_size_bar`; `test_jawa_cli_reads_json_lines` (binário falso: script que imprime 2 progressos e 1 resultado); `test_jawa_cli_raises_with_stderr` (script que sai 1 com mensagem); `test_find_binary_prefers_env`.
- [ ] **Step 3:** FAIL. **Step 4:** implementar. **Step 5:** PASS; `uv run mypy --strict src tests` e `uv run ruff check` limpos.

### Task 5: tela principal (cabeçalho, árvore, detalhes, barra de ações)

**Files:** Create `tui/src/jawa_tui/widgets/*.py`, `tui/src/jawa_tui/app.py`, `tui/src/jawa_tui/jawa.tcss`, `tui/tests/test_app.py`

**Interfaces:**
- `JawaApp(core: Core)`; ao montar, mede num worker (`@work(thread=True)`) mostrando "Medindo…" com spinner; ao terminar, preenche a árvore. `main()` monta `JawaCli(find_binary())` ou mostra o erro de binário ausente.
- Árvore: categorias ordenadas como o registro, cada uma com ícone, título, tamanho, barra (relativa à maior) e etiqueta; os itens com `☑`/`☐` (marcável), `🔒` (guiado) ou `⛔` (bloqueado). `Espaço` ou clique alterna; na categoria, alterna todos os itens marcáveis. `Enter`/clique abre e fecha.
- Detalhes: categoria sob o cursor → resumo, etiqueta, explicação curta (`o_que_e` + `recomendacao`), rodapé e problema; item sob o cursor → local, tamanho, nota, como voltar, motivo do bloqueio ou o comando guiado.
- Barra de ações: `☑ N itens · X GB` + botões `Limpar marcados` (desabilitado sem seleção), `Medir de novo`, `Ajuda`. Teclas: `l`, `m`, `?`, `q`.

- [ ] **Step 1: testes que falham** (com `FakeCore` e `App.run_test()`): `test_shows_disk_and_categories` (cabeçalho com `49,9 GB livres de 249,8 GB` e `80% usado`; `Cache do npm` e `● Seguro` na tela); `test_safe_items_start_checked_and_review_unchecked`; `test_guided_and_blocked_cannot_be_checked`; `test_category_toggle_marks_children`; `test_details_follow_cursor`; `test_problem_category_shows_reason` (Review Focus 4); `test_fits_80x24` (Review Focus 3); `test_missing_binary_message` (Review Focus 1); `test_counter_updates`.
- [ ] **Step 2:** FAIL. **Step 3:** implementar. **Step 4:** PASS, mypy e ruff limpos.

### Task 6: confirmar, limpar, resultado e ajuda

**Files:** Create `tui/src/jawa_tui/screens/{confirm,progress,result,help}.py`, `tui/tests/test_confirm.py`; Modify `app.py`, `test_app.py`

**Interfaces:**
- `ConfirmScreen(items, warnings)` (`ModalScreen[bool]`): lista com rolagem, total fixo, avisos cruzados; com irreversível, um `Input` e o botão `Limpar` só habilitado com o texto exatamente `apagar`.
- `ProgressScreen`: barra `feitos/total` e o item atual. **Sem botão de cancelar na v0.2**: o `jawa limpar` não trata SIGINT (limitação já registrada), então interromper no meio deixaria um item pela metade e fora do histórico. A janela diz "Limpando… não feche o terminal".
- `ResultScreen(result)`: números grandes (liberado no disco, na Lixeira, na VM do Docker Desktop com o comando de recuperar), dica de snapshot, itens pulados/falhos com motivo, avisos; botão `Medir de novo`.
- `HelpScreen`: as teclas e as cores das etiquetas.
- Erro do `limpar` (ex.: medição vencida) → notificação em vermelho com a mensagem do Rust e o botão `Medir de novo` em foco (Review Focus 2).

- [ ] **Step 1: testes que falham:** `test_enter_without_apagar_does_nothing` e `test_apagar_with_spaces_is_rejected` (Review Focus 5); `test_without_irreversible_enter_confirms`; `test_clean_flow_shows_result` (FakeCore devolve `CleanResult` com Lixeira e VM → a tela mostra as três linhas); `test_clean_error_suggests_remeasure` (Review Focus 2); `test_help_lists_keys`.
- [ ] **Step 2:** FAIL. **Step 3:** implementar. **Step 4:** PASS, mypy e ruff limpos.

### Task 7: CI, README e teste manual

**Files:** Modify `.github/workflows/ci.yml`, `README.md`, `.cargo/config.toml` (sem mudança se o lançador do Task 3 cobrir o `cargo dev`)

- [ ] Job `tui` no CI: `astral-sh/setup-uv` fixado por SHA, `uv sync --locked`, `uv run ruff check`, `uv run mypy --strict src tests`, `uv run pytest`.
- [ ] README: "Instalar a interface": `uv tool install "git+https://github.com/Otaviofachin/jawa#subdirectory=tui"`; desenvolvimento: `uv tool install --editable ./tui` uma vez e depois `cargo dev` (o `jawa` acha o `jawa-tui`).
- [ ] Teste manual pelo usuário: `cargo dev`, medir, navegar, marcar, limpar algo pequeno e ver o resultado.
- [ ] Commit único `feat: adiciona interface textual com painel duplo` na branch `feat-tui-textual`; merge na `main` e tag `v0.2.0` só com o aval do usuário.
