#!/usr/bin/env python3
"""品牌资产守门：尺寸 / 体积 / 裸字节长度 / LICENSE 在场（M376）。

守的是什么：作者那张 **9000×9000 / 3.5 MB** 的原始 logo 不许进仓库——它是保留所有权利的
作品（见 `assets/branding/LICENSE.txt`），而且任何构建都不需要那个分辨率。仓库里只允许
「派生到够用」的文件：`.ico`、256 的 PNG、64×64 的裸 RGBA。

四条判据（任一不过 → 退出码 1，每条一行 `error:`）：

1. **单文件 ≤ 256 KB**（`assets/branding/**`）—— 一个超清 PNG 光靠体积就会露馅；
2. **图片边长上限**（全仓受检图片，见 `TIERS`）—— 原图 9000 在任何一层都过不去；
3. **裸 RGBA 文件的长度必须逐档精确**（见 `RAW_RGBA_TIERS`）：`ull-64.rgba` 恰好
   `64*64*4 = 16384` 字节（标题栏标记用），`ull-128.rgba` 恰好 `128*128*4 = 65536` 字节
   （运行时窗口图标用）。它们被 `include_bytes!` 直接喂给 `Handle::from_rgba(..)` /
   `window::icon::from_rgba(..)`，多一个字节运行时就会拿到错位的图，少一个字节会 panic；
4. **`assets/branding/LICENSE.txt` 必须在** —— 派生件全部是「保留所有权利」的作品，
   少了这条声明，仓库就等于默认它们跟着 Apache-2.0 走。

# 「认得就必须判」（M376 r2 P2-1）

判据 2 的口径是**闭合**的：`IMAGE_SUFFIXES` 里的每个后缀都必须给出边长，**没有解码器就判红**。
今天的仓库只用到两种（PNG 的 `docs/images/*` 与 `assets/branding/ull-256.png`、ICO 的
`ull.ico`），所以只留这两个解码器；其它后缀（`.avif` / `.jxl` / `.heic` / `.psd` / `.svg` /
`.jpg` / `.gif` …）**照样会被认出来并判红**「没有解码器」，不静默放过。
要放一种新格式进仓库，先给本脚本加它的头解析，再提交那个文件 —— 这个顺序是有意的：
「悄悄多一种图」不该比「多写 15 行解码器」更容易。

# 为什么边长上限分档而不是「全仓 1024 一刀切」

仓库里已有的 4 张文档截图是 **1180×760**（窗口实拍，`docs/images/`），一刀切 1024 会让门禁
在加进来当天就是红的——红着的门禁等于没有。分档的每个数字都有依据：

| 范围 | 上限 | 依据 |
| ---- | ---- | ---- |
| `assets/branding/` | 1024 | 我们的产出最大 256，1024 是「派生件也不许超清」的反向余量 |
| `docs/images/` | 2560 | 窗口实拍，现状最大 1180；2× HiDPI 同窗实拍是 2360 |
| 其它 | 1024 | 真正的资产目录 |

原图 9000 落在 2560 之上，所以「原图入库」这条在任何一档都是红的。`DevDocs/` 在 `.gitignore`
里（内部取证材料，从不入库），因此不在本脚本的受检范围。

# 用法

    python packaging/check_branding_assets.py              # 检查当前仓库（git ls-files 口径）
    python packaging/check_branding_assets.py --self-test  # 外加反例自检（见 `self_test()`）

只依赖标准库：CI 不该为了「看一眼文件多大、多少像素」装一套图像栈。
"""

from __future__ import annotations

import argparse
import re
import struct
import subprocess
import sys
import tempfile
import zlib
from pathlib import Path

BRANDING_DIR = "assets/branding"
FILE_LIMIT = 256 * 1024
# 裸 RGBA 档位（文件名, 边长）：长度必须恰好 边长 × 边长 × 4。一档一行，加档就加一行。
#   64  → 标题栏左侧的品牌标记（`crates/app-iced/src/shell/title_icon.rs`）
#   128 → 运行时窗口 / 任务栏 / Alt-Tab 图标（`crates/app-iced/src/app/boot/diagnostics.rs`）
RAW_RGBA_TIERS: tuple[tuple[str, int], ...] = (("ull-64.rgba", 64), ("ull-128.rgba", 128))
LICENSE_FILE = f"{BRANDING_DIR}/LICENSE.txt"

# (路径前缀, 边长上限, 说明)；按顺序取第一条命中的，都没有就用最后一条默认档。
TIERS: tuple[tuple[str | None, int, str], ...] = (
    (f"{BRANDING_DIR}/", 1024, "品牌资产"),
    ("docs/images/", 2560, "文档截图"),
    (None, 1024, "其它"),
)

# 「图片」的完整名单：认出来就得给出边长，给不出就红（见模块文档「认得就必须判」）。
IMAGE_SUFFIXES = frozenset(
    {
        ".png", ".jpg", ".jpeg", ".gif", ".bmp", ".ico", ".webp", ".svg", ".svgz",
        ".tif", ".tiff", ".avif", ".jxl", ".heic", ".heif", ".jp2", ".j2k", ".psd",
        ".icns", ".tga", ".dds", ".exr", ".hdr",
    }
)


def png_side(data: bytes) -> int | None:
    if len(data) < 24 or data[:8] != b"\x89PNG\r\n\x1a\n" or data[12:16] != b"IHDR":
        return None
    width, height = struct.unpack_from(">II", data, 16)
    return max(width, height)


def ico_side(data: bytes) -> int | None:
    if len(data) < 6 or data[:4] != b"\x00\x00\x01\x00":
        return None
    count = struct.unpack_from("<H", data, 4)[0]
    if count == 0 or len(data) < 6 + 16 * count:
        return None
    sides = []
    for index in range(count):
        # ICO 目录里 256 写 0（0 即 256）
        width, height = data[6 + 16 * index], data[7 + 16 * index]
        sides += [width or 256, height or 256]
    return max(sides)


# 今天仓库里只有这两种格式在用；其余后缀故意不给解码器 —— 它们会被判红，而不是被跳过。
DECODERS = {".png": png_side, ".ico": ico_side}


def ceiling(relative: str) -> tuple[int, str]:
    for prefix, limit, label in TIERS:
        if prefix is None or relative.startswith(prefix):
            return limit, label
    raise AssertionError("TIERS 最后一条必须是 (None, …)")


def tracked_files(root: Path) -> list[str]:
    """仓库口径与 file-size 守门一致：`git ls-files`；没有 git（自检临时树）就遍历。"""
    if (root / ".git").exists():
        result = subprocess.run(
            ["git", "-C", str(root), "ls-files", "-z"], capture_output=True, check=False
        )
        if result.returncode == 0:
            return [item for item in result.stdout.decode("utf-8", "replace").split("\0") if item]
    found = []
    for path in root.rglob("*"):
        if path.is_file() and not any(part in (".git", "target", "node_modules") for part in path.parts):
            found.append(path.relative_to(root).as_posix())
    return sorted(found)


def audit(root: Path, files: list[str]) -> list[str]:
    problems: list[str] = []

    if not (root / LICENSE_FILE).is_file():
        problems.append(f"{LICENSE_FILE}: 缺失 —— 派生件全是保留所有权利的作品，声明必须随目录一起在")

    for name, side in RAW_RGBA_TIERS:
        expected = side * side * 4
        path = root / BRANDING_DIR / name
        if not path.is_file():
            problems.append(f"{BRANDING_DIR}/{name}: 缺失（运行时的裸 RGBA 靠它）")
        elif path.stat().st_size != expected:
            problems.append(
                f"{BRANDING_DIR}/{name}: {path.stat().st_size} 字节，必须恰好 {expected}"
                f"（= {side}×{side}×4，裸字节无文件头）"
            )

    for relative in files:
        path = root / relative
        suffix = path.suffix.lower()
        if relative.startswith(f"{BRANDING_DIR}/") and path.stat().st_size > FILE_LIMIT:
            problems.append(
                f"{relative}: {path.stat().st_size:,} 字节，超过单文件上限 {FILE_LIMIT:,}"
                "（超清图不许进品牌目录）"
            )
        if suffix not in IMAGE_SUFFIXES:
            continue
        decoder = DECODERS.get(suffix)
        side = decoder(path.read_bytes()) if decoder else None
        limit, label = ceiling(relative)
        if side is None:
            reason = "本脚本没有它的解码器" if decoder is None else "文件头不认识"
            problems.append(
                f"{relative}: 读不出边长（{reason}）—— 认得出的图片就必须能判，"
                "请先给 packaging/check_branding_assets.py 加这个格式的头解析"
            )
        elif side > limit:
            problems.append(
                f"{relative}: 最长边 {side}，超过「{label}」这一档的上限 {limit}"
                "（9000×9000 原图在任何一档都不许入库）"
            )
    return problems


# ---------------------------------------------------------------------------
# 自检：判据必须能变红（合成的假文件，秒级，不需要 git）
# ---------------------------------------------------------------------------


def fake_png(width: int, height: int, *, padding: int = 0) -> bytes:
    """只造 PNG 头 + IHDR + IEND：尺寸判据只读 IHDR，所以自检不需要真像素。"""
    ihdr = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    chunk = b"IHDR" + ihdr
    body = struct.pack(">I", len(ihdr)) + chunk + struct.pack(">I", zlib.crc32(chunk))
    return b"\x89PNG\r\n\x1a\n" + body + b"\x00\x00\x00\x00" + b"IEND" + b"\xae\x42\x60\x82" + b"\0" * padding


def self_test() -> int:
    failures = 0

    def clean_tree(work: str) -> Path:
        root = Path(work)
        (root / BRANDING_DIR).mkdir(parents=True)
        (root / BRANDING_DIR / "LICENSE.txt").write_text("(c) 2026 AskaEth", encoding="utf-8")
        for name, side in RAW_RGBA_TIERS:
            (root / BRANDING_DIR / name).write_bytes(b"\0" * (side * side * 4))
        (root / BRANDING_DIR / "ull-256.png").write_bytes(fake_png(256, 256))
        return root

    def expect_red(label: str, build, marker: str) -> None:
        nonlocal failures
        with tempfile.TemporaryDirectory() as work:
            root = clean_tree(work)
            build(root)
            problems = audit(root, tracked_files(root))
            if not problems:
                print(f"FAIL: {label} 竟然通过了（判据失效）", file=sys.stderr)
                failures += 1
            elif marker not in "\n".join(problems):
                print(f"FAIL: {label} 红了，但报错不是那条判据：{problems}", file=sys.stderr)
                failures += 1
            else:
                print(f"ok: 反例「{label}」红了 —— {problems[0]}")

    # 正控：什么都不动必须绿，否则下面的反例根本没验到东西。
    with tempfile.TemporaryDirectory() as work:
        root = clean_tree(work)
        problems = audit(root, tracked_files(root))
        if problems:
            print(f"FAIL: 合规的临时树竟然报错 {problems}（判据过度）", file=sys.stderr)
            failures += 1
        else:
            print("ok: 正控（只有合规派生件）绿")

    expect_red(
        "品牌目录里一张 2000×2000 的图",
        lambda root: (root / BRANDING_DIR / "huge.png").write_bytes(fake_png(2000, 2000)),
        "超过「品牌资产」这一档的上限 1024",
    )
    expect_red(
        "任意位置一张 9000×9000 的图（原图入库）",
        lambda root: (
            (root / "docs").mkdir(exist_ok=True),
            (root / "docs" / "ULL.png").write_bytes(fake_png(9000, 9000)),
        ),
        "9000×9000 原图在任何一档都不许入库",
    )
    expect_red(
        "品牌目录里一个超重文件（>256 KB）",
        lambda root: (root / BRANDING_DIR / "fat.png").write_bytes(fake_png(64, 64, padding=FILE_LIMIT)),
        "超过单文件上限",
    )
    expect_red(
        "品牌目录缺 LICENSE.txt",
        lambda root: (root / BRANDING_DIR / "LICENSE.txt").unlink(),
        "缺失",
    )
    expect_red(
        "ull-64.rgba 长度不对",
        lambda root: (root / BRANDING_DIR / "ull-64.rgba").write_bytes(b"\0" * (64 * 64 * 4 - 1)),
        "必须恰好",
    )
    # M379：128 档是「运行时窗口图标」那一档，少一个字节（65,535）必须红 —— 这条是 M379 新增
    # 判据的反例；正控里的 65,536 已经证明它不会「总是红」。
    expect_red(
        "ull-128.rgba 少一个字节（65,535）",
        lambda root: (root / BRANDING_DIR / "ull-128.rgba").write_bytes(b"\0" * (128 * 128 * 4 - 1)),
        "必须恰好 65536",
    )
    expect_red(
        "ull-128.rgba 整份缺失",
        lambda root: (root / BRANDING_DIR / "ull-128.rgba").unlink(),
        "ull-128.rgba: 缺失",
    )
    # M376 r2 P2-1 的回归：原图转成「脚本没有解码器」的格式提交 ⇒ 以前是静默 ok，现在必须红。
    expect_red(
        "把原图转成 .avif 提交（脚本没有该格式解码器）",
        lambda root: (root / BRANDING_DIR / "ULL.avif").write_bytes(b"\x00\x00\x00\x1cftypavif" + b"\0" * 64),
        "没有它的解码器",
    )

    print("self-test:", "ok" if failures == 0 else f"{failures} 处失败")
    return 0 if failures == 0 else 1


def safe_streams() -> None:
    """管道里钉 UTF-8（Windows runner 的默认代码页打不出汉字，见 check_pe_version.py 同一段注释）。"""
    for stream in (sys.stdout, sys.stderr):
        reconfigure = getattr(stream, "reconfigure", None)
        if reconfigure is None:
            continue
        try:
            if stream.isatty():
                reconfigure(errors="backslashreplace")
            else:
                reconfigure(encoding="utf-8", errors="backslashreplace")
        except (ValueError, OSError):
            pass


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="品牌资产守门（尺寸 / 体积 / 裸字节 / LICENSE，M376）")
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1], help="仓库根")
    parser.add_argument("--self-test", action="store_true", help="跑反例自检（超大图 / 超重 / 缺 LICENSE / 裸字节 / 无解码器格式）")
    args = parser.parse_args(argv)

    if args.self_test:
        return self_test()

    root = args.root.resolve()
    files = tracked_files(root)
    problems = audit(root, files)
    for line in problems:
        print(f"error: {line}", file=sys.stderr)
    if problems:
        print(f"FAIL：{len(problems)} 条判据没过（品牌资产的口径见 assets/branding/FORMAT.md）", file=sys.stderr)
        return 1
    images = sum(1 for name in files if Path(name).suffix.lower() in IMAGE_SUFFIXES)
    print(f"ok: 品牌资产合规（受检文件 {len(files)} 个，其中图片 {images} 个；原图 9000×9000 未入库）")
    return 0


if __name__ == "__main__":
    safe_streams()
    raise SystemExit(main())
