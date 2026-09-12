# Duplicata

Gerenciador de histórico de área de transferência para Windows.
Sem rede, sem telemetria, sem conta. Roda na bandeja do sistema.

[![Baixar](https://img.shields.io/badge/Baixar-duplicata.msi-2ea44f?style=for-the-badge&logo=windows&logoColor=white)](https://github.com/RoaniPires/duplicata/releases/latest)

## O que ele faz

Guarda o que você copia e devolve por um atalho. Tudo fica na sua máquina,
em `%LocalAppData%\duplicata`.

- **Não guarda senha**: respeita a marcação que gerenciadores de senha usam
  para dizer "não registre isto", e descarta por conta própria o que parece
  chave privada, token ou número de cartão.
- **Não guarda para sempre**: apaga sozinho depois de 7 dias e acima de 500
  itens, os dois configuráveis.
- **Não fala com ninguém**: nenhuma conexão de rede em nenhuma situação.
- **Opcionalmente criptografado**: o banco pode ser protegido pela sua conta
  do Windows.

## Instalar

Baixe o `.msi` em [Releases](https://github.com/RoaniPires/duplicata/releases/latest)
e execute. Não pede administrador e instala só para o seu usuário.

Quem preferir compilar do fonte: veja [installer/README.md](installer/README.md).

## Usar

Copie normalmente com `Ctrl+C`. Para ver o histórico, aperte **`Ctrl+Shift+V`**.

| Atalho | O que faz |
|---|---|
| `Ctrl+Shift+V` | abre o histórico |
| digitar | filtra a lista a cada tecla |
| `↑` `↓` `Home` `End` `PageUp` `PageDown` | navega |
| `Enter` ou duplo clique | cola o item, com a formatação original |
| `Shift+Enter` | cola só o texto, sem formatação |
| `Ctrl+P` | fixa ou desafixa (itens fixados não expiram) |
| `Tab` + `←` `→` | filtra por tipo: tudo, texto, imagem, arquivos |
| `Esc` | limpa o filtro; sem filtro, fecha |

Clique com o botão direito num item para fixar ou apagar todo o histórico.
Clique com o botão direito no ícone da bandeja para abrir as Configurações,
onde ficam o atalho, os prazos de retenção, a lista de programas ignorados e
a proteção do banco.

## Mais

- [ARQUITETURA.md](ARQUITETURA.md) — como é feito por dentro, e por quê
- [SECURITY.md](SECURITY.md) — como verificar que o arquivo baixado é o que diz ser

## Licença

MIT ou Apache 2.0 — [LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE).
