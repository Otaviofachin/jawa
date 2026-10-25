from textual.app import ComposeResult
from textual.binding import Binding
from textual.containers import Vertical
from textual.screen import ModalScreen
from textual.widgets import Static

HELP_TEXT = """\
↑/↓ ou j/k   move o cursor
Enter        abre a categoria (num item, marca ou desmarca)
Espaço       marca ou desmarca (na categoria, todos os itens)
l            limpar os itens marcados
m            medir de novo
?            esta ajuda
q            sair
Também dá para clicar com o mouse.

● Seguro         volta sozinho; já vem marcado
● Revisar        você decide item por item
🔒 Só com sudo   o Jawa só mostra o comando para você rodar
● Indisponível   a ferramenta não está instalada ou parada
⛔               bloqueado: em uso ou de outro dono

Itens irreversíveis pedem que você digite apagar na confirmação."""


class HelpScreen(ModalScreen[None]):
    BINDINGS = [
        Binding("escape", "close", "Fechar"),
        Binding("question_mark", "close", "Fechar", show=False),
        Binding("q", "close", "Fechar", show=False),
    ]

    plain = HELP_TEXT

    def compose(self) -> ComposeResult:
        with Vertical(id="help-box") as box:
            box.border_title = "Ajuda"
            yield Static(HELP_TEXT)

    def action_close(self) -> None:
        self.dismiss(None)
