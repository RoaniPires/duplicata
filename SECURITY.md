# Procedência do artefato publicado

Como confirmar, a partir só deste repositório público e do artefato
publicado, que um `duplicata-<versão>.msi` específico veio exatamente deste
código-fonte — sem contato com o mantenedor, sem acesso privilegiado, sem
conta em serviço nenhum.

Nome do arquivo: os artefatos publicados carregam a versão no nome
(`duplicata-<versão>.msi`, ex.: `duplicata-0.1.2.msi`) — legível para quem
baixa, sem precisar saber o SHA do commit. O artefato **não é assinado** com
certificado Authenticode do Windows — ver `CODE_SIGNING_POLICY.md` para o
estado atual disso. Antes de instalar, confirme a procedência pelo atestado
(passo a passo abaixo).

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
   sem autenticação nenhuma, mesmo para quem nunca configurou nada:

   ```powershell
   $hash = (Get-FileHash .\duplicata-<versão>.msi -Algorithm SHA256).Hash.ToLower()
   curl.exe -s "https://api.github.com/repos/RoaniPires/duplicata/attestations/sha256:$hash" -o attestations.json
   ```

3. A resposta não traz o atestado inline — traz um `bundle_url` (um blob
   assinado, também sem autenticação adicional) cujo conteúdo vem
   **comprimido em Snappy bruto** (`Content-Type: application/x-snappy`,
   não documentado explicitamente pela GitHub). Sem descomprimir, o
   `cosign` não lê o arquivo. Resolvido com `python-snappy` (Python +
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
   release deste repositório, nunca um e-mail ou conta pessoal. O atestado
   é do tipo SLSA provenance v1, então `--type` precisa dizer isso
   explicitamente (sem essa flag o `cosign` assume um predicado
   `custom` e recusa o arquivo antes mesmo de checar a assinatura):

   ```powershell
   cosign verify-blob-attestation `
     --bundle bundle.json `
     --type slsaprovenance1 `
     --certificate-oidc-issuer="https://token.actions.githubusercontent.com" `
     --certificate-identity-regexp="^https://github.com/RoaniPires/duplicata/.github/workflows/release.yml.?" `
     .\duplicata-<versão>.msi
   ```

   - **Sucesso**: `cosign` imprime `Verified OK` — o hash do arquivo bate
     com o que o atestado descreve, com uma entrada real no Rekor público
     por trás. Esta receita, exatamente como está aqui, foi rodada contra
     um MSI baixado de uma release publicada deste repositório e verificou
     com sucesso de ponta a ponta.
   - **Falha por adulteração**: o hash não bate — mude um byte do MSI e
     repita; a verificação reprova sempre. Isto nunca deve ser ignorado.

## Alternativa de conveniência: `gh attestation verify`

Quem já usa o GitHub CLI (`gh`) pode preferir um único comando, que já
resolve os passos 2–4 internamente (inclusive a descompressão do bundle):

```powershell
gh attestation verify .\duplicata-<versão>.msi --repo RoaniPires/duplicata
```

Não é o caminho principal documentado aqui porque `gh attestation verify`
exige `gh auth login` (ou um `GH_TOKEN`) mesmo contra um repositório
público — os passos acima funcionam sem instalar nada além de `cosign` e
`python-snappy`, e sem conta nenhuma. Para quem já tem o `gh` autenticado
por outro motivo, é equivalente e mais rápido.

## O que isto NÃO cobre

- **Assinatura Authenticode** (reputação de editor, SmartScreen) — sistema
  separado, ainda não existe. A procedência aqui prova "este binário saiu
  deste código"; a assinatura, quando existir, prova "o Windows confia no
  editor". Ver `CODE_SIGNING_POLICY.md`.
- **Artefatos construídos localmente** — nunca têm atestado, e não devem ser
  distribuídos.
