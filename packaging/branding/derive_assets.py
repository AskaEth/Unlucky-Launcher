#!/usr/bin/env python3
"""从作者自有的超高分辨率源图派生**可入库**的品牌资产（M376）。

源图（9000×9000 / 3.5 MB）在仓库之外，**永远不入库**：它的版权与仓库代码分开（见
`assets/branding/LICENSE.txt`），体积也远超任何构建需要。本脚本只读它，只把「够用就好」的
派生形式写进 `assets/branding/`。

用法（源图路径是**参数**，脚本里不写死任何本机路径）::

    python packaging/branding/derive_assets.py "D:/Project/Image/.../ULL.png"
    python packaging/branding/derive_assets.py <源图> --out assets/branding

产出（**四项**，没有第五项）::

    ull.ico        16/24/32/48/64/128 走 DIB（BMP）条目 + 256 走 PNG 条目
                   ——「小尺寸 DIB + 256 PNG」是 ICO 的通行组合：资源管理器按最近档取，
                   小尺寸用 DIB 是为了不依赖 PNG-in-ICO 的旧壳兼容性
    ull-256.png    256×256 缩小版，供 README / 关于页 / 将来的安装包
    ull-64.rgba    64×64 **裸 RGBA8888 字节**（16384 字节，行优先、无任何文件头），
                   给 `iced::widget::image::Handle::from_rgba(..)` 用（标题栏左侧的品牌标记，
                   见 `crates/app-iced/src/shell/title_icon.rs`）
    ull-128.rgba   128×128 裸 RGBA8888 字节（65536 字节，同一格式），
                   给 `iced::window::icon::from_rgba(include_bytes!(…).to_vec(), 128, 128)` 用
                   （注意：`from_rgba` 是 `window::icon` 模块的**自由函数**，不是 `Icon` 的关联函数）

为什么窗口图标要 128 而不是 64（M379）：任务栏与 Alt-Tab 在 150% / 200% 缩放下按 96 / 128
取图，64 那张只能被放大 ⇒ 发虚。128 只多 49 KB，两笔预算都还在线内。

为什么不直接在运行时解码 PNG/SVG：那要走一遍图像解码，`include_bytes!` 把「给窗口一张图标」
降级成一个常量 + 一次内存拷贝；窗口图的尺寸与像素布局在编译期就定死（见
`crates/app-iced/src/app/boot/diagnostics.rs` 的 `ICON_SIZE` / `ICON_PIXELS` 与那条 `const _`）。
**这两条运行时路径都不做解码**：窗口图标走 `window::icon::from_rgba`，标题栏标记走
`iced::widget::image::Handle::from_rgba`（像素已经是裸字节，渲染器只建纹理）。
iced 侧开的是 `image-without-codecs`（背景图 / 头像 / 标记这些「图已解好或由别处解好」的用法，
M93 起就有），所以裸字节省下的不是某个 feature，而是**启动路径上的解码与一次 39 KB 的 PNG 入库**。
（本 crate 另有 `image` 依赖用于截图 / 背景图解码，那是 M93 的既有依赖；D40 管的是
「设计层零依赖、解码单独一层」，不是「app-iced 不许有 image」。）

# 体积与尺寸纪律（脚本自己守，`packaging/check_branding_assets.py` 在 CI 上再守一遍）

- 任何一档边长 ≤ `MAX_SIDE`（1024）：源图 9000 那一档**不许**出现在产出里；
- `assets/branding/` 合计 ≤ `BUDGET_BYTES`（300 KB）。超了就**红**，不静默超。

# 依赖

只在**本机派生时**用 Pillow（缩放与 PNG 编码）。CI 不跑本脚本，跑的是
`packaging/check_branding_assets.py`（纯标准库）——门禁不该为了重生成图标装一套图像栈。
"""

from __future__ import annotations

import argparse
import io
import struct
import sys
from pathlib import Path

try:
    from PIL import Image
except ImportError:  # pragma: no cover - 只在没装 Pillow 的本机上走到
    sys.exit("需要 Pillow（仅本机派生用）：pip install pillow")

MAX_SIDE = 1024
BUDGET_BYTES = 300 * 1024
ICO_SIZES = (16, 24, 32, 48, 64, 128, 256)
# 256 换成 PNG 条目（DIB 的话单这一档就 270 KB，见 FORMAT.md 的体积账）
PNG_ENTRY_FROM = 256
# 裸 RGBA 的档位：64 给标题栏标记（20px 缩略不需要 65 KB 源），128 给窗口 / 任务栏图标。
RUNTIME_SIDES = (64, 128)
RUNTIME_SIDE = max(RUNTIME_SIDES)


def load_master(source: Path) -> Image.Image:
    """把源图缩到 `MAX_SIDE` 见方的母版（**它只活在内存里**，不落盘）。"""
    with Image.open(source) as raw:
        image = raw.convert("RGBA")
    if image.width != image.height:
        print(f"warn: 源图不是正方形（{image.width}×{image.height}），图标会按左上角裁方")
        edge = min(image.width, image.height)
        image = image.crop((0, 0, edge, edge))
    if image.width < RUNTIME_SIDE:
        sys.exit(f"源图太小（{image.width}px），至少要 {RUNTIME_SIDE}px")

    master_side = min(MAX_SIDE, image.width)
    # reducing_gap 让 Pillow 先按整数倍率粗降再精降：9000 → 1024 直接一步走会有振铃。
    return image.resize((master_side, master_side), Image.LANCZOS, reducing_gap=3.0)


def frame(master: Image.Image, side: int) -> Image.Image:
    return master if master.width == side else master.resize((side, side), Image.LANCZOS)


def dib_entry(image: Image.Image) -> bytes:
    """ICO 里的 BMP 条目：BITMAPINFOHEADER（高度写两倍）+ 自底向上 BGRA + AND 掩码。

    AND 掩码全 0（「不透明」）是 32bpp 图标的常规写法：真正的透明度由 alpha 通道给，
    掩码只为了满足结构（1bpp、每行 4 字节对齐），少了它资源管理器会画成黑底。
    """
    side = image.width
    header = struct.pack(
        "<IiiHHIIiiII", 40, side, side * 2, 1, 32, 0, side * side * 4, 0, 0, 0, 0
    )
    rows = image.tobytes("raw", "BGRA")
    stride = side * 4
    pixels = b"".join(
        rows[offset : offset + stride]
        for offset in reversed(range(0, len(rows), stride))
    )
    mask_stride = ((side + 31) // 32) * 4
    return header + pixels + b"\0" * (mask_stride * side)


def png_entry(image: Image.Image) -> bytes:
    """ICO 里的 PNG 条目：一张标准 PNG 文件原样塞进去（Pillow 编码，无时间戳，可复跑）。"""
    buffer = io.BytesIO()
    image.save(buffer, format="PNG", optimize=True)
    return buffer.getvalue()


def write_ico(path: Path, master: Image.Image) -> int:
    entries: list[tuple[int, bytes]] = []
    for side in ICO_SIZES:
        image = frame(master, side)
        entries.append((side, png_entry(image) if side >= PNG_ENTRY_FROM else dib_entry(image)))

    offset = 6 + 16 * len(entries)
    directory = bytearray(struct.pack("<HHH", 0, 1, len(entries)))
    payload = bytearray()
    for side, blob in entries:
        directory += struct.pack(
            "<BBBBHHII",
            side if side < 256 else 0,  # 256 在 ICO 目录里写 0
            side if side < 256 else 0,
            0,  # 调色板色数（真彩为 0）
            0,  # 保留
            1,  # 平面数
            32,  # 位深
            len(blob),
            offset,
        )
        payload += blob
        offset += len(blob)
    path.write_bytes(bytes(directory + payload))
    return path.stat().st_size


def write_raw_rgba(path: Path, master: Image.Image, side: int) -> int:
    """`side`×`side` 裸 RGBA 字节：行优先、上到下、无文件头。字节数必须恰好 ``side*side*4``。"""
    blob = frame(master, side).tobytes("raw", "RGBA")
    expected = side * side * 4
    if len(blob) != expected:  # 防「Pillow 换了 raw 编码器的行为」这种静默漂移
        sys.exit(f"裸 RGBA 长度 {len(blob)} ≠ {expected}")
    path.write_bytes(blob)
    return len(blob)


def write_png(path: Path, master: Image.Image, side: int = 256) -> int:
    path.write_bytes(png_entry(frame(master, side)))
    return path.stat().st_size


def directory_bytes(out: Path) -> int:
    return sum(path.stat().st_size for path in sorted(out.iterdir()) if path.is_file())


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="派生品牌图标资产（源图不入库，M376）")
    parser.add_argument("source", type=Path, help="作者自有的高分辨率源图（仓库外）")
    parser.add_argument(
        "--out",
        type=Path,
        default=Path(__file__).resolve().parents[2] / "assets" / "branding",
        help="输出目录（默认 assets/branding）",
    )
    args = parser.parse_args(argv)

    source = args.source.expanduser()
    if not source.is_file():
        sys.exit(f"源图不存在：{source}")
    out = args.out
    out.mkdir(parents=True, exist_ok=True)

    master = load_master(source)
    print(f"source: {source.name} → master {master.width}×{master.height}（只驻内存，不落盘）")

    written = [
        ("ull.ico", write_ico(out / "ull.ico", master)),
        ("ull-256.png", write_png(out / "ull-256.png", master, 256)),
        *[
            (f"ull-{side}.rgba", write_raw_rgba(out / f"ull-{side}.rgba", master, side))
            for side in RUNTIME_SIDES
        ],
    ]
    for name, size in written:
        print(f"  {name:<14} {size:>8,} 字节")

    total = directory_bytes(out)
    print(f"assets/branding 合计 {total:,} 字节 / 预算 {BUDGET_BYTES:,} 字节")
    if total > BUDGET_BYTES:
        sys.stderr.write(
            f"error: 品牌资产合计 {total:,} 字节，超过预算 {BUDGET_BYTES:,}。\n"
            "       先报数据再决定剔哪一档（别默默超预算）：把 ICO_SIZES 里最大的一档去掉即可。\n"
        )
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
