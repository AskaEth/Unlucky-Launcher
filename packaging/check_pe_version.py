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

# 什么时候退出非 0（判据清单；排查红步骤时照这张表读日志）

逐产物（退出码 1，每条一条 `error:`）：

1. 文件不存在；
2. 小于 0x100 字节，或不是可解析的 PE（MZ / PE 头不对）；
3. 找不到 `FIXEDFILEINFO` 签名 0xFEEF04BD —— **版本资源根本没进产物**；
4. 缺结构键 `VS_VERSION_INFO` / `StringFileInfo` / `VarFileInfo` / `Translation`；
5. 缺承诺字段名，或字段值不等于承诺值（CompanyName / ProductName / FileDescription / LegalCopyright）；
6. `OriginalFilename` 不是 `<bin>.exe`；
7. 给了 `--expect-version` 时：①四段式版本字符串不在资源里；②`dwFileVersion` 与它不一致；
   ③签名之后不足 16 字节、读不到 `dwFileVersion`（`FIXEDFILEINFO` 被截断）。

其它：`--self-test` 自检失败 → 1；没给产物也没给 `--self-test` → 2（打印用法）。
输出本身**不再是**失败原因：`safe_streams()` 保证「汉字打不出来」不会把一条通过的判据变成红。
"""

from __future__ import annotations

import argparse
import os
import struct
import subprocess
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
        f" / 版本资源={'有' if has_resource else '无'}",
        # 进度行走在 stderr 的判据之前：CI 的 stdout 是块缓冲，不 flush 就会「错误先、摘要后」，读起来像两份日志。
        flush=True,
    )
    return failures


def safe_streams() -> None:
    """把 stdout/stderr 钉成「汉字打不出来也不许抛异常」。

    CI 的 stdout 是**管道**：GitHub Actions 按 UTF-8 解码，而 Windows runner 是 en-US 镜像、默认代码页
    1252，于是 `print("ok: 2 个产物的版本资源齐全")` 会 `UnicodeEncodeError` —— 判据全过也红，而且崩在
    `print` 上，日志里只剩一段 traceback（`::error::` 注解一条都没有）。本机 cp936 能编汉字，复现不出来。
    管道里钉 UTF-8（Actions 那边正好按 UTF-8 解），终端里只放宽 `errors`（不改编码，免得本机控制台变乱码）。
    """
    for stream in (sys.stdout, sys.stderr):
        reconfigure = getattr(stream, "reconfigure", None)  # Python < 3.7 没有，那就只能用默认行为
        if reconfigure is None:
            continue
        try:
            if stream.isatty():
                reconfigure(errors="backslashreplace")
            else:
                reconfigure(encoding="utf-8", errors="backslashreplace")
        except (ValueError, OSError):
            pass


def synthetic_pe(stem: str, version: tuple[int, int, int, int], *, with_resource: bool = True) -> bytes:
    """合成一个 MZ/PE 头合法的最小文件：`with_resource` 决定版本资源齐不齐。

    两种用途：`False` 是**负控**（判据必须报错），`True` 是**正控**（判据全过 —— 否则「绿的时候能不能
    正常打印/正常退出 0」这条根本没法验）。
    """
    pe_offset = 0x40
    body = bytearray(0x100)
    body[0:2] = b"MZ"
    struct.pack_into("<I", body, 0x3C, pe_offset)
    body[pe_offset : pe_offset + 4] = PE_MAGIC
    struct.pack_into("<H", body, pe_offset + 24 + 0x44, 2)  # Subsystem = WINDOWS_GUI
    if not with_resource:
        return bytes(body)

    ms = (version[0] << 16) | version[1]
    ls = (version[2] << 16) | version[3]
    fixed = FIXED_SIGNATURE + struct.pack("<I", 0x00010000) + struct.pack("<II", ms, ls) + struct.pack("<II", ms, ls)
    dotted = ".".join(str(part) for part in version)
    texts = [
        "VS_VERSION_INFO",
        "StringFileInfo",
        "VarFileInfo",
        "Translation",
        "CompanyName",
        EXPECTED_STRINGS["CompanyName"],
        "ProductName",
        EXPECTED_STRINGS["ProductName"],
        "FileDescription",
        EXPECTED_STRINGS["FileDescription"],
        "LegalCopyright",
        EXPECTED_STRINGS["LegalCopyright"],
        dotted,
        f"{stem}.exe",
    ]
    return bytes(body) + fixed + b"".join(utf16(text) for text in texts)


def spawn_with_io_encoding(artifact: Path, expect: str, io_encoding: str) -> subprocess.CompletedProcess[str]:
    """按指定 I/O 编码起一个真子进程跑本脚本（复刻 CI 上的 Windows 管道环境）。"""
    env = {key: value for key, value in os.environ.items() if key not in ("PYTHONUTF8", "PYTHONIOENCODING")}
    env["PYTHONIOENCODING"] = io_encoding
    return subprocess.run(
        [sys.executable, str(Path(__file__).resolve()), str(artifact), "--expect-version", expect],
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        env=env,
        check=False,
    )


def self_test() -> int:
    """口径表与 `pe_version.rs` 对拍 + 版本换算 + 「假 PE 必须红」+ 「管道编码不许崩」。"""
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
    with tempfile.TemporaryDirectory() as work:
        red = Path(work) / "red.exe"
        red.write_bytes(synthetic_pe("red", (0, 2, 3, 0), with_resource=False))
        problems = check(red, "0.2.3")
        if not problems:
            print("FAIL: 没有版本资源的假 PE 竟然通过了（判据失效）", file=sys.stderr)
            failures += 1

        # 正控：同一套判据对「齐全」的合成 PE 必须绿，否则下面的「绿的时候能不能正常退出」无从验起。
        green = Path(work) / "green.exe"
        green.write_bytes(synthetic_pe("green", (0, 2, 3, 0)))
        problems = check(green, "0.2.3")
        if problems:
            print(f"FAIL: 版本资源齐全的合成 PE 竟然报错 {problems}（判据过度）", file=sys.stderr)
            failures += 1

        # 回归（v0.2.4 Release run 43031591576 两个 Windows 槽位红在这里）：CI 的 stdout 是管道，
        # Windows runner 默认代码页 1252 打不出汉字。下面两条用**真子进程 + 真管道**把那个环境复刻出来：
        # 判据全过时必须退 0（不是崩在 print 上），判据没过时必须退 1 且报错文本还在（不许被吞成 traceback）。
        for case, artifact, expect, want in (
            ("cp1252 绿", green, "0.2.3", 0),
            ("cp437 绿", green, "0.2.3", 0),
            ("cp1252 红", red, "0.2.3", 1),
        ):
            result = spawn_with_io_encoding(artifact, expect, "cp1252" if "cp1252" in case else "cp437")
            crashed = "Traceback" in result.stderr or "UnicodeEncodeError" in result.stderr
            if crashed or result.returncode != want:
                print(
                    f"FAIL: {case} 时退出码 {result.returncode}（期望 {want}）"
                    f"{'，且崩在编码上' if crashed else ''}：{result.stderr.strip()[-300:]}",
                    file=sys.stderr,
                )
                failures += 1
            if want == 0 and "ok:" not in result.stdout:
                print(f"FAIL: {case} 时 stdout 没有 ok 行：{result.stdout.strip()[-200:]}", file=sys.stderr)
                failures += 1
            if want == 1 and "error:" not in result.stderr:
                print(f"FAIL: {case} 时报错文本丢了：{result.stderr.strip()[-200:]}", file=sys.stderr)
                failures += 1

    print("self-test:", "ok" if failures == 0 else f"{failures} 处失败")
    return 0 if failures == 0 else 1


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="读产物字节断言 PE 版本资源（M190）")
    parser.add_argument("artifacts", nargs="*", type=Path, help="要检查的 exe")
    parser.add_argument("--expect-version", help="与 Cargo.toml 同源的版本号（如 0.2.3）")
    parser.add_argument("--self-test", action="store_true", help="自检（口径表 + 版本换算 + 假 PE 必红/齐全必绿 + 管道编码）")
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
    safe_streams()
    raise SystemExit(main())
