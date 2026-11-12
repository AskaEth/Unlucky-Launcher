#!/usr/bin/env python3
"""PE 子系统守卫（M196）：**读产物 PE 头的 `Subsystem` 字段**，不许用「代码看起来对」代替。

起因：群友 2026-11-08 实测发布构建「运行时会带一个控制台窗口在底下」。根因是产物 PE 头的
`Subsystem` 仍是 CUI(3)——`grep -rn windows_subsystem crates/` 当时**零命中**。这个修复在
2026-11-01 做过一次（`37d3039`），但打在已退役的 Slint 树（`crates/app`，M118 起被 workspace
`exclude`、不进发布矩阵），所以从未到达 iced 产物。教训不是「忘了属性」，而是**没有任何一步
去看产物本身**：源码层面怎么写都可能被别的树顶替，只有读那 4 个字节是事实。

两向断言（一条也不许静默）：

1. **GUI 产物必须是 `Subsystem == 2`（`WINDOWS_GUI`）**：`unlucky-launcher-iced.exe`。
   是 3 就意味着双击会多一个控制台宿主窗口——正是本 mission 要修的那个 bug。
2. **CLI 产物必须是 `Subsystem == 3`（`CONSOLE`）**：`ull.exe`。它靠真控制台做 stdout/stderr，
   谁把 `windows_subsystem = "windows"` 抄到它头上，`ull --version | …` 就会静默丢输出。
   反方向同样要红：只守一个方向时，把属性贴错 crate 会被漏掉。

读法（零第三方依赖，PE32 与 PE32+ 都吃）：`MZ` → `e_lfanew`(偏移 `0x3C`) → `PE\\0\\0` →
COFF 头 20 字节 → 可选头起点 → `Subsystem` 在可选头 `+0x44`（两种 magic 一致）。
读数含**文件字节偏移**，报告里贴的是真实位置，不是「我读过了」。

用法（本地与 CI 同一份代码）：

    python3 packaging/check_pe_subsystem.py --gui target/release/unlucky-launcher-iced.exe \\
                                            --cli target/release/ull.exe
    python3 packaging/check_pe_subsystem.py --self-test    # 合成 PE 桩，断言该红的红（8 条：红 6 / 绿 2）

`--self-test` 是这条守门的自测证据：它造合成 PE 头（GUI / CUI / 非 MZ / 截断 / 缺文件），
断言「GUI 读成 3 ⇒ 红」「CLI 读成 2 ⇒ 红」「两边都对 ⇒ 绿」——共 **8 条用例（红 6 / 绿 2）**，
另加一条「Subsystem 真落在报出来的那个偏移上」的偏移自证。守门抓不住回归时它自己先红。
"""

from __future__ import annotations

import struct
import sys
import tempfile
from pathlib import Path

#: PE 签名（`e_lfanew` 处那四个字节）。
PE_SIGNATURE = b"PE\0\0"

#: `IMAGE_OPTIONAL_HEADER.Subsystem` 在可选头里的偏移（PE32 与 PE32+ 同一个位置）。
SUBSYSTEM_OFFSET = 0x44
#: 想要的两种取值。
WINDOWS_GUI = 2
CONSOLE = 3

SUBSYSTEM_NAMES = {
    0: "UNKNOWN",
    1: "NATIVE",
    2: "WINDOWS_GUI",
    3: "CONSOLE",
    5: "OS2_CUI",
    7: "POSIX_CUI",
    9: "WINDOWS_CE_GUI",
    10: "EFI_APPLICATION",
    11: "EFI_BOOT_SERVICE_DRIVER",
    12: "EFI_RUNTIME_DRIVER",
    14: "XBOX",
    16: "WINDOWS_BOOT_APPLICATION",
}

#: 可选头 magic → 人话（只用于打印，判据不看它）。
MAGICS = {0x10B: "PE32", 0x20B: "PE32+", 0x107: "ROM"}


class PeError(Exception):
    """读 PE 头失败（不是 PE / 被截断）—— 同样是红的理由，不许当成「跳过」。"""


class PeInfo:
    """一次读数：值 + 它在文件里的位置（位置进报告，是「真读过」的凭据）。"""

    __slots__ = ("path", "subsystem", "file_offset", "e_lfanew", "magic", "machine")

    def __init__(self, path: Path, subsystem: int, file_offset: int, e_lfanew: int, magic: int, machine: int):
        self.path = path
        self.subsystem = subsystem
        self.file_offset = file_offset
        self.e_lfanew = e_lfanew
        self.magic = magic
        self.machine = machine

    @property
    def name(self) -> str:
        return self.path.name

    def describe(self) -> str:
        return (
            f"[pe] {self.name}: Subsystem={self.subsystem} "
            f"({SUBSYSTEM_NAMES.get(self.subsystem, '?')}) @ 文件偏移 0x{self.file_offset:X}"
            f"（e_lfanew=0x{self.e_lfanew:X}，{MAGICS.get(self.magic, f'0x{self.magic:X}')}，"
            f"可选头+0x{SUBSYSTEM_OFFSET:X}，machine=0x{self.machine:X}）"
        )


def read_subsystem(path: Path) -> PeInfo:
    """读 PE 可选头的 `Subsystem`。任何一步对不上都抛 [`PeError`]，绝不返回「猜的值」。"""
    if not path.is_file():
        raise PeError(f"{path} 不存在（构建步没跑？还是产物名改了？）")
    with path.open("rb") as handle:
        dos = handle.read(0x40)
        if len(dos) < 0x40:
            raise PeError(f"{path} 只有 {len(dos)} 字节，连 DOS 头都不够（文件被截断？）")
        if dos[0:2] != b"MZ":
            raise PeError(f"{path} 不是 PE：开头是 {dos[0:2]!r}，不是 b'MZ'")
        e_lfanew = struct.unpack_from("<I", dos, 0x3C)[0]
        handle.seek(e_lfanew)
        coff = handle.read(4 + 20)
        if len(coff) < 24:
            raise PeError(f"{path} 的 e_lfanew=0x{e_lfanew:X} 越界（读不到 PE 签名 + COFF 头）")
        if coff[0:4] != b"PE\0\0":
            raise PeError(f"{path} 在 e_lfanew=0x{e_lfanew:X} 处没有 PE 签名（期望 {PE_SIGNATURE!r}）：{coff[0:4]!r}")
        machine = struct.unpack_from("<H", coff, 4)[0]
        optional_size = struct.unpack_from("<H", coff, 4 + 16)[0]
        optional_start = e_lfanew + 24
        if optional_size < SUBSYSTEM_OFFSET + 2:
            raise PeError(
                f"{path} 的 SizeOfOptionalHeader={optional_size}，装不下 Subsystem（+0x{SUBSYSTEM_OFFSET:X}）"
            )
        handle.seek(optional_start)
        optional = handle.read(SUBSYSTEM_OFFSET + 2)
        if len(optional) < SUBSYSTEM_OFFSET + 2:
            raise PeError(f"{path} 的可选头被截断：只读到 {len(optional)} 字节")
        magic = struct.unpack_from("<H", optional, 0)[0]
        if magic not in (0x10B, 0x20B):
            raise PeError(f"{path} 的可选头 magic=0x{magic:X} 既不是 PE32(0x10B) 也不是 PE32+(0x20B)")
        subsystem = struct.unpack_from("<H", optional, SUBSYSTEM_OFFSET)[0]
    return PeInfo(path, subsystem, optional_start + SUBSYSTEM_OFFSET, e_lfanew, magic, machine)


def check(role: str, path: Path, expected: int, errors: list[str]) -> PeInfo | None:
    """按角色断言一个产物的子系统；不匹配就往 `errors` 里写一条带文件注解的 error。"""
    try:
        info = read_subsystem(path)
    except PeError as exc:
        errors.append(f"::error::{role} 读数失败：{exc}")
        return None
    print(info.describe())
    if info.subsystem != expected:
        want = SUBSYSTEM_NAMES.get(expected, str(expected))
        got = SUBSYSTEM_NAMES.get(info.subsystem, str(info.subsystem))
        hint = {
            WINDOWS_GUI: "双击会多一个控制台宿主窗口（M196 要修的就是这个）",
            CONSOLE: "CLI 的 stdout/stderr 会静默丢失（ull 靠真控制台）",
        }.get(expected, "")
        errors.append(
            f"::error file={path}::{role} 的 PE 子系统应为 {expected}（{want}），"
            f"实际读到 {info.subsystem}（{got}）@ 偏移 0x{info.file_offset:X}；{hint}"
        )
    return info


def main(argv: list[str]) -> int:
    gui: Path | None = None
    cli: Path | None = None
    args = list(argv)
    while args:
        arg = args.pop(0)
        if arg == "--gui" and args:
            gui = Path(args.pop(0))
        elif arg == "--cli" and args:
            cli = Path(args.pop(0))
        elif arg == "--self-test":
            return self_test()
        elif arg in ("-h", "--help"):
            print(__doc__)
            return 0
        else:
            print(f"::error::不认识的参数（或缺少取值）：{arg}", file=sys.stderr)
            return 2
    if gui is None and cli is None:
        print("::error::至少要给 --gui <exe> 或 --cli <exe>", file=sys.stderr)
        return 2

    errors: list[str] = []
    if gui is not None:
        check("GUI", gui, WINDOWS_GUI, errors)
    if cli is not None:
        check("CLI", cli, CONSOLE, errors)
    for error in errors:
        print(error, file=sys.stderr)
    if errors:
        return 1
    print(
        f"OK: PE 子系统符合预期（GUI={WINDOWS_GUI} {SUBSYSTEM_NAMES[WINDOWS_GUI]}"
        f" / CLI={CONSOLE} {SUBSYSTEM_NAMES[CONSOLE]}）"
    )
    return 0


# ────────────────────────── 自测：合成 PE 桩 ──────────────────────────


def synth_pe(subsystem: int, *, pe32_plus: bool = True, signature: bytes = b"PE\0\0", mz: bytes = b"MZ") -> bytes:
    """造一个只够本脚本读的 PE 头（不执行、不链接，只让 `Subsystem` 落在正确位置）。"""
    e_lfanew = 0x80
    buf = bytearray(0x200)
    buf[0:2] = mz
    struct.pack_into("<I", buf, 0x3C, e_lfanew)
    buf[e_lfanew : e_lfanew + 4] = signature
    struct.pack_into("<H", buf, e_lfanew + 4, 0x8664)
    optional_size = 0xF0 if pe32_plus else 0xE0
    struct.pack_into("<H", buf, e_lfanew + 4 + 16, optional_size)
    optional_start = e_lfanew + 24
    struct.pack_into("<H", buf, optional_start, 0x20B if pe32_plus else 0x10B)
    struct.pack_into("<H", buf, optional_start + SUBSYSTEM_OFFSET, subsystem)
    return bytes(buf)


def self_test() -> int:
    failures = 0
    with tempfile.TemporaryDirectory(prefix="ull-pe-subsystem-") as tmp:
        root = Path(tmp)

        def write(name: str, data: bytes) -> Path:
            path = root / name
            path.write_bytes(data)
            return path

        cases: list[tuple[str, Path, int, bool]] = [
            (
                "基线：GUI 产物 Subsystem=2",
                write("gui_ok.exe", synth_pe(WINDOWS_GUI)),
                WINDOWS_GUI,
                False,
            ),
            (
                "回归：GUI 产物退回 CUI(3) —— 就是本次 bug 的形状",
                write("gui_cui.exe", synth_pe(CONSOLE)),
                WINDOWS_GUI,
                True,
            ),
            (
                "基线：CLI 产物 Subsystem=3",
                write("cli_ok.exe", synth_pe(CONSOLE, pe32_plus=False)),
                CONSOLE,
                False,
            ),
            (
                "回归：CLI 产物被贴成 WINDOWS_GUI(2)",
                write("cli_gui.exe", synth_pe(WINDOWS_GUI)),
                CONSOLE,
                True,
            ),
            (
                "回归：不是 PE（开头不是 MZ）",
                write("not_pe.exe", synth_pe(WINDOWS_GUI, mz=b"ZZ")),
                WINDOWS_GUI,
                True,
            ),
            (
                "回归：PE 头被截断",
                write("truncated.exe", synth_pe(WINDOWS_GUI)[: optional_start_of(synth_pe(WINDOWS_GUI)) + 0x10]),
                WINDOWS_GUI,
                True,
            ),
            (
                "回归：签名不是 PE\\0\\0",
                write("bad_sig.exe", synth_pe(WINDOWS_GUI, signature=b"PX\0\0")),
                WINDOWS_GUI,
                True,
            ),
            (
                "回归：文件根本不存在（构建步没跑）",
                root / "missing.exe",
                WINDOWS_GUI,
                True,
            ),
        ]

        for label, path, expected, want_red in cases:
            errors: list[str] = []
            check("SELFTEST", path, expected, errors)
            got_red = bool(errors)
            if got_red != want_red:
                want = "红" if want_red else "绿"
                print(
                    f"::error::自测失败：{label} 应该{want}，实际 {'红' if got_red else '绿'}",
                    file=sys.stderr,
                )
                failures += 1
            else:
                print(f"OK: {label} → {'红' if got_red else '绿'}")

        # 位置读数本身也要对：GUI 桩的 Subsystem 必须正好落在报告的那个文件偏移上。
        gui = write("offset.exe", synth_pe(WINDOWS_GUI))
        info = read_subsystem(gui)
        if gui.read_bytes()[info.file_offset : info.file_offset + 2] != struct.pack("<H", WINDOWS_GUI):
            print(f"::error::自测失败：报出来的偏移 0x{info.file_offset:X} 上不是 Subsystem", file=sys.stderr)
            failures += 1
        else:
            print(f"OK: 偏移自证 0x{info.file_offset:X} 处的两字节正是 {WINDOWS_GUI}")

    if failures:
        return 1
    print("OK: PE 子系统守卫自测全部通过（该红的红、该绿的不绿）")
    return 0


def optional_start_of(pe: bytes) -> int:
    """合成桩里可选头的起点（给「截断」用例算切点用）。"""
    return struct.unpack_from("<I", pe, 0x3C)[0] + 24


if __name__ == "__main__":
    for stream in (sys.stdout, sys.stderr):
        if hasattr(stream, "reconfigure"):
            stream.reconfigure(errors="backslashreplace")
    sys.exit(main(sys.argv[1:]))
