# Procedência do artefato publicado

Como confirmar, a partir só deste repositório público e do artefato
publicado, que um `duplicata-<versão>.msi` específico veio exatamente deste
código-fonte — sem contato com o mantenedor, sem acesso privilegiado, sem
conta em serviço nenhum (research.md R7, FR-027/028/029/030).

Nome do arquivo: os artefatos publicados carregam a versão no nome
(`duplicata-<versão>.msi`, ex.: `duplicata-0.1.1.msi`) — legível para quem
baixa, sem precisar saber o SHA do commit. O artefato **não é assinado** com
certificado Authenticode (PG-003); esse aviso está nas notas de cada release
e nesta página, não mais codificado no nome do arquivo. Antes de instalar,
confirme a procedência pelo atestado (passo a passo abaixo).

## O que a esteira publica

Cada release (`.github/workflows/release.yml`, disparada por uma tag `v*`)
gera o MSI e, em seguida, um **atestado de procedência** assinado pela
identidade efêmera do próprio workflow do GitHub Actions (sem chave de longo
prazo para gerenciar ou vazar — `actions/attest-build-provenance`), no
formato SLSA/in-toto, publicado no **Rekor**, o log de transparência público
do projeto Sigstore. O atestado liga o hash do MSI ao commit exato e ao
workflow que o produziu — é o que este roteiro verifica.

## Ferramenta

**[`cosign`](https://github.com/sigstore/cosign)** — binário único, sem
dependência, do projeto Sigstore (Linux Foundation). Instalação sem conta,
sem custo:

```powershell
winget install sigstore.cosign
# ou: scoop install cosign
```

## Passo a passo (Windows, PowerShell)

1. Baixe o MSI da release e calcule o hash SHA-256:

   ```powershell
   Get-FileHash .\duplicata-<versão>.msi -Algorithm SHA256
   ```

2. Busque a lista de atestados desse hash no endpoint público do GitHub —
   **sem autenticação nenhuma** para um repositório público (confirmado
   nesta sessão, testando de verdade contra um artefato público real: HTTP
   200 sem qualquer cabeçalho de autenticação):

   ```powershell
   $hash = (Get-FileHash .\duplicata-<versão>.msi -Algorithm SHA256).Hash.ToLower()
   curl.exe -s "https://api.github.com/repos/RoaniPires/duplicata/attestations/sha256:$hash" -o attestations.json
   ```

3. **Achado real, não documentado explicitamente pela GitHub** (verificado
   nesta sessão baixando e inspecionando um bundle de verdade): a resposta
   não traz o atestado inline — traz um `bundle_url` (um blob assinado, ele
   também sem autenticação adicional) cujo conteúdo vem **comprimido em
   Snappy bruto** (`Content-Type: application/x-snappy`). Sem descomprimir,
   o `cosign` não lê o arquivo. Resolvido com `python-snappy` (Python +
   `pip`, sem conta, sem custo):

   ```powershell
   pip install python-snappy
   ```

   ```python
   # baixar_atestado.py — extrai o bundle_url da resposta do passo 2,
   # baixa o blob e descomprime (Snappy bruto, não "framed") para
   # bundle.json, pronto para o cosign.
   import json, snappy, urllib.request

   attestations = json.load(open("attestations.json", encoding="utf-8"))
   bundle_url = attestations["attestations"][0]["bundle_url"]
   with urllib.request.urlopen(bundle_url) as r:
       compressed = r.read()
   with open("bundle.json", "wb") as f:
       f.write(snappy.decompress(compressed))
   print("bundle.json escrito")
   ```

   ```powershell
   python baixar_atestado.py
   ```

4. Verifique com `cosign` — a identidade esperada é o próprio workflow de
   release deste repositório, nunca um e-mail ou conta pessoal:

   ```powershell
   cosign verify-blob-attestation `
     --bundle bundle.json `
     --new-bundle-format `
     --certificate-oidc-issuer="https://token.actions.githubusercontent.com" `
     --certificate-identity-regexp="^https://github.com/RoaniPires/duplicata/.github/workflows/release.yml.?" `
     .\duplicata-<versão>.msi
   ```

   - **Sucesso**: `cosign` confirma a assinatura, a entrada no Rekor e que o
     hash do arquivo bate com o que o atestado descreve — este MSI saiu
     exatamente deste workflow, deste repositório, e não foi alterado depois
     de publicado (FR-029).
   - **Falha por adulteração**: o hash não bate — mude um byte do MSI e
     repita para confirmar que a verificação reprova sempre (T042). Isto
     nunca deve ser ignorado.
   - **Falha com "not enough verified log entries from transparency log"**:
     ver a nota abaixo — não é necessariamente adulteração.

   ⚠️ **Achado real desta sessão, não resolvido por completo**: testando
   esta receita contra um artefato público de terceiros (`gh` da própria
   GitHub, mesmo mecanismo `actions/attest-build-provenance`), o bundle
   servido por `bundle_url` trouxe `timestampVerificationData` mas **zero**
   `tlogEntries` — o comando acima falhou com exatamente esse erro, mesmo
   sem nenhuma adulteração. A documentação da GitHub afirma que repositórios
   **públicos** têm o atestado também publicado no Rekor público (ao
   contrário de privados, que usam só o carimbo de tempo) — mas o bundle
   servido pela API de attestations, nesta amostra real, não trouxe essa
   prova inline. Pode ser uma amostra específica (workflow mais antigo,
   configuração particular daquele repositório) — **este projeto ainda não
   publicou uma release real** (T053) para confirmar como o próprio atestado
   do duplicata se comporta. Se a verificação falhar com esse erro
   especificamente:
   - **Não** trate como prova de adulteração — o erro é sobre qual prova de
     transparência o bundle carrega, não sobre o hash do arquivo.
   - A alternativa cosign suporta é verificação por carimbo de tempo RFC3161
     em vez de Rekor (`--insecure-ignore-tlog` combinado com
     `--rfc3161-timestamp`/`--timestamp-certificate-chain` apontando para a
     cadeia da autoridade de carimbo do GitHub) — não documentado aqui por
     completo porque não foi possível terminar de confirmar a cadeia de
     certificado correta nesta sessão.
   - Registre o resultado (com o log completo do `cosign`) e trate como
     pendência a resolver quando a primeira release real sair — não como
     verificação bem-sucedida nem como falha definitiva.

## Alternativa de conveniência: `gh attestation verify`

Quem já usa o GitHub CLI (`gh`) pode preferir um único comando, que já
resolve os passos 2–4 internamente (inclusive a descompressão do bundle):

```powershell
gh attestation verify .\duplicata-<versão>.msi --repo RoaniPires/duplicata
```

Não é o caminho **principal** documentado aqui: o fluxo padrão do `gh`
recomenda `gh auth login` para não esbarrar no limite de taxa não
autenticado da API — fricção de conta que o passo a passo acima
deliberadamente evita (research.md R7, critério 4). Funciona igual, só que
para quem já tem o `gh` configurado.

## O que isto NÃO cobre

- **Assinatura Authenticode** (reputação de editor, SmartScreen) — sistema
  separado, ainda pendente (FR-005a). A procedência aqui prova "este binário
  saiu deste código"; a assinatura, quando existir, prova "o Windows confia
  no editor". Ver README.md, seção de distribuição.
- **Artefatos construídos localmente** — nunca têm atestado, e não devem ser
  distribuídos nem submetidos à assinatura (FR-005c).
