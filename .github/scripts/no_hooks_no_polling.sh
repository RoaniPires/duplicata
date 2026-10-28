#!/usr/bin/env bash
# Proíbe ganchos de janela/teclado e timers periódicos no código do app:
# src/ e examples/ de cada crate. As pastas tests/ não vão para o app
# instalado e ficam de fora (ex.: o gancho WH_CBT do teste de fumaça).
# Uso: bash no_hooks_no_polling.sh [raiz]   (padrão: crates)
set -u
raiz="${1:-crates}"
pattern='SetWindowsHookEx|WH_KEYBOARD_LL|WH_KEYBOARD\b|SetTimer\(|KillTimer\(|timeBeginPeriod|timeSetEvent'

pastas=()
for pasta in "$raiz"/*/src "$raiz"/*/examples; do
  [ -d "$pasta" ] && pastas+=("$pasta")
done
if [ "${#pastas[@]}" -eq 0 ]; then
  echo "::error::A verificação de ganchos e timers não pôde rodar: nenhuma pasta src/ ou examples/ encontrada em $raiz/."
  exit 2
fi

codigo=0
grep -rEn -- "$pattern" "${pastas[@]}" || codigo=$?
case "$codigo" in
  0)
    echo '::error::Gancho de janela/teclado ou timer periódico encontrado no código do app (src/ ou examples/). Proibido pelo princípio "Zero despertares em ociosidade" do ARQUITETURA.md: o app não pode acordar sozinho em ociosidade nem interceptar o teclado do sistema. As pastas tests/ não são verificadas.'
    exit 1
    ;;
  1)
    echo 'OK: nenhum gancho nem timer periódico no código do app (src/ e examples/ de cada crate).'
    exit 0
    ;;
  *)
    echo "::error::A verificação de ganchos e timers não pôde rodar: grep terminou com código $codigo."
    exit 2
    ;;
esac
