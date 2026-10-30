# Arquitetura

Como o duplicata é feito por dentro, e as decisões que explicam o formato
atual. Para instalar e usar, veja o [README](README.md).

## Princípios

Oito regras que valeram para todas as decisões do projeto e que continuam
sendo o critério de revisão:

**Privacidade por padrão.** Nenhuma rede, nenhuma telemetria. O banco fica em
`%LocalAppData%\duplicata`, nunca em `Roaming` — que é replicado por perfil
móvel em domínio e frequentemente sincronizado por serviços de nuvem. Na
dúvida entre gravar e não gravar, não gravar.

**Decisão fora do Win32.** Toda lógica de decisão vive em `duplicata-core`,
sem nenhuma dependência de API do sistema operacional. Ela compila e passa nos
testes em qualquer plataforma, o que é verificado na CI rodando o crate em
Linux. O código Win32 é uma casca fina atrás de traits.

**Nenhuma funcionalidade sem teste.** Cobertura mínima de 90% em
`duplicata-core`, verificada na CI. E "o binário compilou" nunca conta como
"o teste rodou": a métrica é testes executados, e quem reporta declara o que o
ambiente não exercitou.

**O handler de clipboard nunca faz trabalho pesado.** Ele copia os bytes e
delega para uma worker thread. Hash, miniatura e escrita em banco acontecem
fora dele, porque o clipboard é um recurso global e segurá-lo trava o
aplicativo que acabou de copiar.

**Zero despertares em ociosidade.** Sem polling, sem hook global de teclado,
sem temporizador periódico. A message loop bloqueia em `GetMessageW` e a
worker bloqueia num `Condvar` sem timeout. Um teste conta os `WM_TIMER`
recebidos em sessões de 60 segundos e exige zero.

**Só Win32 e DWM.** Nenhum framework de interface com runtime embutido.

**Erros de inicialização são sempre visíveis.** Diálogo modal, nunca falha
silenciosa.

**A direção da degradação é escolhida de propósito.** Quando uma operação
pode falhar no meio, o que sobra não pode ser resultado do acaso — opaco em
vez de invisível, silêncio em vez de afirmação errada, arquivo inócuo em vez
de histórico, destravar em vez de bloquear.

## Workspace

| Crate | Responsabilidade | Win32 |
|---|---|:---:|
| `duplicata-core` | Lógica pura: seleção de formato canônico, identidade e dedupe, filtros de privacidade, retenção, ordenação da lista, busca, geometria de linha, contraste, plano de translucidez. Traits com fakes para teste. | não |
| `duplicata-store` | Persistência SQLite com WAL, `secure_delete` e `foreign_keys`. Criptografia via DPAPI. | só o módulo `crypto` |
| `duplicata-win` | Casca Win32/DWM: listener de clipboard, janela de histórico, diálogos, bandeja, atalho global. Todo o `unsafe` do projeto vive aqui. | sim |
| `duplicata-app` | Fiação e ordem de inicialização, observabilidade, worker thread. Nenhuma decisão. | indireto |

`duplicata-core` é `#![forbid(unsafe_code)]`. `duplicata-store` é `deny` com
exceção pontual no módulo de criptografia.

## Pipeline de captura

```
o conteúdo da área de transferência muda
   │
   ▼
janela message-only recebe WM_CLIPBOARDUPDATE
   │  (AddClipboardFormatListener — nunca polling)
   ▼
antes de abrir, só com a lista de formatos (GetUpdatedClipboardFormats):
   · só formatos de controle do OLE  → recusa, sem abrir
   · dono OLE ("DataObject")        → sonda o dono com WM_NULL (20 ms);
     calado → não abre e espera o backoff; a área mudou → cede à
     notificação nova; esgotado → lê como antes
   │
   ▼
OpenClipboard com backoff exponencial, teto de 250 ms
   │
   ▼
captura em duas fases, na mesma sessão de clipboard:
   1. enumera formatos e tamanhos, sem copiar nenhum byte
   2. aplica os filtros de privacidade antes de qualquer cópia:
      · formato de exclusão presente  → rejeita
      · programa de origem bloqueado  → rejeita
   3. escolhe o formato canônico e confere o limite do tipo
   4. só então copia os bytes
   │
   ├─ rejeitado → log só com o código, nenhum byte copiado
   ▼
fila com teto de 128 MiB agregados: nunca bloqueia o handler,
descarta o item mais antigo não processado se precisar
- - - - - - - - - - - - - - - - - - - - - - - - - - - - - - -
worker thread, bloqueada no pop da fila
   │
   ▼
heurística de segredo sobre o texto já copiado: PEM, token,
cartão validado por Luhn → não grava, avisa na bandeja com um
balão clicável que relê o clipboard e recupera
   │
   ▼
identidade = blake3 dos bytes do formato canônico, sem
normalização (CRLF e LF são conteúdos diferentes)
   │
   ▼
preview e miniatura
   │
   ▼
upsert: hash novo insere; hash repetido atualiza a recência e
troca os formatos pelos da cópia mais recente
   │
   ▼
retenção por evento, nunca por timer: remove itens não fixados
por prazo e por quantidade. Os mesmos dois cortes rodam na
inicialização e ao aplicar, cancelar ou salvar a configuração de
retenção nas Configurações.
```

A dedupe considera apenas o formato canônico. Formatos auxiliares variam entre
cópias do mesmo conteúdo — o `CF_HTML` carrega offsets que mudam conforme o
markup ao redor, o Office anexa dados de sessão, navegadores anexam a URL de
origem — e incluí-los na identidade faria cópias idênticas virarem entradas
diferentes.

## Decisões que parecem estranhas e não são

**`format_hotkey` é separado de `HotkeyCombo::format`.** O segundo é o formato
de serialização do `config.toml`, com round-trip garantido contra o parser.
Unificar os dois faria o aplicativo deixar de ler a configuração de quem já o
usa. As duas funções existem para propósitos diferentes e não devem ser
fundidas.

**A classe da janela do histórico tem `CS_DBLCLKS`.** Sem essa flag o Windows
nunca envia `WM_LBUTTONDBLCLK` e o duplo clique chega como dois cliques
simples.

**Formatos não-`HGLOBAL` do clipboard são ignorados por lista positiva.**
`CF_BITMAP` devolve um handle GDI, não um bloco de memória global, e tratá-lo
como tal corrompe o heap. A regra é permitir os formatos sabidamente
`HGLOBAL` e ignorar o resto, e não excluir uma lista de conhecidos — lista de
exclusão precisa estar completa para ser segura.

**A pintura translúcida pré-multiplica o alfa.** Com alfa 0 ou 255 isso é
indiferente; com alfa parcial o DWM compõe diferente do esperado e as cores
saem lavadas. O sintoma é sutil e difícil de rastrear até a origem.

**A leitura do estado de inicialização aprovado é tolerante.** O formato
binário da chave não é documentado; valor ausente, de tamanho inesperado ou
ilegível resulta em "indeterminado", e a interface mostra apenas o estado da
chave de execução. Melhor mostrar menos do que afirmar errado.

**A folha de vidro do DWM exige buffered paint.** GDI não escreve o canal
alfa, então estender o frame sem controlar o alfa deixa a janela inteira como
chave de composição — invisível, não translúcida. O plano de alfa começa
sempre tornando o buffer inteiro opaco e só então abre transparência, de modo
que uma interrupção produz uma janela opaca, nunca invisível.

**A captura não abre a área enquanto um dono OLE não responde.** O
`Set-Clipboard` do PowerShell 5.1 (que o Claude Code chama a cada cópia no
Windows) publica pelo `OleSetClipboard` em três atualizações: primeiro só
"DataObject" e "Ole Private Data", depois os formatos de texto adiados, e por
fim o flush, com os bytes. Medido com o exemplo `clipboard_trace`: desligado,
16 de 16 cópias completaram as três em ~15 ms; ligado, a captura abria a área
no meio e pedia formatos adiados (`GetClipboardData` manda `WM_RENDERFORMAT` à
janela OLE), o intervalo entre as atualizações subia para ~110 ms e, em 3 de 9
cópias, o flush não aconteceu: o Claude Code mata o PowerShell aos 2 s e o
texto some — fica só [DataObject, Ole Private Data, CF_LOCALE], sem dono, e o
Ctrl+V cola vazio. A leitura é de que o dono OLE e o leitor ficam esperando um
pelo outro; o mecanismo exato dentro do OLE não foi observado. Por isso a
decisão é tomada antes de abrir: estado só de controle do OLE não é conteúdo
(o "DataObject" continua lá mesmo depois do flush, então o que se recusa é a
lista sem nenhum outro formato), e com dono OLE ele precisa responder a
`WM_NULL` em 20 ms.

`tests/owner_gate.rs`, `tests/capture_phases.rs` e `tests/backoff.rs` do
`duplicata-core` prendem a regra. `tests/ole_flush_race_smoke.rs` do
`duplicata-win` prova a regra contra um dono OLE de modelo, que supõe o que a
medição sugere: atende os pedidos de render dos outros programas da sessão e
não atende o do duplicata a tempo. Ele não reproduz o PowerShell real; a prova
causal é o aceite manual, com o `clipboard_trace` gravando junto.

**O duplicata reinicia o Explorer, mas nunca encerra processo à força.** A
opção "Abrir com Win+V" inclui `V` no `DisabledHotkeys` do Explorer, que só
relê o valor ao iniciar. O reinício acontece só pelo botão "Reiniciar o
Windows Explorer agora" das Configurações, depois de uma confirmação, e pelo
Restart Manager sem a flag de encerramento forçado: o Explorer pode recusar
fechar, e então nada muda. A espera de até 10 segundos pela volta dele roda
numa thread criada pelo clique, que só existe durante a ação e termina em
seguida, sem `WM_TIMER` — a message loop continua dormindo em `GetMessageW`,
e o "zero despertares em ociosidade" vale. Se, passado o prazo, não houver
shell, o duplicata abre `%WINDIR%\explorer.exe` pelo caminho completo: é o
único programa que ele executa. A CI barra `RmForceShutdown` junto com
`TerminateProcess`.

**O reinício do Explorer é medido pela hora de início do processo do
shell.** O `TaskbarCreated` é como o duplicata percebe que o Explorer voltou,
sem timer nem polling, mas ele também chega sem reinício (troca de DPI ou de
tema). A hora de criação do processo dono da janela do shell, guardada no
`config.toml` a cada Salvar da opção, diz se é outro Explorer: igual, o
`TaskbarCreated` não muda nada; diferente, o duplicata tenta o `Win+V` e, se
o Windows não o liberou, abre a janela de falha. Sem conseguir ler a hora, o
estado fica "aguardando", sem janela. `tests/win_v.rs` e
`tests/win_v_explorer.rs` do `duplicata-core` prendem a regra.

## Build e verificação

```powershell
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all --check
cargo llvm-cov -p duplicata-core --fail-under-lines 90
cargo deny check bans licenses sources advisories
```

Alguns testes precisam de sessão gráfica real e ficam `#[ignore]` por padrão:

```powershell
cargo test -p duplicata-win -- --ignored --test-threads=1 --nocapture
cargo test -p duplicata-app --test idle_wakeups -- --ignored --test-threads=1
```

A CI roda, além do conjunto acima em Windows, o `duplicata-core` sozinho em
Linux — é o que prova a fronteira de plataforma —, um lint que proíbe hooks de
teclado e temporizadores periódicos, um lint que proíbe encerramento forçado
de processo, e um verificador (`cargo run -p duplicata-core --example
wxs_invariants -- installer/duplicata.wxs`) que faz parse do instalador e reprova se o
escopo deixar de ser por usuário, se aparecer instalação de serviço, se o
Restart Manager for reativado ou se o downgrade passar a ser permitido.

## Instalador

MSI construído com WiX, escopo por usuário, sem elevação. A escolha do MSI
sobre outros empacotadores foi pelo critério de não disparar heurística de
antivírus: um MSI é uma tabela declarativa interpretada pelo `msiexec`, que já
é assinado pela Microsoft, enquanto outros formatos produzem um executável com
payload embutido — exatamente o padrão que a heurística persegue.

O Restart Manager fica desabilitado deliberadamente. Ligado, ele faz o
instalador enviar uma mensagem de fim de sessão, o aplicativo interpreta
corretamente como tal e reduz o tempo de encerramento, pulando a consolidação
do banco e a re-encriptação — o que deixaria a cópia de trabalho em texto
claro no disco. O ganho aparente de "fechar o app sozinho" é justamente o que
produz o vazamento.

A atualização por cima encerra o aplicativo pelo mesmo caminho do "Encerrar"
da bandeja e espera a saída do processo sem teto de tempo, porque a operação
que mais demora é justamente a que não pode ser interrompida. Nunca força o
encerramento.

## Limitações conhecidas

**Programas bloqueados são identificados pelo nome do executável.** É uma
conveniência contra vazamento acidental de aplicativo legítimo, não uma defesa
contra software que se disfarce. As outras duas checagens de captura valem
independentemente da lista.

**A marcação de conteúdo sensível depende do aplicativo de origem.** O
reconhecimento do formato está coberto por teste, incluindo um que registra o
formato de verdade e confirma a rejeição, mas se um gerenciador de senhas não
emitir a marcação não há como detectar isso do lado de quem lê o clipboard.

**A proteção por conta do Windows não protege contra a própria sessão.** Ela
protege o arquivo contra acesso ao disco por fora da conta que a ativou. Um
programa rodando com o mesmo usuário logado consegue decriptar, porque é assim
que o DPAPI funciona. Enquanto o aplicativo roda, o SQLite opera sobre uma
cópia de trabalho decriptada; num encerramento não limpo ela fica em texto
claro até o próximo início, que a reencripta antes de aceitar qualquer item.

**Colar só texto não funciona em aplicativos com toolkit próprio.** Essa ação
usa `WM_PASTE`, que só é respondida por controles de edição nativos. O
conteúdo é escrito corretamente na área de transferência de qualquer forma.
Confirmar com `Enter` ou duplo clique usa um mecanismo diferente e não tem
essa limitação.

**A busca considera o texto exibido, não o conteúdo completo.** É deliberado:
a busca nunca lê os bytes do item, o que a mantém instantânea e sem tocar em
conteúdo fora da tela. Recopiar o item atualiza o preview. A busca ignora
maiúsculas, mas não acentos.

**O balão de rejeição pode não aparecer** se as notificações estiverem
desativadas para o aplicativo. `Shell_NotifyIcon` devolve sucesso mesmo quando
o shell decide não desenhar, e não existe API documentada para consultar essa
permissão para um ícone de bandeja sem identidade de aplicativo registrada.

---

As cinco etapas de desenvolvimento deste projeto foram conduzidas com o
[GitHub Spec Kit](https://github.com/github/spec-kit).
