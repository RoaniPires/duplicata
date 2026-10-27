# Ícones

| arquivo | de onde vem |
|---|---|
| `duplicata.ico` | ícone do aplicativo (barra de tarefas, bandeja, instalador). **Não** é gerado aqui: é um asset editado à parte. |
| `settings_light.ico` | engrenagem de "Configurações" do menu da bandeja, traço escuro (#1F1F22) para o menu claro. Gerado. |
| `settings_dark.ico` | a mesma engrenagem, traço claro (#F0F0F2) para o menu escuro. Gerado. |

## A engrenagem de "Configurações"

A fonte dos dois `settings_*.ico` é
`crates/duplicata-core/examples/settings_icons/gear.rs`. Os parâmetros do glifo
(dentes, raios da ponta, da raiz e do furo, meia-largura do dente,
supersampling, cores e tamanhos 16, 20, 24 e 32) ficam juntos em `GearParams`,
`SETTINGS_GEAR`, `SIZES` e `VARIANTS`. Para mudar o glifo, ajuste esses valores
e regenere, de qualquer pasta do workspace (só o toolchain Rust):

```sh
cargo run -p duplicata-core --example settings_icons
```

O comando regrava os dois arquivos nesta pasta e imprime o nome e os tamanhos
de cada um. É um *example* do `duplicata-core`: o build do app
(`cargo build -p duplicata-app` ou `cargo build --workspace`) não o compila.

### Testes

- `crates/duplicata-core/tests/settings_icons.rs` é a guarda: gera os ícones em
  memória e compara pixel a pixel (RGBA) com os `.ico` desta pasta. Falha
  citando o arquivo, o tamanho e a primeira coordenada diferente se alguém
  editar um `.ico` à mão ou mudar o gerador sem regenerar.
- `crates/duplicata-core/tests/icon_contrast.rs` mede o contraste dos ícones
  contra os fundos claro e escuro. Roda também em Linux, no job
  `core-portability` da CI.

## Experimento descartado: keyline no ícone da barra

Para dar contraste ao ícone da barra de tarefas no tema claro, foi tentado
aplicar um keyline escuro e uma margem interna ao `duplicata.ico`. O resultado
media bem, mas engordava o traço a ponto de embolar o glifo em 16×16, e foi
descartado. O script da tentativa saiu do repositório; para recuperá-lo:

```sh
git show 6e54eb1:assets/tools/mkicon.py
```
