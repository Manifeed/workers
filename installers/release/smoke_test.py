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

        for family in ("desktop", "rss", "embedding"):
            run_family_smoke_test(workers_dir, helper_tmp, family)
    finally:
        stack.close()

    print("release-workers.sh dry-run smoke tests passed for desktop, rss and embedding")
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

    if family == "embedding" and release_platform() == "linux" and release_arch() == "x86_64":
        runtime_bundles = sorted(item.get("runtime_bundle") for item in items)
        assert runtime_bundles == ["cuda12", "none"], runtime_bundles


def materialize_fixture_artifacts(
    workers_dir: Path,
    helper_tmp: Path,
    stack: ExitStack,
    current_platform: str,
    current_arch: str,
) -> None:
    desktop_manifest = load_manifest(workers_dir / "worker-desktop" / "Cargo.toml")
    rss_manifest = load_manifest(workers_dir / "worker-rss" / "Cargo.toml")
    embedding_manifest = load_manifest(workers_dir / "worker-source-embedding" / "Cargo.toml")

    desktop_version = resolve_artifact_version(desktop_manifest, current_platform, current_arch)
    rss_version = resolve_artifact_version(rss_manifest, current_platform, current_arch)
    embedding_version = resolve_artifact_version(embedding_manifest, current_platform, current_arch)

    if current_platform == "linux":
        deb_arch = "amd64" if current_arch == "x86_64" else "arm64"
        register_fixture_file(
            workers_dir / "dist" / "debian" / f"manifeed-workers-desktop_{desktop_version}-1_{deb_arch}.deb",
            f"desktop-{desktop_version}-{deb_arch}".encode("utf-8"),
            helper_tmp,
            stack,
        )
    else:
        register_fixture_file(
            workers_dir / "dist" / "macos" / "Manifeed Workers.dmg",
            f"desktop-{desktop_version}-dmg".encode("utf-8"),
            helper_tmp,
            stack,
        )

    register_fixture_file(
        workers_dir
        / "dist"
        / "bundles"
        / current_platform
        / f"rss_worker_bundle-{rss_version}-{current_platform}-{current_arch}.tar.gz",
        f"rss-{rss_version}-{current_platform}-{current_arch}".encode("utf-8"),
        helper_tmp,
        stack,
    )

    embedding_bundles = ["none"]
    if current_platform == "linux" and current_arch == "x86_64":
        embedding_bundles.append("cuda12")
    if current_platform == "macos":
        embedding_bundles.append("coreml")

    for runtime_bundle in embedding_bundles:
        register_fixture_file(
            workers_dir
            / "dist"
            / "bundles"
            / current_platform
            / f"embedding_worker_bundle-{embedding_version}-{current_platform}-{current_arch}-{runtime_bundle}.tar.gz",
            f"embedding-{embedding_version}-{runtime_bundle}".encode("utf-8"),
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
