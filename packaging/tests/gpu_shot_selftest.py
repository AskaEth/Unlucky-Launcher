#!/usr/bin/env python3
"""`packaging/check_gpu_shot.py --self-test` 的执行器（M249）：合成日志 + 合成 PNG + 向量比对。

为什么单独一份而不是塞进判据脚本：那一份要守 ≤400 行硬线，而这里全是「按真实行格式手拼日志、
现造 PNG、逐条对 `expect_fail`」的**测试脚手架**，与判据逻辑无关。数据另外放
`gpu_shot_vectors.json`（同 `guard_vectors.json` 的做法），被测的判据函数由调用方**以参数传入**
——本文件不反向 import `check_gpu_shot`，免得循环导入。

一颗诚实的钉子：本文件的样本是**按今天的日志格式**手拼的，所以它守**判据**，不守**格式漂移**。
产品真换了 Debug 形状时，靠的是判据里「取不到 device_type 就算失败」这类保守口径（见 doc §3）。
"""

from __future__ import annotations

import json
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))  # `check_screenshot` 在 packaging/
from check_screenshot import encode_png  # noqa: E402

#: 向量数据。`log` 是 `synth_log` 的关键字覆盖（缺省键取函数默认值，`null` = 该行不出现）。
VECTORS_PATH = Path(__file__).resolve().parent / "gpu_shot_vectors.json"

SELFTEST_SIZE = (200, 120)


def synth_log(
    *,
    renderer: str | None = "wgpu",
    device_type: str | None = "DiscreteGpu",
    backend: str | None = "final=wgpu result=started",
    size: tuple[int, int] | None = None,
    no_size_line: bool = False,
    multiline_selected: bool = False,
    empty_selected: bool = False,
    extra_shot_renderer: str | None = None,
) -> str:
    """按 M246 真实日志的**行格式**拼一份最小日志（传 `None` = 该行故意不出现）。"""
    lines = [
        "[probe] renderer = wgpu reason=forced-gpu 兜底=tiny-skia（失败会先打印具名错误）",
        # 真实日志里永远有这一行提示文本，且它含 `Selected: AdapterInfo` 字样 —— 留着它，
        # 「只有提示、没有真 Selected 行」的向量才是在测真东西。
        "[probe] 取证：出现 `[gpu] iced_wgpu: …Selected: AdapterInfo…` ⇒ GPU 真生效；出现 `[gpu] wgpu 初始化失败` ⇒ 已降级 CPU",
    ]
    if empty_selected:  # `Selected: AdapterInfo {` 之后再没有内容（字段全丢）
        lines.append("[gpu] iced_wgpu::window::compositor: Selected: AdapterInfo {")
    elif device_type is not None:
        fields = [
            '     name: "AMD Radeon RX 6600 XT",',
            f"     device_type: {device_type},",
            '     backend: Vulkan,',
        ]
        if multiline_selected:  # pretty-print Debug：字段不在 `{` 那一行（P2-1 的形状）
            lines.append("[gpu] iced_wgpu::window::compositor: Selected: AdapterInfo {")
            lines.extend(fields)
            lines.append("}")
        else:
            lines.append("[gpu] iced_wgpu::window::compositor: Selected: AdapterInfo {" + "".join(fields) + " }")
    if not no_size_line:
        dims = size or SELFTEST_SIZE
        lines.append(f"[shot] {dims[0]}x{dims[1]} scale=1 -> selftest.png")
    for value in (renderer, extra_shot_renderer):
        if value is not None:
            lines.append(f'[shot] screen=gallery theme=light renderer={value} route=Detail("survival")')
    if backend is not None:
        lines.append(f"[backend] {backend} elapsed=1000ms evidence=selftest")
    return "\n".join(lines) + "\n"


def synth_png(kind: str, size: tuple[int, int]) -> bytes:
    """`content` = 有内容的合成帧（渐变 + 8px 棋盘格）；`flat` = 全白。"""
    width, height = size

    def content_row(y: int) -> bytes:
        return b"".join(
            bytes((x * 255 // max(width - 1, 1), y * 255 // max(height - 1, 1), 40))
            if ((x // 8) + (y // 8)) % 2 == 0
            else b"\x14\x14\x14"
            for x in range(width)
        )

    if kind == "flat":
        return encode_png(width, height, lambda _y: b"\xff" * (width * 3))
    return encode_png(width, height, content_row)


def run_vector(root: Path, index: int, case: dict, default_size: tuple[int, int], judges: tuple) -> list[str]:
    """跑一条向量，返回它实际红的判据清单。`judges` = (judge_evidence, judge_refuse)。"""
    judge_evidence, judge_refuse = judges
    overrides = {key: (tuple(value) if isinstance(value, list) else value) for key, value in case.get("log", {}).items()}
    log_path = root / f"case{index:02d}.log"
    log_path.write_text(synth_log(**{"size": default_size, **overrides}), encoding="utf-8")
    png_path = root / f"case{index:02d}.png"
    png_size = tuple(case.get("png_size", default_size))
    if case.get("png") == "truncated":
        png_path.write_bytes(synth_png("content", png_size)[:200])
    elif case.get("png") is not None:
        png_path.write_bytes(synth_png(case["png"], png_size))
    report = judge_refuse(log_path, png_path) if case.get("mode") == "refuse" else judge_evidence(log_path, png_path)
    return report.problems()


def run_selftest(judge_evidence, judge_refuse) -> int:
    """跑 `gpu_shot_vectors.json` 的全部向量，逐条对 `expect_fail`（多一条少一条都算失败）。"""
    data = json.loads(VECTORS_PATH.read_text(encoding="utf-8"))
    default_size = tuple(data["size"])
    failures = 0
    with tempfile.TemporaryDirectory(prefix="ull-gpu-shot-") as tmp:
        root = Path(tmp)
        for index, case in enumerate(data["cases"], start=1):
            got = run_vector(root, index, case, default_size, (judge_evidence, judge_refuse))
            want = case["expect_fail"]
            if got != want:
                print(f"::error::自测失败：{case['label']} 期望失败 {want}，实际 {got}", file=sys.stderr)
                failures += 1
            else:
                print(f"OK: {case['label']} → {'红 ' + ' / '.join(got) if got else '绿'}")
    red = sum(1 for case in data["cases"] if case["expect_fail"])
    if failures:
        return 1
    print(f"OK: GPU 截图证据判定自测通过（{len(data['cases'])} 条向量：红 {red} / 绿 {len(data['cases']) - red}）")
    return 0
