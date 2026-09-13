# Code Signing Policy

## Project

- **Name:** Duplicata
- **Repository:** https://github.com/RoaniPires/duplicata (public)
- **License:** MIT OR Apache-2.0
- **Description:** A Windows clipboard history manager. Runs locally only —
  no network access, no telemetry, no account.

## Build process

- Release artifacts (a Windows `.msi` installer) are built exclusively by
  the project's GitHub Actions workflow
  (`.github/workflows/release.yml`), running on GitHub-hosted
  `windows-latest` runners.
- The workflow compiles the application with `cargo build --release`, then
  packages the resulting binary into the `.msi` with the WiX Toolset.
- Binaries or installers built on a contributor's own machine are never
  distributed and are never submitted for signing.

## Release process

- A release build is triggered only by pushing a git tag matching `v*`
  (e.g. `v0.1.1`) to the repository.
- Before packaging, the workflow compares the version in the pushed tag
  against the version declared in the project's `Cargo.toml`. If the two
  don't match, the workflow fails and no artifact is produced.
- The installer's version is derived from that same `Cargo.toml` value, so
  there is a single source of truth for the version number.
- Each release build produces a GitHub Actions build provenance attestation
  (`actions/attest-build-provenance`), cryptographically linking the
  published `.msi` to the exact commit and workflow run that produced it.
  Step-by-step verification instructions are published in `SECURITY.md` in
  this repository.

## Authorization

- The only maintainer authorized to push release tags and publish releases
  is the repository owner, RoaniPires.

## Current signing status

- Release artifacts are not currently code-signed. This policy is submitted
  as part of the project's application for free code signing through the
  SignPath Foundation.
