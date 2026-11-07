#!/usr/bin/env python3
"""截图「不是一帧空白」判定：CI 运行时冒烟的门禁脚本（零第三方依赖）。

为什么需要它：`--screenshot` 的 PNG 是进程自己编码写出的，只要流程走到 `take_snapshot` 就会得到
「合法 PNG + 退出码 0」；像素断言是**纵深防御**，守的是「进程活着、写出合法 PNG，但场景几乎没画」这一档。
本机可复现的两种失效都是硬失败（运行步就红，不必等像素断言）：①完全没有字体 → fontique panic
（退出码 101，不产生 PNG）；②不是软渲染（默认 femtovg）→ `take_snapshot is not supported by this
FemtoVG backend`（退出码 1）。其余「渲染器出图但内容没上去」的情形本机暂无样本，属防御性设计。

三个判据（阈值依据见 `--selftest` 与 packaging/ci-runtime-smoke.md）：

- `unique_colors`：唯一色数。全白 / 全黑图 = 1；真实 UI 数百到上千。
- `luma_stddev`：亮度标准差。纯色图 = 0；真实 UI 有面板/文字/渐变的明暗层次。
- `edge_ratio`：**强边缘像素占比**——水平相邻像素对里亮度差 ≥8 的比例。纯色图 = 0；真实 UI 的文字与
  边框占千分之几到百分之几。用它而不是「平均亮度差」：空数据目录的主界面 57% 是同一片底色，
  平均差会被大片平面稀释（实测只有 0.4），而强边缘占比只数真正的边界，跨主题/跨字体都稳定。

只用 Python 标准库：`zlib` 解压 PNG 的 deflate 流，自己反过滤（filter 0~4）。
不引 Pillow 是因为 CI 不该为了「看一张图」多装一套图像栈（也符合项目的依赖白名单纪律）。

用法::

    python packaging/check_screenshot.py shot.png            # 判定，退出码 0/1
    python packaging/check_screenshot.py shot.png --json     # 机器可读（CI 日志用）
    python packaging/check_screenshot.py --selftest          # 打印阈值依据的两组数值
"""

from __future__ import annotations

import argparse
import json
import math
import struct
import sys
import tempfile
import zlib
from collections import Counter
from pathlib import Path

PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"

# 判定阈值。两侧数值实测（M62 本机 Docker + xvfb + Windows 历史截图）：
# - 真实帧：唯一色 342~2352、亮度σ 19.45~27.46、强边缘占比 0.0076~0.0433（最弱是 Linux 空数据主界面）；
# - 人为纯色帧：1 / 0.0 / 0.0。
# 阈值取在两者之间、贴近真实下限的 1/5~1/8：换主题/换字体/换窗口尺寸都不会误报，空白帧一定红。
DEFAULT_MIN_UNIQUE = 64
DEFAULT_MIN_STDDEV = 3.0
DEFAULT_MIN_EDGE_RATIO = 0.001

# 「强边缘」的亮度差门槛：≥8/255 肉眼即视为一条边界；抗锯齿造成的 1~2 级抖动不算。
EDGE_LUMA_DELTA = 8

# PNG 颜色类型 → 每像素通道数（调色板 3 走索引，需要 PLTE，本脚本不解）
CHANNELS_BY_COLOR_TYPE = {0: 1, 2: 3, 4: 2, 6: 4}


class PngError(Exception):
    """PNG 结构不受支持或损坏。"""


def _iter_chunks(data: bytes):
    """按 PNG 规范切块：长度(4) + 类型(4) + 数据 + CRC(4)。"""
    offset = len(PNG_SIGNATURE)
    while offset + 12 <= len(data):
        (length,) = struct.unpack(">I", data[offset : offset + 4])
        yield data[offset + 4 : offset + 8], data[offset + 8 : offset + 8 + length]
        offset += 12 + length


def _unfilter(kind: int, line: bytearray, prev: bytearray, bpp: int) -> None:
    """就地把一条扫描线反过滤（PNG 规范 §9.2 的五个过滤类型）。"""
    if kind == 0:
        return
    if kind == 1:  # Sub：加上左边
        for index in range(bpp, len(line)):
            line[index] = (line[index] + line[index - bpp]) & 0xFF
    elif kind == 2:  # Up：加上正上方
        for index in range(len(line)):
            line[index] = (line[index] + prev[index]) & 0xFF
    elif kind == 3:  # Average：加上 (左 + 上) // 2
        for index in range(len(line)):
            left = line[index - bpp] if index >= bpp else 0
            line[index] = (line[index] + ((left + prev[index]) >> 1)) & 0xFF
    elif kind == 4:  # Paeth：加上 Paeth 预测子
        for index in range(len(line)):
            left = line[index - bpp] if index >= bpp else 0
            up = prev[index]
            upper_left = prev[index - bpp] if index >= bpp else 0
            estimate = left + up - upper_left
            dist_left = abs(estimate - left)
            dist_up = abs(estimate - up)
            dist_corner = abs(estimate - upper_left)
            if dist_left <= dist_up and dist_left <= dist_corner:
                predictor = left
            elif dist_up <= dist_corner:
                predictor = up
            else:
                predictor = upper_left
            line[index] = (line[index] + predictor) & 0xFF
    else:
        raise PngError(f"未知的行过滤类型 {kind}")


def decode_png(path: Path) -> tuple[int, int, list[bytearray], int]:
    """读 8bit 非隔行 PNG，返回 (宽, 高, 每行原始字节, 通道数)。"""
    data = path.read_bytes()
    if not data.startswith(PNG_SIGNATURE):
        raise PngError(f"{path} 不是 PNG（签名不符，可能是 0 字节文件或半截写入）")

    width = height = depth = color_type = interlace = None
    compressed = bytearray()
    for kind, body in _iter_chunks(data):
        if kind == b"IHDR":
            width, height, depth, color_type, _, _, interlace = struct.unpack(">IIBBBBB", body)
        elif kind == b"IDAT":
            compressed.extend(body)
        elif kind == b"IEND":
            break

    if width is None or height is None:
        raise PngError("缺 IHDR 块")
    if interlace:
        raise PngError("不支持隔行（Adam7）PNG")
    if depth != 8:
        raise PngError(f"只支持 8bit PNG，实际 {depth}bit")
    channels = CHANNELS_BY_COLOR_TYPE.get(color_type)
    if channels is None:
        raise PngError(f"不支持的颜色类型 {color_type}（调色板 PNG 请先转成 RGB/RGBA）")

    raw = zlib.decompress(bytes(compressed))
    stride = width * channels
    if len(raw) < (stride + 1) * height:
        raise PngError(f"像素数据不足：期望 {(stride + 1) * height} 字节，实际 {len(raw)}")

    rows: list[bytearray] = []
    previous = bytearray(stride)
    offset = 0
    for _ in range(height):
        kind = raw[offset]
        offset += 1
        line = bytearray(raw[offset : offset + stride])
        offset += stride
        _unfilter(kind, line, previous, channels)
        rows.append(line)
        previous = line
    return width, height, rows, channels


def measure(width: int, height: int, rows: list[bytearray], channels: int) -> dict:
    """统计「这帧图里有没有信息」的三项指标。"""
    colors: Counter[int] = Counter()
    luma_sum = 0
    luma_square_sum = 0
    strong_edges = 0
    pair_count = 0

    for row in rows:
        previous_luma = None
        for x in range(width):
            base = x * channels
            if channels >= 3:
                red, green, blue = row[base], row[base + 1], row[base + 2]
            else:
                red = green = blue = row[base]
            colors[(red << 16) | (green << 8) | blue] += 1
            # 整数亮度（ITU-R BT.601 系数），避免浮点误差影响阈值比较
            luma = (red * 299 + green * 587 + blue * 114) // 1000
            luma_sum += luma
            luma_square_sum += luma * luma
            if previous_luma is not None:
                pair_count += 1
                if abs(luma - previous_luma) >= EDGE_LUMA_DELTA:
                    strong_edges += 1
            previous_luma = luma

    total = width * height
    mean = luma_sum / total if total else 0.0
    variance = max(luma_square_sum / total - mean * mean, 0.0) if total else 0.0
    top_color, top_count = colors.most_common(1)[0] if colors else (0, 0)
    return {
        "width": width,
        "height": height,
        "pixels": total,
        "unique_colors": len(colors),
        "top_color": f"#{top_color:06x}",
        "top_color_share": round(top_count / total, 4) if total else 0.0,
        "luma_mean": round(mean, 2),
        "luma_stddev": round(math.sqrt(variance), 2),
        "edge_ratio": round(strong_edges / pair_count, 5) if pair_count else 0.0,
    }


def verdict(
    stats: dict, min_unique: int, min_stddev: float, min_edge_ratio: float
) -> list[str]:
    """把指标折成「失败原因清单」；空清单 = 通过。"""
    problems = []
    if stats["pixels"] <= 0:
        problems.append("图像尺寸为 0（宽或高是 0）")
        return problems
    if stats["unique_colors"] < min_unique:
        problems.append(
            f"唯一色数 {stats['unique_colors']} < 阈值 {min_unique}（像纯色/空白帧）"
        )
    if stats["luma_stddev"] < min_stddev:
        problems.append(
            f"亮度标准差 {stats['luma_stddev']} < 阈值 {min_stddev}（整帧明暗无层次）"
        )
    if stats["edge_ratio"] < min_edge_ratio:
        problems.append(
            f"强边缘占比 {stats['edge_ratio']} < 阈值 {min_edge_ratio}（没有边界/文字的迹象）"
        )
    return problems


def encode_png(width: int, height: int, rgb_row) -> bytes:
    """最小 PNG 编码器（8bit RGB、filter 0、zlib 压缩）——只给 `--selftest` 造样图。"""
    raw = bytearray()
    for y in range(height):
        raw.append(0)
        raw.extend(rgb_row(y))
    out = bytearray(PNG_SIGNATURE)
    for kind, body in (
        (b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)),
        (b"IDAT", zlib.compress(bytes(raw), 6)),
        (b"IEND", b""),
    ):
        out.extend(struct.pack(">I", len(body)))
        out.extend(kind)
        out.extend(body)
        out.extend(struct.pack(">I", zlib.crc32(kind + body) & 0xFFFFFFFF))
    return bytes(out)


def selftest() -> int:
    """造两张对照样图跑一遍，打印阈值依据的两组数值。

    - 全白 / 全黑：模拟「窗口起来了但一个像素都没画」的空白帧；
    - 一像素黑边框 + 中部一条横线：最弱的「有内容」下限（远弱于真实 UI）。
    """
    width, height = 1180, 760
    cases = {
        "flat-white": lambda y: b"\xff" * (width * 3),
        "flat-black": lambda y: b"\x00" * (width * 3),
        "border-line": lambda y: b"".join(
            b"\x00\x00\x00"
            if (y in (0, height - 1) or x in (0, width - 1) or y == height // 2)
            else b"\xff\xff\xff"
            for x in range(width)
        ),
    }
    with tempfile.TemporaryDirectory() as tmp:
        for name, describe in cases.items():
            path = Path(tmp) / f"{name}.png"
            path.write_bytes(encode_png(width, height, describe))
            stats = measure(*decode_png(path))
            failed = verdict(
                stats, DEFAULT_MIN_UNIQUE, DEFAULT_MIN_STDDEV, DEFAULT_MIN_EDGE_RATIO
            )
            print(
                json.dumps(
                    {
                        "case": name,
                        "unique_colors": stats["unique_colors"],
                        "luma_stddev": stats["luma_stddev"],
                        "edge_ratio": stats["edge_ratio"],
                        "verdict": "fail" if failed else "pass",
                    },
                    ensure_ascii=False,
                )
            )
    return 0


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description="截图非空白判定（CI 运行时冒烟门禁）")
    parser.add_argument("png", nargs="?", type=Path, help="待判定的 PNG 路径")
    parser.add_argument("--json", action="store_true", help="以 JSON 输出统计结果")
    parser.add_argument("--min-unique", type=int, default=DEFAULT_MIN_UNIQUE, help="唯一色数下限")
    parser.add_argument("--min-stddev", type=float, default=DEFAULT_MIN_STDDEV, help="亮度标准差下限")
    parser.add_argument(
        "--min-edge-ratio", type=float, default=DEFAULT_MIN_EDGE_RATIO, help="强边缘占比下限"
    )
    parser.add_argument("--selftest", action="store_true", help="打印阈值依据的对照数值后退出")
    args = parser.parse_args(argv)

    if args.selftest:
        return selftest()
    if args.png is None:
        parser.error("需要给出 PNG 路径（或 --selftest）")

    try:
        stats = measure(*decode_png(args.png))
    except (PngError, OSError, zlib.error) as error:
        print(f"[fail] 读不出截图 {args.png}: {error}", file=sys.stderr)
        return 1

    problems = verdict(stats, args.min_unique, args.min_stddev, args.min_edge_ratio)
    if args.json:
        print(json.dumps({**stats, "problems": problems}, ensure_ascii=False))
    else:
        print(
            f"{args.png}: {stats['width']}x{stats['height']} "
            f"唯一色 {stats['unique_colors']} 亮度σ {stats['luma_stddev']} "
            f"强边缘 {stats['edge_ratio']} 主色 {stats['top_color']}"
            f"({stats['top_color_share'] * 100:.1f}%)"
        )
    if problems:
        for problem in problems:
            print(f"[fail] {problem}", file=sys.stderr)
        return 1
    print("[ok] 截图非空白")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
