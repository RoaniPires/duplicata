# Instalador (WiX) — build local

**Este build local NÃO é distribuível.** O único artefato que pode ser
distribuído (restrita ou publicamente) é o produzido pela integração
contínua a partir de um commit do repositório público — ver
`.github/workflows/release.yml`.
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
  ferramenta.
- Extensão utilitária do WiX (`WixShellExec`, usada para lançar o app sem
  herdar elevação): precisa estar **adicionada ao cache local**
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

## Verificar as invariantes do `.wxs`

Depois de editar o `duplicata.wxs`, e antes do push, rode na raiz do
repositório (só precisa da toolchain Rust, em Windows ou Linux):

```powershell
cargo run -p duplicata-core --example wxs_invariants -- installer/duplicata.wxs
```

O comando confere seis regras:

1. Existe `<Package>` com `Scope="perUser"`: a instalação é por usuário e não
   pede elevação de administrador.
2. Não há `<ServiceInstall>` nem `<ServiceControl>` em lugar nenhum: o
   duplicata não é registrado como serviço do Windows.
3. Existe `<Property Id="MSIRESTARTMANAGERCONTROL" Value="Disable">`: o
   Restart Manager fica desligado, e o instalador não fecha o duplicata como
   fim de sessão (o que deixaria a cópia de trabalho do histórico em texto
   claro no disco).
4. Existe `<MajorUpgrade>` com `AllowDowngrades="no"`: ninguém instala uma
   versão mais antiga por cima e deixa o histórico protegido ilegível.
5. Existe `<Media>` com `EmbedCab="yes"`: o cabinet vai dentro do `.msi`, e
   baixar só o `.msi` da release basta para instalar.
6. Existe `<Property Id="ARPPRODUCTICON">` com um `Value` que é o `Id` de algum
   `<Icon>` declarado: o duplicata aparece com o próprio ícone em Aplicativos
   instalados.

Os elementos são procurados no namespace do WiX v4, e os valores são
comparados exatamente (`PerUser` não é `perUser`). Com tudo certo, o comando
imprime uma linha `OK: ...` com as seis regras e sai com 0. Cada regra
quebrada vira uma linha `::error::<mensagem>` (o formato de anotação do GitHub
Actions), na ordem acima, e o comando sai com falha depois de listar todas.
Arquivo inexistente, XML inválido ou argumento errado também saem com uma
linha `::error::` e falha.

O mesmo comando roda no job `wxs-invariants` do CI. Além dele, o teste
`crates/duplicata-core/tests/wxs_invariants.rs` aplica a verificação ao
`duplicata.wxs` versionado em todo `cargo test --workspace`, mostrando as
mesmas mensagens quando falha. O comando é um *example* do `duplicata-core`
(`crates/duplicata-core/examples/wxs_invariants/`): o leitor de XML que ele
usa (`roxmltree`) é só dependência de desenvolvimento e não entra no build do
`duplicata.exe`.

## Estrutura

- `duplicata.wxs` — definição do pacote MSI. `UpgradeCode` é **fixo para
  sempre**; nunca regenerar: é por ele que o `MajorUpgrade` reconhece a versão
  instalada como a mesma aplicação e faz a atualização por cima. Com outro
  `UpgradeCode`, a versão anterior não é encontrada e as duas ficam instaladas
  lado a lado.
- `Scope="perUser"` — instalação por usuário: o instalador não pede elevação
  de administrador.
- Instala em `%LocalAppData%\Programs\duplicata\duplicata.exe`.
- `MSIRESTARTMANAGERCONTROL="Disable"` — Restart Manager desligado: ligado,
  ele faria o instalador fechar o duplicata como fim de sessão, e o app pularia
  a consolidação do banco e a re-encriptação, deixando a cópia de trabalho do
  histórico em texto claro no disco.
- `WixShellExec` lança o app ao fim da instalação, sem herdar elevação do
  instalador — ver o comentário no `.wxs` sobre o que ainda
  não está verificado nesse caminho.
