#!/usr/bin/env bash
# Autoteste de no_hooks_no_polling.sh: cada cenário monta uma árvore crates/
# falsa numa pasta temporária, roda a verificação e confere saída e código.
set -u

aqui="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
script="$aqui/no_hooks_no_polling.sh"
repo="$(cd "$aqui/../.." && pwd)"
smoke="$repo/crates/duplicata-win/tests/history_window_smoke.rs"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

msg_ok='OK: nenhum gancho nem timer periódico no código do app (src/ e examples/ de cada crate).'
falhas=0
saida=""
codigo=0

falhou() {
  echo "FALHOU: $1 — $2"
  echo "--- saída ---"
  echo "$saida"
  echo "-------------"
  falhas=$((falhas + 1))
}

# Cria uma árvore nova para o cenário e devolve a raiz (crates/) em $raiz.
nova_arvore() {
  raiz="$tmp/$1/crates"
  mkdir -p "$raiz"
}

# escreve <caminho relativo à raiz> <conteúdo>
escreve() {
  mkdir -p "$(dirname "$raiz/$1")"
  printf '%s\n' "$2" > "$raiz/$1"
}

roda() {
  saida="$(bash "$script" "$raiz" 2>&1)"
  codigo=$?
}

espera_passar() {
  roda
  [ "$codigo" -eq 0 ] || { falhou "$1" "esperava saída 0, veio $codigo"; return; }
  printf '%s\n' "$saida" | /usr/bin/grep -qxF -- "$msg_ok" \
    || falhou "$1" "mensagem de sucesso ausente"
}

# espera_falhar <cenário> <arquivo relativo à raiz> <linha> <trecho>
# A ocorrência tem que aparecer como "<arquivo>:<linha>:<trecho>" e a mensagem
# de erro tem que vir depois das ocorrências (última linha da saída).
espera_falhar() {
  roda
  [ "$codigo" -eq 1 ] || { falhou "$1" "esperava saída 1, veio $codigo"; return; }
  printf '%s\n' "$saida" | /usr/bin/grep -qxF -- "$raiz/$2:$3:$4" \
    || falhou "$1" "ocorrência \"$2:$3:$4\" não listada"
  case "$(printf '%s\n' "$saida" | tail -n 1)" in
    '::error::'*) ;;
    *) falhou "$1" "a última linha não é a mensagem de erro" ;;
  esac
}

# espera_nao_rodar <cenário>: a verificação não pôde rodar e não finge sucesso.
espera_nao_rodar() {
  roda
  [ "$codigo" -eq 2 ] || { falhou "$1" "esperava saída 2, veio $codigo"; return; }
  printf '%s\n' "$saida" | /usr/bin/grep -qF -- '::error::' \
    || falhou "$1" "mensagem de erro ausente"
  if printf '%s\n' "$saida" | /usr/bin/grep -qF -- 'OK:'; then
    falhou "$1" "imprimiu mensagem de sucesso"
  fi
}

limpo='pub fn soma(a: i32, b: i32) -> i32 { a + b }'

# a) o teste de fumaça real em tests/ não reprova
nova_arvore a
escreve x/src/lib.rs "$limpo"
if [ -f "$smoke" ]; then
  mkdir -p "$raiz/x/tests"; cp "$smoke" "$raiz/x/tests/"
  espera_passar "a) gancho do teste de fumaça em tests/"
else
  falhou "a) gancho do teste de fumaça em tests/" "arquivo $smoke não encontrado"
fi

# b) gancho de teclado em src/
nova_arvore b
gancho_teclado='unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, None, None, 0) };'
escreve duplicata-win/src/history_window.rs "fn f() {}
$gancho_teclado"
espera_falhar "b) WH_KEYBOARD_LL em duplicata-win/src" duplicata-win/src/history_window.rs 2 "$gancho_teclado"
erro_b="$saida"

# c) gancho de thread (WH_CBT) em src/ do app
nova_arvore c
gancho_cbt='SetWindowsHookExW(WH_CBT, Some(f), None, GetCurrentThreadId());'
escreve duplicata-app/src/main.rs "$gancho_cbt"
espera_falhar "c) WH_CBT em duplicata-app/src" duplicata-app/src/main.rs 1 "$gancho_cbt"

# d) timer em examples/ (subpasta)
nova_arvore d
timer='SetTimer(hwnd, 1, 100, None);'
escreve duplicata-core/src/lib.rs "$limpo"
escreve duplicata-core/examples/wxs_invariants/main.rs "$timer"
espera_falhar "d) SetTimer em duplicata-core/examples" duplicata-core/examples/wxs_invariants/main.rs 1 "$timer"

# e) timer em examples/
nova_arvore e
escreve duplicata-win/src/lib.rs "$limpo"
escreve duplicata-win/examples/show_history_window.rs "$timer"
espera_falhar "e) SetTimer em duplicata-win/examples" duplicata-win/examples/show_history_window.rs 1 "$timer"

# f) timer e gancho em tests/
nova_arvore f
escreve duplicata-store/src/lib.rs "$limpo"
escreve duplicata-store/tests/t.rs "SetTimer(hwnd, 1, 100, None);
SetWindowsHookExW(WH_CBT, None, None, 0);"
espera_passar "f) SetTimer e SetWindowsHookEx em duplicata-store/tests"

# g) gancho em tests/support/
nova_arvore g
escreve duplicata-win/src/lib.rs "$limpo"
escreve duplicata-win/tests/support/qualquer.rs "SetWindowsHookExW(WH_CBT, Some(f), None, GetCurrentThreadId());"
espera_passar "g) WH_CBT em duplicata-win/tests/support"

# h) #[cfg(test)] e testsupport dentro de src/ continuam verificados
nova_arvore h1
escreve x/src/lib.rs "#[cfg(test)]
mod tests {
    fn f() { SetTimer(hwnd, 1, 100, None); }
}"
espera_falhar "h) #[cfg(test)] mod tests em src/lib.rs" x/src/lib.rs 3 "    fn f() { SetTimer(hwnd, 1, 100, None); }"
nova_arvore h2
escreve x/src/lib.rs "$limpo"
escreve x/src/testsupport/x.rs "SetWindowsHookExW(WH_CBT, None, None, 0);"
espera_falhar "h) src/testsupport/x.rs" x/src/testsupport/x.rs 1 "SetWindowsHookExW(WH_CBT, None, None, 0);"

# i) crate nova entra sem mudar o job
nova_arvore i
escreve duplicata-core/src/lib.rs "$limpo"
escreve nova/src/lib.rs "KillTimer(hwnd, 1);"
espera_falhar "i) crate nova" nova/src/lib.rs 1 "KillTimer(hwnd, 1);"

# j) texto da mensagem de erro (do cenário b)
saida="$erro_b"
for trecho in 'Zero despertares em ociosidade' 'ARQUITETURA.md' 'tests/'; do
  printf '%s\n' "$erro_b" | /usr/bin/grep -qF -- "$trecho" \
    || falhou "j) mensagem de erro" "não contém \"$trecho\""
done
if printf '%s\n' "$erro_b" | /usr/bin/grep -qiF -- 'constituição'; then
  falhou "j) mensagem de erro" "contém \"constituição\""
fi
# "princípio" sozinho é permitido ("pelo princípio ..."); proibido é o número.
# Sem -i, para [IVXLC] casar só numeral romano maiúsculo.
if printf '%s\n' "$erro_b" | /usr/bin/grep -qE -- '[Pp]rincípio +[IVXLC0-9]+\b'; then
  falhou "j) mensagem de erro" "contém número de princípio"
fi

# k) cada padrão restante em src/
n=0
for linha in 'let k = WH_KEYBOARD;' 'KillTimer(hwnd, 1);' 'timeBeginPeriod(1);' 'timeSetEvent(1, 0, None, 0, 0);'; do
  n=$((n + 1))
  nova_arvore "k$n"
  escreve x/src/lib.rs "$linha"
  espera_falhar "k) padrão '$linha' em src/" x/src/lib.rs 1 "$linha"
done

# l) crate sem examples/ não engana a verificação
nova_arvore l1
escreve duplicata-app/src/main.rs "$limpo"
escreve duplicata-core/src/lib.rs "$limpo"
escreve duplicata-core/examples/e.rs "$limpo"
espera_passar "l) crate sem examples/, tudo limpo"
nova_arvore l2
escreve duplicata-app/src/main.rs "timeSetEvent(1, 0, None, 0, 0);"
escreve duplicata-core/src/lib.rs "$limpo"
escreve duplicata-core/examples/e.rs "$limpo"
espera_falhar "l) crate sem examples/, padrão no src/ dela" duplicata-app/src/main.rs 1 "timeSetEvent(1, 0, None, 0, 0);"

# m) sem nenhuma pasta src/ ou examples/ a verificação não roda (nem finge sucesso)
nova_arvore m1
espera_nao_rodar "m) raiz vazia"
raiz="$tmp/m2/nao-existe"
espera_nao_rodar "m) raiz inexistente"
nova_arvore m3
escreve x/tests/t.rs "$timer"
espera_nao_rodar "m) crate só com tests/ (com padrão)"

# n) pastas e arquivos chamados tests/support DENTRO de src/ ou examples/ continuam verificados
nova_arvore n1
escreve x/src/lib.rs "$limpo"
escreve x/src/tests/t.rs "$timer"
espera_falhar "n) pasta src/tests/" x/src/tests/t.rs 1 "$timer"
nova_arvore n2
escreve x/src/lib.rs "$limpo"
escreve x/src/tests.rs "$timer"
espera_falhar "n) arquivo src/tests.rs" x/src/tests.rs 1 "$timer"
nova_arvore n3
escreve x/src/lib.rs "$limpo"
escreve x/examples/tests/t.rs "$timer"
espera_falhar "n) pasta examples/tests/" x/examples/tests/t.rs 1 "$timer"
nova_arvore n4
escreve tests/src/lib.rs "$timer"
espera_falhar "n) crate chamada tests" tests/src/lib.rs 1 "$timer"
nova_arvore n5
escreve x/src/support/s.rs "$timer"
espera_falhar "n) pasta src/support/" x/src/support/s.rs 1 "$timer"

# o) o alcance não filtra por extensão
nova_arvore o1
escreve x/src/lib.rs "$limpo"
escreve x/src/dados.toml "$timer"
espera_falhar "o) padrão em arquivo .toml dentro de src/" x/src/dados.toml 1 "$timer"
nova_arvore o2
escreve x/src/lib.rs "$limpo"
escreve x/examples/LEIAME "$timer"
espera_falhar "o) padrão em arquivo sem extensão dentro de examples/" x/examples/LEIAME 1 "$timer"

# p) cada ocorrência é listada, não só a primeira
nova_arvore p
escreve x/src/lib.rs "$timer
$limpo
KillTimer(hwnd, 1);"
espera_falhar "p) primeira ocorrência listada" x/src/lib.rs 1 "$timer"
saida_p="$saida"
printf '%s\n' "$saida_p" | /usr/bin/grep -qxF -- "$raiz/x/src/lib.rs:3:KillTimer(hwnd, 1);" \
  || falhou "p) segunda ocorrência listada" "x/src/lib.rs:3 não listada"

if [ "$falhas" -gt 0 ]; then
  echo "Autoteste: $falhas falha(s)."
  exit 1
fi
echo "Autoteste: todos os cenários passaram."
