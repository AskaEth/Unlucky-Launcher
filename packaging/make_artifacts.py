#!/usr/bin/env python3
"""Build the single-file release artifacts for the current platform.

Release matrix (see `packaging/README.md` for the full contract):

    Windows x64   unlucky-launcher-<ver>-windows-x64.exe        (bare exe, no zip)
    Linux x64     unlucky-launcher-<ver>-linux-x64.tar.gz
    macOS arm64   unlucky-launcher-<ver>-macos-arm64.tar.gz
    macOS x64     unlucky-launcher-<ver>-macos-x64.tar.gz
    all           SHA256SUMS.txt

`--version` is taken from the cargo workspace so a tag can never drift away from
`Cargo.toml`; pass `--version` to cross-check a tag against it (mismatch is fatal).
The archives carry the launcher binary plus LICENSE / NOTICE / README.md, which is
what the license obligations ask for; Windows ships the bare `.exe` (the single-file
selling point) and the same three documents are attached to the Release next to it.

Usage:
    cargo build --release --locked
    python3 packaging/make_artifacts.py [--version v0.1.0] [--out dist] [--no-smoke]
"""

from __future__ import annotations

import argparse
import gzip
import hashlib
import json
import os
import platform
import shutil
import subprocess
import sys
import tarfile
from pathlib import Path
from typing import NoReturn

LAUNCHER_BIN = "unlucky-launcher"
CLI_BIN = "ull"
# 随包分发的合规文本（仓库根目录）；英文主版 README 是对外分发口径。
COMPLIANCE_DOCS = ("LICENSE", "NOTICE", "README.md")
SUMS_NAME = "SHA256SUMS.txt"

REPO_ROOT = Path(__file__).resolve().parent.parent
TARGET_DIR = REPO_ROOT / "target" / "release"


def fail(message: str) -> NoReturn:
    print(f"error: {message}", file=sys.stderr)
    raise SystemExit(1)


def workspace_version() -> str:
    """Read `[workspace.package] version` through cargo itself (single source of truth)."""
    try:
        raw = subprocess.run(
            ["cargo", "metadata", "--no-deps", "--format-version", "1"],
            cwd=REPO_ROOT,
            check=True,
            capture_output=True,
            text=True,
            # Windows 默认 GBK：cargo 的 package description 是中文，不显式指定会解码失败。
            encoding="utf-8",
            errors="replace",
        ).stdout
    except (OSError, subprocess.CalledProcessError) as exc:
        fail(f"cargo metadata failed: {exc}")
    packages = json.loads(raw)["packages"]
    versions = {pkg["version"] for pkg in packages}
    if len(versions) != 1:
        fail(f"workspace members disagree on version: {sorted(versions)}")
    return versions.pop()


def platform_tag() -> str:
    """Map the build host to the artifact matrix slot (hard-fail on unsupported ones)."""
    os_name = sys.platform
    machine = platform.machine().lower()
    if os_name.startswith("win") and machine in ("amd64", "x86_64"):
        return "windows-x64"
    if os_name.startswith("linux") and machine in ("x86_64", "amd64"):
        return "linux-x64"
    if os_name == "darwin" and machine in ("arm64", "aarch64"):
        return "macos-arm64"
    if os_name == "darwin" and machine in ("x86_64", "amd64"):
        return "macos-x64"
    fail(f"unsupported build host {os_name}/{machine}; matrix is windows-x64 / linux-x64 / macos-arm64 / macos-x64")


def binary(name: str) -> Path:
    path = TARGET_DIR / (f"{name}.exe" if sys.platform.startswith("win") else name)
    if not path.is_file():
        fail(f"missing {path}; run `cargo build --release --locked` first")
    return path


def smoke_version(cli: Path, version: str) -> None:
    """`ull --version` must report the workspace version (guards tag/version drift)."""
    proc = subprocess.run(
        [str(cli), "--version"],
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    out = (proc.stdout or "") + (proc.stderr or "")
    if proc.returncode != 0:
        fail(f"`{cli.name} --version` exited {proc.returncode}: {out.strip()}")
    if version not in out:
        fail(f"`{cli.name} --version` does not report {version}: {out.strip()}")
    print(f"smoke: {cli.name} --version -> {version} (ok)")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def write_sums(out_dir: Path, assets: list[Path]) -> Path:
    sums = out_dir / SUMS_NAME
    lines = [f"{sha256(path)}  {path.name}" for path in sorted(assets)]
    sums.write_text("\n".join(lines) + "\n", encoding="utf-8", newline="\n")
    return sums


def stage_archive(out_dir: Path, tag: str, version: str, launcher: Path) -> Path:
    """tar.gz with one top-level dir holding the binary + compliance docs."""
    stem = f"unlucky-launcher-{version}-{tag}"
    stage = out_dir / stem
    if stage.exists():
        shutil.rmtree(stage)
    stage.mkdir(parents=True)
    shutil.copy2(launcher, stage / launcher.name)
    for doc in COMPLIANCE_DOCS:
        source = REPO_ROOT / doc
        if not source.is_file():
            fail(f"compliance file {source} is missing")
        shutil.copy2(source, stage / doc)
    # tar 内的 mtime/uid/mode 与 gzip 头的 mtime 全部归零 → 同源构建的 sha256 与打包时刻无关。
    archive = out_dir / f"{stem}.tar.gz"
    with archive.open("wb") as raw, gzip.GzipFile(
        fileobj=raw, mode="wb", compresslevel=9, mtime=0
    ) as gz, tarfile.open(fileobj=gz, mode="w") as tar:
        for entry in sorted(stage.iterdir()):
            info = tar.gettarinfo(str(entry), arcname=f"{stem}/{entry.name}")
            info.mtime = 0
            info.uid = info.gid = 0
            info.uname = info.gname = ""
            info.mode = 0o755 if entry.name == launcher.name else 0o644
            with entry.open("rb") as handle:
                tar.addfile(info, handle)
    shutil.rmtree(stage)
    return archive


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", help="expected version (a leading 'v' is stripped); cross-checks Cargo.toml")
    parser.add_argument("--out", default="dist", help="output directory (default: dist/)")
    parser.add_argument("--no-smoke", action="store_true", help="skip the `ull --version` smoke run")
    args = parser.parse_args()

    version = workspace_version()
    if args.version:
        expected = args.version[1:] if args.version.startswith("v") else args.version
        if expected != version:
            fail(f"tag/version mismatch: requested {expected}, Cargo.toml says {version}")
    tag = platform_tag()
    launcher = binary(LAUNCHER_BIN)
    cli = binary(CLI_BIN)
    if not args.no_smoke:
        smoke_version(cli, version)

    out_dir = Path(args.out)
    if not out_dir.is_absolute():
        out_dir = REPO_ROOT / out_dir
    out_dir.mkdir(parents=True, exist_ok=True)

    if tag == "windows-x64":
        asset = out_dir / f"unlucky-launcher-{version}-{tag}.exe"
        shutil.copy2(launcher, asset)
    else:
        asset = stage_archive(out_dir, tag, version, launcher)

    sums = write_sums(out_dir, [asset])
    size_mb = asset.stat().st_size / (1024 * 1024)
    print(f"artifact: {asset.name}  {size_mb:.2f} MiB  sha256={sha256(asset)}")
    print(f"sums:     {sums.name}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
