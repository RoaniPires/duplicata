# Instalador (WiX) — build local

**Este build local NÃO é distribuível.** O único artefato que pode ser
distribuído (restrita ou publicamente) é o produzido pela integração
contínua a partir de um commit do repositório público (FR-005c) — ver
`.github/workflows/release.yml` e `specs/005-install-and-distribution/research.md` R4/R7.
Um MSI gerado localmente não tem procedência verificável nem pode ser
submetido à assinatura.

## Pré-requisitos

- [WiX Toolset](https://wixtoolset.org/) como ferramenta de build
  (`dotnet tool install --global wix`). Ferramenta de build, não dependência
  de runtime do aplicativo.

  ⚠️ **A partir do WiX v6, existe o Open Source Maintenance Fee (OSMF)**, e o
  v7 exige aceitar um EULA (`wix eula accept wix7` ou `--acceptEula`) antes
  de compilar qualquer coisa. Isso **não afeta o MSI gerado nem sua
  distribuição** (a cobrança é só sobre o binário pré-compilado da
  ferramenta, para uso comercial acima de US$10.000/ano de receita) — mas é
  decisão do mantenedor aceitar o EULA, fixar uma versão anterior sem essa
  exigência (`--version 5.0.2`, sem patch desde abr/2025) ou reavaliar a
  ferramenta. Ver a pendência registrada em `research.md`, logo após a R1.
- Extensão utilitária do WiX (`WixShellExec`, usada para lançar o app sem
  herdar elevação — FR-002a): precisa estar **adicionada ao cache local**
  antes do primeiro build, uma vez por máquina —
  `wix extension add WixToolset.Util.wixext`. Sem esse passo, `wix build`
  falha com `WIX0144: The extension 'WixToolset.Util.wixext' could not be
  found` — foi o primeiro erro real encontrado ao validar este `.wxs`
  (2026-09-12). Só depois disso o `-ext WixToolset.Util.wixext` na hora do
  build funciona.
- O binário `duplicata.exe` já compilado (`cargo build --release -p duplicata-app`).

## Build local (para desenvolvimento/depuração do `.wxs`)

```powershell
# uma vez por máquina:
wix extension add WixToolset.Util.wixext

wix build installer\duplicata.wxs `
  -ext WixToolset.Util.wixext `
  -d DuplicataExePath=..\target\release\duplicata.exe `
  -o installer\bin\duplicata.msi
```

## Estrutura

- `duplicata.wxs` — definição do pacote MSI. `UpgradeCode` é **fixo para
  sempre**; nunca regenerar (research.md R6, FR-021a/b).
- `Scope="perUser"` — instalação por usuário, sem elevação (FR-001,
  research.md R1).
- Instala em `%LocalAppData%\Programs\duplicata\duplicata.exe`.
- `MSIRESTARTMANAGERCONTROL="Disable"` — Restart Manager desligado (FR-024e).
- `WixShellExec` lança o app ao fim da instalação, sem herdar elevação do
  instalador (FR-002/FR-002a) — ver o comentário no `.wxs` sobre o que ainda
  não está verificado nesse caminho.

## O que ainda falta (fases posteriores da Fatia 5)

Registro de desinstalação com diálogo de escolha de dados, entrada de
inicialização automática, `MajorUpgrade`/recusa de downgrade — ver
`specs/005-install-and-distribution/tasks.md`, fases 4–6 (US2–US4).
