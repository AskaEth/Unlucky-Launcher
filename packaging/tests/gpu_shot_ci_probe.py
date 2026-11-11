#!/usr/bin/env python3
"""`gpu-evidence.yml` 的**功能断言**部分（M253 / r1+r2 复审返工）：把 workflow 里的 `run:` 原文抽出来真跑。

为什么单独一份：判据脚本自己守 ≤400 行硬线，而这里全是「把 `run:` 落盘、用桩 env 起 bash、读
`$GITHUB_OUTPUT` / `$GITHUB_STEP_SUMMARY` / 退出码」的**执行脚手架**，与「哪条判据该红」的判断无关
（同 `gpu_shot_selftest.py` 与 `check_gpu_shot.py` 的分工）。

两组功能断言，各自对应一类「绿而没判」：

**A. `detect` 步（`probe_problems`）—— 跳过要留痕、红线不许退化成跳过。**
字符串存在性检查被 r1 复审证伪过：旧版只要求「同一步的 run 里同时出现 4 个字符串」，于是把
`available` 写成恒 `false`（门禁永久关闭、每次运行都绿）它照样绿 —— **文案都在，执行路径却没走到**。

| # | 输入 | 必须发生什么 |
|---|------|-------------|
| S1 | 无产物（CI 常态） | `available=false` + summary 写「跳过 / 未验证」+ stdout 有 `::warning::` |
| S2 | 约定产物成对 | `available=true` |
| S3 | 约定产物半套（只有日志） | `available=false`（半套不算可用） |
| S4 | dispatch 成对 | `available=true` |
| S5 | dispatch 点名不存在 | `exit≠0`，且**不写** `available`（写了就等于退化成跳过） |
| S6 | dispatch 正例缺 PNG | `exit≠0`，同上 |
| S7 | dispatch 负例只给日志 | `available=true`（`--expect-refuse` 允许省略 PNG） |

**B. `judge` 步（`judge_problems`）—— 判据步必须真的把红判成红。**
r2 复审实测：判据命令后面加 `|| echo "先忽略"`、`; :`、去掉 pipefail 再 `| cat`、`if ! python3 …`、
`true  # python3 …`（保留子串锚点），五种写法的判据步**全 exit 0**（绿而没判），而标记表判据一条都没抓住。
所以这里跑两次：正例成对必须 `exit 0`，CPU 档日志（`renderer=tiny-skia`）必须 `exit≠0`。第 2 条一红就
同时封掉「吞码」整类，不依赖任何标记表。

**跑的是原文，只改一个令牌**：`run` 原文里 `python3` 换成本进程的解释器（Windows 上没有 `python3`；
控制流、运算符、参数一字不改）。夹具用 `gpu_shot_selftest.synth_log/synth_png`，不另造第二套。
判据步的 `cwd` 是仓库根（它要 `packaging/check_gpu_shot.py` 的相对路径）。

不降级：找不到能跑的 `bash` 就**判红**（「认不出/跑不了」≠「没问题」）。
"""

from __future__ import annotations

import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

PACKAGING = Path(__file__).resolve().parent.parent
REPO = PACKAGING.parent
sys.path[:0] = [str(PACKAGING), str(Path(__file__).resolve().parent)]
from gpu_shot_selftest import SELFTEST_SIZE, synth_log, synth_png  # noqa: E402

_CACHE: dict[str, list[str]] = {}
PYTHON_TOKEN = re.compile(r"\bpython3\b")


def _find_bash() -> str | None:
    """挑一个**真能跑**的 bash。Windows 上 `bash.exe` 可能是 WSL 的 relay（没有 `/bin/bash`），要验一次。"""
    candidates = [os.environ.get("BASH"), shutil.which("bash")]
    if os.name == "nt":
        candidates += [r"C:\Program Files\Git\bin\bash.exe", r"C:\Program Files\Git\usr\bin\bash.exe"]
    candidates.append("bash")
    for candidate in candidates:
        if not candidate:
            continue
        try:
            done = subprocess.run([candidate, "-c", "echo ok"], capture_output=True, text=True, timeout=60)
        except (OSError, subprocess.SubprocessError):
            continue
        if done.returncode == 0 and "ok" in (done.stdout or ""):
            return candidate
    return None


def _run(bash: str, script: Path, workdir: Path, env_extra: dict, cwd: str | None = None, flags: tuple[str, ...] = ("-e",)):
    """跑一份 `run:` 原文，返回 `(CompletedProcess, GITHUB_OUTPUT 字典, GITHUB_STEP_SUMMARY 文本)`。

    `flags` 默认 `-e`（GitHub 的 `bash -e {0}`）；判据步还会用**裸 bash**（不带 `-e`）再跑一遍 ——
    `... ; :` 这种写法只有在不靠 `-e` 兜底时才会把红吞掉（实测：`bash -e` 仍 exit 1，裸 bash exit 0）。
    """
    out_path, sum_path = workdir / "gh_output", workdir / "gh_summary"
    env = os.environ.copy()
    env.update({"GITHUB_OUTPUT": out_path.as_posix(), "GITHUB_STEP_SUMMARY": sum_path.as_posix()})
    env.update({key: ("" if value is None else str(value)) for key, value in env_extra.items()})
    done = subprocess.run([bash, *flags, str(script)], cwd=cwd or str(workdir), env=env, capture_output=True, text=True, encoding="utf-8", errors="replace")
    outputs: dict[str, str] = {}
    if out_path.exists():
        for line in out_path.read_text(encoding="utf-8", errors="replace").splitlines():
            key, sep, value = line.partition("=")
            if sep:
                outputs[key.strip()] = value
    summary = sum_path.read_text(encoding="utf-8", errors="replace") if sum_path.exists() else ""
    return done, outputs, summary


def _case(label: str, done, outputs: dict, summary: str, *, available: str, contains: tuple[str, ...] = (), warning: bool = False) -> list[str]:
    problems: list[str] = []
    if done.returncode != 0:
        problems.append(f"{label}：退出码 {done.returncode}，应为 0（stderr: {(done.stderr or '').strip()[:200]}）")
    if outputs.get("available") != available:
        problems.append(f"{label}：`available={outputs.get('available')!r}`，应为 {available!r} —— 探测结论被改坏了，门就永久开或永久关")
    for needle in contains:
        if needle not in summary:
            problems.append(f"{label}：job summary 里缺 `{needle}` —— 跳过不再留痕")
    if warning and "::warning::" not in (done.stdout or ""):
        problems.append(f"{label}：stdout 里没有 `::warning::` —— 跳过变成了静默 pass")
    return problems


def _probe_scenarios(bash: str, script_text: str) -> list[str]:
    problems: list[str] = []
    with tempfile.TemporaryDirectory(prefix="ull-gpu-ci-", ignore_cleanup_errors=True) as tmp:
        root = Path(tmp)
        fixtures = root / "fixtures"
        fixtures.mkdir()
        log = fixtures / "a1-gpu.stderr.log"
        log.write_text("[shot] 200x120\n", encoding="utf-8")
        png = fixtures / "a1-gpu.png"
        png.write_bytes(b"\x89PNG\r\n\x1a\n")

        def case(slug: str, tree: tuple[tuple[str, bytes], ...] = ()):
            workdir = root / slug
            workdir.mkdir()
            for rel, payload in tree:
                path = workdir / rel
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(payload)
            script = workdir / "probe.sh"
            script.write_text(script_text, encoding="utf-8")
            return script, workdir

        base = {"EVENT": "push", "SHOT_LOG": "", "SHOT_PNG": "", "EXPECT_REFUSE": None}
        convention = "DevDocs/artifacts/gpu-probe/gpu-shot"
        script, workdir = case("s1-none")
        problems += _case("S1 无产物（CI 常态）", *_run(bash, script, workdir, base), available="false", contains=("跳过", "未验证"), warning=True)
        script, workdir = case("s2-convention-pair", ((f"{convention}.stderr.log", b"x"), (f"{convention}.png", b"x")))
        problems += _case("S2 约定产物成对", *_run(bash, script, workdir, base), available="true")
        script, workdir = case("s3-convention-half", ((f"{convention}.stderr.log", b"x"),))
        problems += _case("S3 约定产物半套（只有日志）", *_run(bash, script, workdir, base), available="false", contains=("跳过",))
        script, workdir = case("s4-dispatch-pair")
        problems += _case("S4 dispatch 成对", *_run(bash, script, workdir, dict(base, SHOT_LOG=str(log), SHOT_PNG=str(png))), available="true")
        for slug, label, extra in (
            ("s5-dispatch-missing", "S5 dispatch 点名不存在", {"SHOT_LOG": str(root / "nope.log"), "SHOT_PNG": str(png)}),
            ("s6-dispatch-no-png", "S6 dispatch 正例缺 PNG", {"SHOT_LOG": str(log), "SHOT_PNG": ""}),
        ):
            script, workdir = case(slug)
            done, outputs, _summary = _run(bash, script, workdir, dict(base, **extra))
            if done.returncode == 0:
                problems.append(f"{label}：退出码 0 —— 「明确说要判、东西不在」退化成了跳过（假绿）")
            if outputs:
                problems.append(f"{label}：该红却写了 `$GITHUB_OUTPUT`（{outputs}）")
        script, workdir = case("s7-dispatch-refuse")
        problems += _case("S7 dispatch 负例只给日志", *_run(bash, script, workdir, dict(base, SHOT_LOG=str(log), SHOT_PNG="", EXPECT_REFUSE="true")), available="true")
    return problems


def probe_problems(script_text: str) -> list[str]:
    """判据 A：`id: detect` 那一步的 `run` 原文实跑 7 种输入（带缓存，同文本只跑一次）。"""
    if not script_text.strip():
        return ["`id: detect` 那一步没有 `run:` —— 跳过路径无从执行"]
    if script_text not in _CACHE:
        bash = _find_bash()
        _CACHE[script_text] = (
            ["找不到能跑的 `bash`：跳过路径的功能断言无法执行，本判据不降级（认不出就红）"]
            if not bash
            else _probe_scenarios(bash, script_text)
        )
    return _CACHE[script_text]


def _judge_scenarios(bash: str, script_text: str) -> list[str]:
    problems: list[str] = []
    with tempfile.TemporaryDirectory(prefix="ull-gpu-judge-", ignore_cleanup_errors=True) as tmp:
        root = Path(tmp)
        passing = root / "pass.stderr.log"
        passing.write_text(synth_log(size=SELFTEST_SIZE), encoding="utf-8")
        cpu = root / "cpu.stderr.log"
        cpu.write_text(synth_log(renderer="tiny-skia", size=SELFTEST_SIZE), encoding="utf-8")
        png = root / "pass.png"
        png.write_bytes(synth_png("content", SELFTEST_SIZE))
        script = root / "judge.sh"
        script.write_text(PYTHON_TOKEN.sub(lambda _match: f'"{Path(sys.executable).as_posix()}"', script_text), encoding="utf-8")
        common = {"PNG": png.as_posix(), "EXPECT_REFUSE": "false"}
        # 两种模式都跑：`-e` = GitHub 的默认 shell；裸 bash = 不靠 `-e` 兜底（`; :` 只在后者现形）。
        for mode, flags in (("bash -e", ("-e",)), ("裸 bash", ())):
            for label, log_path, want_zero in (
                ("正例（六条判据全过）", passing, True),
                ("CPU 档日志（`renderer=tiny-skia`，必须判红）", cpu, False),
            ):
                done, _outputs, _summary = _run(bash, script, root, dict(common, LOG=log_path.as_posix()), cwd=str(REPO), flags=flags)
                if want_zero and done.returncode != 0:
                    problems.append(
                        f"[{mode}] 判据步对**正例**退出码 {done.returncode}，应为 0 —— 「判据全过」这条路走不通了"
                        f"（stderr: {(done.stderr or '').strip()[:200]}）"
                    )
                if not want_zero and done.returncode == 0:
                    problems.append(
                        f"[{mode}] 判据步对**必须判红**的输入退出码 0 —— 「绿而没判」：`|| echo` / `; :` / 管道 / `if !` / "
                        "`true  # 原行` / 裸 `exit 0` 都属于这一类"
                    )
    return problems


def judge_problems(script_text: str) -> list[str]:
    """判据 B：`id: judge` 那一步的 `run` 原文实跑两次（正例必须 0、CPU 档必须非 0）。"""
    if not script_text.strip():
        return ["`id: judge` 那一步没有 `run:` —— 判据步无从执行"]
    key = "judge:" + script_text
    if key not in _CACHE:
        bash = _find_bash()
        _CACHE[key] = (
            ["找不到能跑的 `bash`：判据步的功能断言无法执行，本判据不降级（认不出就红）"]
            if not bash
            else _judge_scenarios(bash, script_text)
        )
    return _CACHE[key]
