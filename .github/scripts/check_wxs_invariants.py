#!/usr/bin/env python3
import sys
import xml.etree.ElementTree as ET

NS = {"w": "http://wixtoolset.org/schemas/v4/wxs"}


def fail(msg: str) -> None:
    print(f"::error::{msg}")
    sys.exit(1)


def main() -> None:
    if len(sys.argv) != 2:
        fail("uso: check_wxs_invariants.py <caminho para o .wxs>")

    path = sys.argv[1]
    tree = ET.parse(path)
    root = tree.getroot()

    package = root.find(".//w:Package", NS)
    if package is None:
        fail("elemento <Package> não encontrado")
    scope = package.get("Scope")
    if scope != "perUser":
        fail(
            f"Package/@Scope é {scope!r}, esperado 'perUser' (FR-001/FR-003) — "
            "instalação por usuário é o que garante nenhuma elevação."
        )

    for tag in ("ServiceInstall", "ServiceControl"):
        if root.find(f".//w:{tag}", NS) is not None:
            fail(
                f"elemento <{tag}> encontrado — o duplicata MUST NOT ser "
                "registrado como serviço do Windows (FR-006)."
            )

    rm_disabled = any(
        prop.get("Id") == "MSIRESTARTMANAGERCONTROL" and prop.get("Value") == "Disable"
        for prop in root.findall(".//w:Property", NS)
    )
    if not rm_disabled:
        fail(
            "Property Id=MSIRESTARTMANAGERCONTROL Value=Disable ausente "
            "(FR-024e) — reverter isto reabre o vazamento que o requisito "
            "existe para impedir (ver research.md R2)."
        )

    major_upgrade = root.find(".//w:MajorUpgrade", NS)
    if major_upgrade is None:
        fail("elemento <MajorUpgrade> ausente (FR-021a)")
    if major_upgrade.get("AllowDowngrades") != "no":
        fail(
            f"MajorUpgrade/@AllowDowngrades é {major_upgrade.get('AllowDowngrades')!r}, "
            "esperado 'no' (FR-021a) — sem isso um downgrade acidental pode "
            "deixar o histórico protegido ilegível."
        )

    print("OK: Package/@Scope=perUser, nenhum ServiceInstall/ServiceControl, "
          "Restart Manager desabilitado, MajorUpgrade recusa downgrade.")


if __name__ == "__main__":
    main()
