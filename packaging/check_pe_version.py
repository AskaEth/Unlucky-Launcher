#!/usr/bin/env python3
"""Assert the PE **version resource is really inside the artifact** (M190).

Writing `CompanyName` into `build.rs` proves nothing — this reads the produced `.exe` bytes and
looks for the `VS_VERSIONINFO` structure (`FIXEDFILEINFO.dwSignature = 0xFEEF04BD`, key strings in
UTF-16LE) plus the exact values Windows "Properties → Details" must show:

    CompanyName       Unlucky Launcher Project
    ProductName       Unlucky Launcher
    FileDescription   Unlucky Launcher
    LegalCopyright    © 2026 Unlucky Launcher Project · Apache-2.0
    FileVersion       <Cargo.toml version, four parts>

It is deliberately a *byte* check, not a "did the build script run" check: a resource that got lost
in linking still fails here. `--expect-version` also compares `dwFileVersionMS/LS` with the crate
version, so "version drifted from Cargo.toml" is caught.

    python3 packaging/check_pe_version.py <exe>... --expect-version 0.2.3
    python3 packaging/check_pe_version.py --self-test
"""

from __future__ import annotations

import argparse
import struct
import sys
from pathlib import Path

PE_MAGIC = bytes([0x50, 0x45, 0, 0])  # "PE" + two NUL
FIXED_SIGNATURE = b"\xbd\x04\xef\xfe"  # FIXEDFILEINFO.dwSignature = 0xFEEF04BD
PE_VERSION_SOURCE = Path(__file__).resolve().parent / "pe_version.rs"

# 承诺的署名（与 packaging/pe_version.rs 同一份口径；`--self-test` 解析那个文件对拍）。
EXPECTED_STRINGS = {
    "CompanyName": "Unlucky Launcher Project",
    "ProductName": "Unlucky Launcher",
    "FileDescription": "Unlucky Launcher",
    "LegalCopyright": "© 2026 Unlucky Launcher Project · Apache-2.0",
}


def utf16(text: str) -> bytes:
    return text.encode("utf-16-le")


def four_part(version: str) -> tuple[int, int, int, int]:
    """`v0.2.3` / `0.2.3-beta.1` → `(0, 2, 3, 0)`（与 `pe_version.rs::four_part` 同规则）。"""
    numeric = version.split("-")[0].split("+")[0].strip().lstrip("v")
    parts = [0, 0, 0, 0]
    for index, piece in enumerate(numeric.split(".")[:4]):
        try:
            parts[index] = min(int(piece), 0xFFFF)
        except ValueError:
            parts[index] = 0
    return (parts[0], parts[1], parts[2], parts[3])


def pe_subsystem(data: bytes) -> int | None:
    """可选头里的 `Subsystem`（2 = GUI，3 = 控制台）；PE 头不对时返回 None。"""
    if len(data) < 0x40 or data[:2] != b"MZ":
        return None
    offset = struct.unpack_from("<I", data, 0x3C)[0]
    if offset + 0x5C > len(data) or data[offset : offset + 4] != PE_MAGIC:
        return None
    return struct.unpack_from("<H", data, offset + 24 + 0x44)[0]


def check(path: Path, expect_version: str | None) -> list[str]:
    """返回没过的判据（空 = 通过）。"""
    failures: list[str] = []
    data = path.read_bytes()
    if len(data) < 0x100:
        return [f"{path}: 文件太小（{len(data)} 字节），不像 PE"]

    subsystem = pe_subsystem(data)
    if subsystem is None:
        return [f"{path}: 不是可解析的 PE（MZ / PE 头不对）"]

    has_resource = FIXED_SIGNATURE in data
    if not has_resource:
        failures.append(f"{path}: 找不到 FIXEDFILEINFO 签名 0xFEEF04BD —— 这个 exe 里**没有版本资源**")

    for key in ("VS_VERSION_INFO", "StringFileInfo", "VarFileInfo", "Translation"):
        if utf16(key) not in data:
            failures.append(f"{path}: 版本资源里缺结构键 `{key}`（UTF-16LE 未命中）")

    for field, value in EXPECTED_STRINGS.items():
        if utf16(field) not in data:
            failures.append(f"{path}: 缺字段名 `{field}`")
        if utf16(value) not in data:
            failures.append(f"{path}: 字段 `{field}` 的值不是 `{value}`")

    if utf16(f"{path.stem}.exe") not in data:
        failures.append(f"{path}: `OriginalFilename` 不是 `{path.stem}.exe`")

    if expect_version:
        parts = four_part(expect_version)
        dotted = ".".join(str(part) for part in parts)
        if utf16(dotted) not in data:
            failures.append(f"{path}: 版本字符串 `{dotted}` 不在资源里（与 Cargo.toml 不同源？）")
        index = data.find(FIXED_SIGNATURE)
        if index >= 0 and index + 16 <= len(data):
            ms, ls = struct.unpack_from("<II", data, index + 8)
            actual = (ms >> 16, ms & 0xFFFF, ls >> 16, ls & 0xFFFF)
            if actual != parts:
                failures.append(f"{path}: 数值版本 {actual} 与期望 {parts} 不一致（dwFileVersion）")
        else:
            failures.append(f"{path}: FIXEDFILEINFO 之后读不到 dwFileVersion")

    print(
        f"{path.name}: {len(data)} 字节 / subsystem={subsystem}（2=GUI 3=控制台）"
        f" / 版本资源={'有' if has_resource else '无'}"
    )
    return failures


def self_test() -> int:
    """口径表与 `pe_version.rs` 对拍 + 版本换算 + 「假 PE 必须红」。"""
    import os
    import tempfile

    failures = 0
    source = PE_VERSION_SOURCE.read_text(encoding="utf-8")
    for field, value in (
        ("COMPANY_NAME", EXPECTED_STRINGS["CompanyName"]),
        ("PRODUCT_NAME", EXPECTED_STRINGS["ProductName"]),
        ("COPYRIGHT", EXPECTED_STRINGS["LegalCopyright"]),
    ):
        if f'pub const {field}: &str = "{value}";' not in source:
            print(f"FAIL: pe_version.rs 里找不到 {field} = {value!r}", file=sys.stderr)
            failures += 1

    for case, expect in (("0.2.3", (0, 2, 3, 0)), ("v1.2.3-beta.4", (1, 2, 3, 0)), ("1.2.3.4", (1, 2, 3, 4))):
        if four_part(case) != expect:
            print(f"FAIL: four_part({case!r}) = {four_part(case)}，期望 {expect}", file=sys.stderr)
            failures += 1

    # 判据本身要能变红：造一个「像 PE 但没有版本资源」的最小文件，check() 必须报错
    # （否则这个脚本对任何输入都会「绿」，门禁等于没有）。
    fake = bytearray(0x200)
    fake[0:2] = b"MZ"
    struct.pack_into("<I", fake, 0x3C, 0x40)
    fake[0x40:0x44] = PE_MAGIC
    struct.pack_into("<H", fake, 0x40 + 24 + 0x44, 2)
    with tempfile.NamedTemporaryFile(suffix=".exe", delete=False) as handle:
        handle.write(bytes(fake))
        name = handle.name
    problems = check(Path(name), "0.2.3")
    os.unlink(name)
    if not problems:
        print("FAIL: 没有版本资源的假 PE 竟然通过了（判据失效）", file=sys.stderr)
        failures += 1

    print("self-test:", "ok" if failures == 0 else f"{failures} 处失败")
    return 0 if failures == 0 else 1


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="读产物字节断言 PE 版本资源（M190）")
    parser.add_argument("artifacts", nargs="*", type=Path, help="要检查的 exe")
    parser.add_argument("--expect-version", help="与 Cargo.toml 同源的版本号（如 0.2.3）")
    parser.add_argument("--self-test", action="store_true", help="自检（口径表 + 版本换算 + 假 PE 必红）")
    args = parser.parse_args(argv)

    if args.self_test:
        return self_test()
    if not args.artifacts:
        parser.print_help()
        return 2

    failures: list[str] = []
    for artifact in args.artifacts:
        if not artifact.is_file():
            failures.append(f"{artifact}: 文件不存在")
            continue
        failures.extend(check(artifact, args.expect_version))

    for line in failures:
        print(f"error: {line}", file=sys.stderr)
    if failures:
        print(f"FAIL：{len(failures)} 条判据没过（版本资源没进产物就是没进，构建日志不算证据）", file=sys.stderr)
        return 1
    print(f"ok: {len(args.artifacts)} 个产物的版本资源齐全")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
