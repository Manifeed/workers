#!/usr/bin/env python3
from __future__ import annotations

import json
import platform
import shutil
import subprocess
import tempfile
import tomllib
from contextlib import ExitStack
from pathlib import Path


def main() -> int:
    workers_dir = Path(__file__).resolve().parents[2]
    helper_tmp = Path(tempfile.mkdtemp(prefix="manifeed-release-smoke-"))
    stack = ExitStack()
    stack.callback(lambda: shutil.rmtree(helper_tmp, ignore_errors=True))

    try:
        current_platform = release_platform()
        current_arch = release_arch()
        materialize_fixture_artifacts(workers_dir, helper_tmp, stack, current_platform, current_arch)

        for family in ("rss",):
            run_family_smoke_test(workers_dir, helper_tmp, family)
    finally:
        stack.close()

    print("release-workers.sh dry-run smoke tests passed for rss")
    return 0


def run_family_smoke_test(workers_dir: Path, helper_tmp: Path, family: str) -> None:
    family_dir = helper_tmp / family
    storage_root = family_dir / "storage"
    catalog_path = family_dir / "catalog.json"
    subprocess.run(
        [
            "./installers/release-workers.sh",
            "--dry-run",
            "--skip-build",
            "--family",
            family,
            "--storage-root",
            str(storage_root),
            "--catalog-path",
            str(catalog_path),
        ],
        cwd=workers_dir,
        check=True,
    )

    payload = json.loads(catalog_path.read_text(encoding="utf-8"))
    items = [item for item in payload.get("items", []) if item.get("family") == family]
    if not items:
        raise AssertionError(f"no catalog items generated for family={family}")


def materialize_fixture_artifacts(
    workers_dir: Path,
    helper_tmp: Path,
    stack: ExitStack,
    current_platform: str,
    current_arch: str,
) -> None:
    rss_manifest = load_manifest(workers_dir / "crawler_rss" / "Cargo.toml")

    rss_version = resolve_artifact_version(rss_manifest, current_platform, current_arch)

    register_fixture_file(
        workers_dir
        / "dist"
        / "bundles"
        / current_platform
        / f"crawler_rss_bundle-{rss_version}-{current_platform}-{current_arch}.tar.gz",
        f"rss-{rss_version}-{current_platform}-{current_arch}".encode("utf-8"),
        helper_tmp,
        stack,
    )


def register_fixture_file(
    target_path: Path,
    payload: bytes,
    helper_tmp: Path,
    stack: ExitStack,
) -> None:
    backup_path = helper_tmp / "backups" / target_path.name
    backup_path.parent.mkdir(parents=True, exist_ok=True)

    if target_path.exists():
        shutil.copy2(target_path, backup_path)
        stack.callback(lambda: shutil.copy2(backup_path, target_path))
    else:
        stack.callback(lambda: target_path.unlink(missing_ok=True))

    target_path.parent.mkdir(parents=True, exist_ok=True)
    target_path.write_bytes(payload)


def load_manifest(path: Path) -> dict:
    return tomllib.loads(path.read_text(encoding="utf-8"))


def resolve_artifact_version(manifest: dict, release_platform_value: str, release_arch_value: str) -> str:
    release_metadata = (
        manifest.get("package", {})
        .get("metadata", {})
        .get("manifeed", {})
        .get("release", {})
    )
    override_key = f"artifact_version_{release_platform_value}_{release_arch_value}"
    return str(release_metadata.get(override_key) or manifest["package"]["version"])


def release_platform() -> str:
    return "macos" if platform.system().lower() == "darwin" else "linux"


def release_arch() -> str:
    machine = platform.machine().lower()
    if machine in {"x86_64", "amd64"}:
        return "x86_64"
    if machine in {"arm64", "aarch64"}:
        return "aarch64"
    raise RuntimeError(f"unsupported architecture for smoke test: {machine}")


if __name__ == "__main__":
    raise SystemExit(main())
