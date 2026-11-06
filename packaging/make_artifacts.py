#!/usr/bin/env python3
"""Build the single-file release artifacts for the current platform.

Release matrix (see `packaging/README.md` for the full contract):

    Windows x64     unlucky-launcher_<ver>_windows_x64.exe      (bare exe, no zip)
    Windows arm64   unlucky-launcher_<ver>_windows_arm64.exe    (bare exe, cross-built)
    Linux x64       unlucky-launcher_<ver>_linux_x64.tar.gz
    Linux arm64     unlucky-launcher_<ver>_linux_arm64.tar.gz
    macOS arm64     unlucky-launcher_<ver>_macos_arm64.tar.gz
    macOS x64       unlucky-launcher_<ver>_macos_x64.tar.gz
    every slot      ull_<ver>_<platform>_<arch>[.exe]           (bare CLI binary, no archive)
    all             SHA256SUMS.txt

Naming rule (user's call, 2026-11-05): `<name>_<version>_<platform>_<arch>`, underscore
separated, and **no implementation detail in the file name** — the asset never says which UI
framework produced it. `asset_stem()` is the single source of truth for that.

`--version` is taken from the cargo workspace so a tag can never drift away from
`Cargo.toml`; pass `--version` to cross-check a tag against it (mismatch is fatal).
The archives carry the launcher binary plus LICENSE / NOTICE / README.md, which is
what the license obligations ask for; Windows ships the bare `.exe` (the single-file
selling point) and the same three documents are attached to the Release next to it.

The CLI ships as its **own bare asset** on every platform rather than inside the tar.gz:
the point of `ull` is a small single file you download and run on a machine with no
display, and an archive it has to be extracted from adds a step for that user. The
launcher is the archive because it needs the license texts beside it; the Release
carries LICENSE / NOTICE / README.md anyway, so a bare `ull` loses nothing.

The launcher binary also answers every CLI subcommand, so `ull` is not a second
interface — it is the version of it that links no graphics stack at all.

Slot and build directory are overridable because a cross build's binaries live in
`target/<triple>/release` and the build host (x64) is not the artifact (arm64): CI
passes both from its matrix, and the executable header is read back so a binary can
never be shipped under a slot name it was not built for.

Usage:
    cargo build --release --locked
    python3 packaging/make_artifacts.py [--version v0.2.0] [--out dist] [--no-smoke]
    # cross build (arm64 exe from an x64 host):
    python3 packaging/make_artifacts.py --slot windows-arm64 \\
        --build-dir target/aarch64-pc-windows-msvc/release
"""

from __future__ import annotations

import argparse
import gzip
import hashlib
import json
import platform
import shutil
import subprocess
import sys
import tarfile
from pathlib import Path
from typing import NoReturn

# M118（D26）：GUI 的**构建产物**名是 iced 版的 `unlucky-launcher-iced`。旧值 `unlucky-launcher`
# 是 `crates/app`（Slint 退役树）的 bin —— 那棵树已被根 Cargo.toml `exclude`，再也不产出它了。
# **对外**文件名仍叫 `unlucky-launcher`：下载链接 / SHA256SUMS / README 都绑着这个名字，
# 改名是「翻牌」那一步的事，不是打包这一步的事。所以刻意拆成两个常量：
# 找文件用 LAUNCHER_BIN，落盘（tar 内文件名 / exe 资产名）用 LAUNCHER_ASSET。
LAUNCHER_BIN = "unlucky-launcher-iced"
LAUNCHER_ASSET = "unlucky-launcher"
CLI_BIN = "ull"
# 随包分发的合规文本（仓库根目录）；英文主版 README 是对外分发口径。
COMPLIANCE_DOCS = ("LICENSE", "NOTICE", "README.md")
SUMS_NAME = "SHA256SUMS.txt"
DEFAULT_BUILD_DIR = "target/release"

# 发布矩阵：槽位 -> (OS 家族, 架构, 产物形态)。键名即产物文件名里的槽位段，
# 单一真相源是 packaging/README.md §1；未知槽位一律拒绝，不出"凑合能跑"的产物。
SLOTS = {
    "windows-x64": ("windows", "x64", "exe"),
    "windows-arm64": ("windows", "arm64", "exe"),
    "linux-x64": ("linux", "x64", "tar"),
    "linux-arm64": ("linux", "arm64", "tar"),
    "macos-arm64": ("macos", "arm64", "tar"),
    "macos-x64": ("macos", "x64", "tar"),
}
# `sys.platform` / `platform.machine()` 的别名归一（aarch64 与 arm64 是同一架构的两种叫法）。
OS_FAMILIES = (("win", "windows"), ("linux", "linux"), ("darwin", "macos"))
MACHINE_ALIASES = {"amd64": "x64", "x86_64": "x64", "aarch64": "arm64", "arm64": "arm64"}
# 可执行文件头里的机器码 -> 架构（PE / ELF / Mach-O）。
IMAGE_ARCH = {
    ("pe", 0x8664): "x64",
    ("pe", 0xAA64): "arm64",
    ("elf", 0x3E): "x64",
    ("elf", 0xB7): "arm64",
    ("macho", 0x01000007): "x64",
    ("macho", 0x0100000C): "arm64",
}

REPO_ROOT = Path(__file__).resolve().parent.parent


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


def host_slot(os_name: str | None = None, machine: str | None = None) -> str:
    """Map the build host to a matrix slot.

    Both arguments are injectable so the mapping can be exercised for a host we do not
    own (no ARM machine here); they default to the real ones.
    """
    os_name = (sys.platform if os_name is None else os_name).lower()
    raw_machine = platform.machine() if machine is None else machine
    arch = MACHINE_ALIASES.get(raw_machine.lower())
    family = next((fam for prefix, fam in OS_FAMILIES if os_name.startswith(prefix)), None)
    for slot, (slot_os, slot_arch, _) in SLOTS.items():
        if (slot_os, slot_arch) == (family, arch):
            return slot
    # 报错带**原始**取值而不是归一后的 None：`riscv64` 这类不认识的架构得在诊断里看得见。
    fail(f"unsupported build host {os_name}/{raw_machine}; matrix is {' / '.join(SLOTS)}")


def binary(name: str, build_dir: Path, family: str) -> Path:
    path = build_dir / (f"{name}.exe" if family == "windows" else name)
    if not path.is_file():
        fail(f"missing {path}; run `cargo build --release --locked` first")
    return path


def image_arch(path: Path) -> str | None:
    """Architecture stamped into the executable header, or `None` if unreadable."""
    with path.open("rb") as handle:
        head = handle.read(0x40)
        if head[:4] == b"\x7fELF":
            order = "little" if head[5] == 1 else "big"
            return IMAGE_ARCH.get(("elf", int.from_bytes(head[18:20], order)))
        if head[:2] == b"MZ":
            handle.seek(int.from_bytes(head[0x3C:0x40], "little"))
            pe = handle.read(6)
            if pe[:4] == b"PE\0\0":
                return IMAGE_ARCH.get(("pe", int.from_bytes(pe[4:6], "little")))
            return None
        if head[:4] == b"\xcf\xfa\xed\xfe":  # thin 64-bit little-endian Mach-O
            return IMAGE_ARCH.get(("macho", int.from_bytes(head[4:8], "little")))
    return None


def verify_arch(path: Path, slot: str) -> None:
    """Prove the binary really is built for the slot it is about to be named after."""
    want = SLOTS[slot][1]
    got = image_arch(path)
    if got is None:
        fail(f"cannot read the machine type of {path}; refusing to ship an unverified binary")
    if got != want:
        fail(f"{path} is a {got} binary but slot {slot} says {want}")
    print(f"arch:     {path.name} -> {got} (ok for {slot})")


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
    print(f"smoke:    {cli.name} --version -> {version} (ok)")


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


def asset_stem(name: str, version: str, slot: str) -> str:
    """`<name>_<version>_<platform>_<arch>` — 产物命名的唯一真相源。

    用户令（2026-11-05）：产物名按 `名字_版本号_平台_架构` 下划线分段，且**不得出现 `iced`
    这类实现细节字样** —— 下载的人不该关心我们内部用哪个 UI 框架。槽位键（`windows-x64`）
    仍是脚本内部的写法，落到文件名时拆成 `windows_x64` 两段。
    """
    family, arch, _kind = SLOTS[slot]
    return f"{name}_{version}_{family}_{arch}"


def stage_cli(out_dir: Path, version: str, slot: str, cli: Path) -> Path:
    """Copy the CLI out as its own bare asset (`ull_<ver>_<platform>_<arch>[.exe]`)."""
    name = asset_stem(CLI_BIN, version, slot) + (".exe" if cli.suffix == ".exe" else "")
    asset = out_dir / name
    shutil.copy2(cli, asset)
    # 裸文件下载即可运行：Windows 宿主机上源文件没有 x 位，这里补上（UNIX 侧才有意义）。
    asset.chmod(0o755)
    return asset


def stage_archive(out_dir: Path, tag: str, version: str, launcher: Path) -> Path:
    """tar.gz with one top-level dir holding the binary + compliance docs."""
    stem = asset_stem(LAUNCHER_ASSET, version, tag)
    stage = out_dir / stem
    if stage.exists():
        shutil.rmtree(stage)
    stage.mkdir(parents=True)
    shutil.copy2(launcher, stage / LAUNCHER_ASSET)
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
            info.mode = 0o755 if entry.name == LAUNCHER_ASSET else 0o644
            with entry.open("rb") as handle:
                tar.addfile(info, handle)
    shutil.rmtree(stage)
    return archive


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", help="expected version (a leading 'v' is stripped); cross-checks Cargo.toml")
    parser.add_argument("--out", default="dist", help="output directory (default: dist/)")
    parser.add_argument("--slot", choices=list(SLOTS), help="matrix slot; defaults to the one derived from the host")
    parser.add_argument("--build-dir", default=DEFAULT_BUILD_DIR, help=f"directory holding the built binaries (default: {DEFAULT_BUILD_DIR})")
    parser.add_argument("--no-smoke", action="store_true", help="skip the `ull --version` smoke run")
    args = parser.parse_args()

    version = workspace_version()
    if args.version:
        expected = args.version[1:] if args.version.startswith("v") else args.version
        if expected != version:
            fail(f"tag/version mismatch: requested {expected}, Cargo.toml says {version}")
    native = host_slot()
    slot = args.slot or native
    family, _, kind = SLOTS[slot]
    build_dir = Path(args.build_dir)
    if not build_dir.is_absolute():
        build_dir = REPO_ROOT / build_dir
    launcher = binary(LAUNCHER_BIN, build_dir, family)
    verify_arch(launcher, slot)
    # 跨槽位产物（宿主 x64 / 目标 arm64）在本机根本执行不了，冒烟只能等 CI 真机；
    # 拿"跳过了"当通过是假证据，所以把跳过原因原样打出来。
    if args.no_smoke or slot != native:
        reason = "--no-smoke" if args.no_smoke else f"host is {native}, artifact is {slot}"
        print(f"smoke:    skipped ({reason})")
    else:
        smoke_version(binary(CLI_BIN, build_dir, family), version)

    out_dir = Path(args.out)
    if not out_dir.is_absolute():
        out_dir = REPO_ROOT / out_dir
    out_dir.mkdir(parents=True, exist_ok=True)

    if kind == "exe":
        asset = out_dir / f"{asset_stem(LAUNCHER_ASSET, version, slot)}.exe"
        shutil.copy2(launcher, asset)
    else:
        asset = stage_archive(out_dir, slot, version, launcher)
    # CLI 与 GUI 同一矩阵槽位、各自独立资产：架构同样回读文件头校验，不给 arm64 槽位混进 x64 的 ull。
    cli = binary(CLI_BIN, build_dir, family)
    verify_arch(cli, slot)
    cli_asset = stage_cli(out_dir, version, slot, cli)

    sums = write_sums(out_dir, [asset, cli_asset])
    for item in (asset, cli_asset):
        size_mb = item.stat().st_size / (1024 * 1024)
        print(f"artifact: {item.name}  {size_mb:.2f} MiB  sha256={sha256(item)}")
    print(f"sums:     {sums.name}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
