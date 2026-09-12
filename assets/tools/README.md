# Geradores dos ícones

Scripts de apoio, **não** entram no build. Rodam com Python 3 puro (sem
dependências): decodificam/codificam PNG com `zlib` e montam o `.ico` à mão.

| script | o quê |
|---|---|
| `gear.py` | gera `../settings_light.ico` e `../settings_dark.ico` — a engrenagem de "Configurações" do menu da bandeja (US9). É a **fonte** desses dois assets: para mexer no glifo, ajuste os raios/dentes aqui e rode de novo. |
| `mkicon.py` | aplica keyline escuro + margem interna a um `.ico` existente. Escrito para o T090 (contraste do ícone da barra no tema claro) e **não aplicado**: o resultado mede bem mas engorda o traço a ponto de embolar o glifo em 16×16. Fica aqui porque a tentativa é reproduzível — ver as pendências em `specs/004-native-look-and-filter/tasks.md`. |
| `png.py`, `pngenc.py` | decodificador/codificador PNG mínimos, usados pelos dois acima. |

```sh
cd assets/tools && python3 gear.py && mv settings_*.ico ..
```

A verificação de contraste que antes era script virou teste de verdade:
`crates/duplicata-core/tests/icon_contrast.rs`, que roda em Linux no job
`core-portability` da CI.
