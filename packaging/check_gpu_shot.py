#!/usr/bin/env python3
"""GPU 截图证据判定（M249）：把「这张图真是 GPU 档出来的」变成机器判据，而不是靠人读日志。

为什么需要它：M246 实测到 `--renderer=gpu` 落到 RX 6600 XT / Vulkan（DiscreteGpu）并出图，但那份
结论是**人读 258 行日志**得出的。日志少一行、`Selected` 换了适配器、PNG 是上一次的残留，人眼复核
一遍就得重来。本脚本把那套判据固定成三分钟能复跑一遍的门禁，且**缺哪条就点名哪条**。

入参 = 一份 stderr 日志 + 一张 PNG（可选 `--expect-refuse` 走负例口径）。正例六条判据（每条抓得住
什么、抓不住什么见 `packaging/docs/gpu-evidence.md` §2）：

1. **全部** `[shot] … renderer=` 行都是 `wgpu`——不是「出现过」（把 CPU 那次的 stderr `>>` 拼进来
   就会骗过旧写法），也**不是** `[probe] renderer = wgpu`：后者只是启动前的意图，M246 的 A6 负例里
   它照样打印 wgpu，却一个像素都没画。
2. `Selected: AdapterInfo { … device_type: X }` 的 `X` 不在 `DISALLOWED_DEVICE_TYPES` 内；
   **取不到 `X` 也算失败**（多行 Debug 漂移不许静默给绿）。
3. PNG 能解码且像素数 > 0。
4. PNG 宽高 = 日志里 `[shot] W×H` 声明的宽高（把图和日志钉在一起）。
5. `[backend] final=wgpu result=started`（后端自己的结论行与上面几条不矛盾）。
6. PNG 过 `check_screenshot.py` 的非空白判据（复用它的解码器与阈值，不复制第二份）。

`--expect-refuse` 是负例口径（M246 的 A6）：要求 `[backend] final=… via=refuse`、**没有**任何
`[shot]` 出图行、且目标 PNG 不存在。它证明「该拒绝时真拒绝了」，不证明「GPU 能出图」。

零第三方依赖；PNG 只解码一次，用的是同目录 `check_screenshot.py` 的 `decode_png` / `measure` / `verdict`。

用法::

    python packaging/check_gpu_shot.py shot.stderr.log shot.png       # 正例：判定，0/1 退出
    python packaging/check_gpu_shot.py a6.stderr.log --expect-refuse  # 负例：判定拒绝是否成立
    python packaging/check_gpu_shot.py --self-test                    # 19 条向量（数据+脚手架都在 tests/）
"""

from __future__ import annotations

import re
import sys
import zlib
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))  # 同目录 import（-m 方式调用时也要成立）
from check_screenshot import (  # noqa: E402
    DEFAULT_MIN_EDGE_RATIO,
    DEFAULT_MIN_STDDEV,
    DEFAULT_MIN_UNIQUE,
    PngError,
    decode_png,
    measure,
    verdict,
)

#: 这些 `device_type` 等于「不是硬件 GPU」：`Cpu` = WARP/软渲染适配器，`Other` = 未分类（本机那条
#: `Other` 是 Gl 兼容层）。用 `not in` 而不是 `in {DiscreteGpu, IntegratedGpu}`，与
#: `crates/app-iced/src/probe.rs:165-167` 的硬件判据同口径：`VirtualGpu` 之类新词条不会因为本脚本
#: 没登记就被当成「不是硬件」。反过来，把 `Cpu` 从这张禁令里拿掉正是最想抓的回归（向量第 3 条）。
DISALLOWED_DEVICE_TYPES = frozenset({"Cpu", "Other"})

#: 截图行里的 renderer。只认 `[shot]` 行 —— `[probe] renderer = …` 是启动前的意图，不是结果。
SHOT_RENDERER_RE = re.compile(r"^\[shot\][^\n]*?\brenderer=(\S+)", re.MULTILINE)
#: `[shot] 1180x760 scale=1 -> …`：出图行声明的像素尺寸。
SHOT_SIZE_RE = re.compile(r"^\[shot\]\s+(\d+)x(\d+)\b", re.MULTILINE)
#: `[gpu] …compositor: Selected: AdapterInfo { … }`：真正建出来的那块适配器。这两处约束不是洁癖：
#: 每份日志里还有一行 `[probe] 取证：出现 …Selected: AdapterInfo…` 的**提示文本**（含同样几个字、
#: 但没有 `{`），锚 `^\[gpu\]` + 要求紧跟 `{` 才不会被那行骗过去。
SELECTED_RE = re.compile(r"^\[gpu\][^\n]*Selected:\s*AdapterInfo\s*\{(?P<body>[^\n]*)", re.MULTILINE)
ADAPTER_NAME_RE = re.compile(r'name:\s*"([^"]*)"')
DEVICE_TYPE_RE = re.compile(r"device_type:\s*(\w+)")
ADAPTER_BACKEND_RE = re.compile(r"backend:\s*(\w+)")
#: `[backend] final=… result=… … via=…`：后端自己的结论行。
BACKEND_FINAL_RE = re.compile(r"^\[backend\] final=(\S+)\s+result=(\S+)(?P<rest>[^\n]*)", re.MULTILINE)
VIA_RE = re.compile(r"\bvia=(\w+)")

#: 判据编号 → 标签。编号是报告里的次序，也在 `--self-test` 的向量里被逐条点名。
LABELS = {
    1: "截图行 renderer=wgpu",
    2: "Selected: AdapterInfo 的 device_type 不是 Cpu/Other",
    3: "PNG 能解码且非空",
    4: "PNG 宽高 = 日志 [shot] 的宽高",
    5: "后端结论行 final=wgpu result=started",
    6: "PNG 不是空白帧",
    7: "后端结论行 via=refuse",
    8: "没有出图行（[shot] 尺寸 + renderer 都不该有）",
    9: "目标 PNG 不存在（拒绝出图就该一张都没有）",
}


class Report:
    """逐条判据的结论。失败时由 `print_report` 逐条点名，绝不只印一个 FAILED。"""

    def __init__(self) -> None:
        self.items: list[tuple[int, bool, str]] = []

    def add(self, criterion: int, ok: bool, detail: str = "") -> bool:
        self.items.append((criterion, ok, detail))
        return ok

    def problems(self) -> list[str]:
        return [LABELS[criterion] for criterion, ok, _detail in self.items if not ok]

    def print_report(self, log_path: Path, png_path: Path | None) -> None:
        for criterion, ok, detail in self.items:
            print(f"{'[ok]' if ok else '[fail]'} {criterion}. {LABELS[criterion]}" + (f"：{detail}" if detail else ""))
        target = str(log_path) if png_path is None else f"{log_path} + {png_path}"
        problems = self.problems()
        if problems:
            print(f"[fail] 判据没过 {len(problems)}/{len(self.items)} 条（{target}）", file=sys.stderr)
        else:
            print(f"[ok] {len(self.items)} 条判据全过（{target}）")


def read_log(path: Path) -> str:
    """日志按 UTF-8 读、坏字节替换 —— 判据只看 ASCII 标记，不该因为一行中文乱码就崩。"""
    return path.read_text(encoding="utf-8", errors="replace")


def shot_renderers(text: str) -> list[str]:
    return SHOT_RENDERER_RE.findall(text)


def shot_size(text: str) -> tuple[int, int] | None:
    match = SHOT_SIZE_RE.search(text)
    return (int(match.group(1)), int(match.group(2))) if match else None


def _pick(pattern: re.Pattern[str], text: str, default: str | None = "?") -> str | None:
    match = pattern.search(text)
    return match.group(1) if match else default


def selected_adapter(text: str) -> dict[str, str | None] | None:
    """只取 `Selected:` 那一行：`Available adapters: [...]` 里有 12 条 `AdapterInfo`，取错就全错。

    `device_type` 取不到时返回 `None`（**不是** `"?"`）：一旦 iced 改成 pretty-print Debug，
    字段就落不到 `{` 同一行，此时按「取不到 = 判红」处理，不许静默给绿（M249 r1 P2-1）。
    """
    match = SELECTED_RE.search(text)
    if match is None:
        return None
    body = match.group("body")
    return {
        "name": _pick(ADAPTER_NAME_RE, body),
        "device_type": _pick(DEVICE_TYPE_RE, body, None),
        "backend": _pick(ADAPTER_BACKEND_RE, body),
    }


def backend_final(text: str) -> dict[str, str] | None:
    match = BACKEND_FINAL_RE.search(text)
    if match is None:
        return None
    via = VIA_RE.search(match.group("rest"))
    return {"final": match.group(1), "result": match.group(2), "via": via.group(1) if via else ""}


def decode_shot(png_path: Path, report: Report) -> tuple[int, int, dict | None]:
    """解码 PNG 并核对像素数，返回 (宽, 高, 统计)。读不出就记一条失败并返回 0 尺寸。"""
    try:
        width, height, rows, channels = decode_png(png_path)
    except (PngError, OSError, zlib.error) as error:
        report.add(3, False, f"读不出 {png_path}：{error}")
        return 0, 0, None
    pixels = width * height
    report.add(3, pixels > 0, f"{width}x{height} = {pixels} 像素")
    return (width, height, measure(width, height, rows, channels)) if pixels > 0 else (width, height, None)


def judge_evidence(log_path: Path, png_path: Path) -> Report:
    """正例口径：这份日志 + 这张 PNG 能不能当「GPU 档真的出了这一帧」的证据。"""
    report = Report()
    try:
        text = read_log(log_path)
    except OSError as error:
        report.add(1, False, f"日志读不出 {log_path}：{error}")
        return report

    renderers = list(dict.fromkeys(shot_renderers(text)))
    # 「这张图是 wgpu 出的」= 日志里**每一行** `[shot] … renderer=` 都说 wgpu。用 `in` 就退化成
    # 「曾经出现过」：把 CPU 那次的 stderr `>>` 追加进来、配 CPU 那张图，会静默判绿（M249 r1 P2-2）。
    report.add(
        1,
        set(renderers) == {"wgpu"},
        "实际 renderer=" + "/".join(renderers) if renderers else "日志里没有 `[shot] … renderer=…` 行（没走到出图那一步？）",
    )

    adapter = selected_adapter(text)
    if adapter is None:
        report.add(2, False, "日志里没有 `Selected: AdapterInfo { … }`（wgpu 没建成设备，或只打印了 Available adapters）")
    else:
        device_type = adapter["device_type"]
        # `None` = 字段不在 `{` 同一行（多行 Debug 漂移）或字段缺失：按取不到即失败处理，
        # 宁可假红也不许「格式一变就静默给绿」。
        report.add(
            2,
            device_type not in (None, "?") and device_type not in DISALLOWED_DEVICE_TYPES,
            f'device_type={device_type}（{adapter["name"]} / {adapter["backend"]}）'
            if device_type not in (None, "?")
            else "取不到 device_type：`Selected: AdapterInfo {` 与字段不在同一行（多行 Debug），或字段缺失",
        )

    width, height, stats = decode_shot(png_path, report)

    size = shot_size(text)
    if size is None:
        report.add(4, False, "日志里没有 `[shot] WxH …` 出图行，尺寸无从对账")
    elif width == 0:
        report.add(4, False, f"PNG 没解出来，无法核对日志声明的 {size[0]}x{size[1]}")
    else:
        report.add(4, (width, height) == size, f"日志 {size[0]}x{size[1]} vs PNG {width}x{height}")

    final = backend_final(text)
    if final is None:
        report.add(5, False, "日志里没有 `[backend] final=…` 行（ULL_ACCEPTANCE=1 才有）")
    else:
        report.add(
            5,
            final["final"] == "wgpu" and final["result"] == "started",
            f"final={final['final']} result={final['result']}" + (f" via={final['via']}" if final["via"] else ""),
        )

    if stats is None:
        report.add(6, False, "上一步没解出像素，无从统计")
    else:
        blank = verdict(stats, DEFAULT_MIN_UNIQUE, DEFAULT_MIN_STDDEV, DEFAULT_MIN_EDGE_RATIO)
        report.add(
            6,
            not blank,
            f"唯一色 {stats['unique_colors']} 亮度σ {stats['luma_stddev']} 强边缘 {stats['edge_ratio']}",
        )
    return report


def judge_refuse(log_path: Path, png_path: Path | None) -> Report:
    """负例口径：这次启动是不是**确实拒绝**降级出图（而不是悄悄换后端出了一张）。"""
    report = Report()
    try:
        text = read_log(log_path)
    except OSError as error:
        report.add(7, False, f"日志读不出 {log_path}：{error}")
        return report

    final = backend_final(text)
    if final is None:
        report.add(7, False, "日志里没有 `[backend] final=…` 行")
    else:
        report.add(7, final["via"] == "refuse", f"final={final['final']} result={final['result']} via={final['via'] or '<缺>'}")

    renderers = list(dict.fromkeys(shot_renderers(text)))
    size = shot_size(text)
    surplus = (["renderer=" + "/".join(renderers)] if renderers else []) + (
        [f"[shot] {size[0]}x{size[1]}"] if size is not None else []
    )
    report.add(8, not surplus, "存在出图行：" + "、".join(surplus) if surplus else "")

    if png_path is None:
        return report
    if png_path.exists():
        report.add(9, False, f"{png_path} 存在（{png_path.stat().st_size} 字节）")
    else:
        report.add(9, True, f"{png_path} 不存在")
    return report


def main(argv: list[str]) -> int:
    args = list(argv)
    positional: list[str] = []
    expect_refuse = False
    while args:
        arg = args.pop(0)
        if arg == "--expect-refuse":
            expect_refuse = True
        elif arg == "--self-test":
            sys.path.insert(0, str(Path(__file__).resolve().parent / "tests"))
            from gpu_shot_selftest import run_selftest  # 延迟 import：脚手架反过来要拿本模块的判据函数

            return run_selftest(judge_evidence, judge_refuse)
        elif arg in ("-h", "--help"):
            print(__doc__)
            return 0
        elif arg.startswith("-"):
            print(f"::error::不认识的参数：{arg}", file=sys.stderr)
            return 2
        else:
            positional.append(arg)

    if not positional:
        print("::error::至少要给一份 stderr 日志（--help 看用法）", file=sys.stderr)
        return 2
    if len(positional) > 2:
        print(f"::error::最多两个位置参数（日志 + PNG），实际 {len(positional)} 个", file=sys.stderr)
        return 2
    log_path = Path(positional[0])
    png_path = Path(positional[1]) if len(positional) == 2 else None
    if png_path is None and not expect_refuse:
        print("::error::正例必须给出那张 PNG（只有负例可用 --expect-refuse 省略）", file=sys.stderr)
        return 2

    report = judge_refuse(log_path, png_path) if expect_refuse else judge_evidence(log_path, png_path)
    report.print_report(log_path, png_path)
    return 1 if report.problems() else 0


# ────────────────────────── 自测 ──────────────────────────
#
# 向量数据在 `packaging/tests/gpu_shot_vectors.json`，造样本与逐条比对的脚手架在
# `packaging/tests/gpu_shot_selftest.py`（判据脚本自己守 ≤400 行，脚手架不占这里的行数）。
# 每条的 `expect_fail` 是**期望的失败判据清单**，逐条对：关掉一条判据、或让某条判据被别的
# 东西掩盖，清单就对不上。真图不在仓库里（`DevDocs/` 被 ignore），所以这颗钉子守判据，真实
# 产物由报告里的实跑退出码守。

if __name__ == "__main__":
    for stream in (sys.stdout, sys.stderr):
        if hasattr(stream, "reconfigure"):
            stream.reconfigure(errors="backslashreplace")
    sys.exit(main(sys.argv[1:]))
