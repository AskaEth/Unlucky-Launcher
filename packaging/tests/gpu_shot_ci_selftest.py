#!/usr/bin/env python3
r"""`gpu-evidence.yml` 的**接线自检**（M253 / r1+r2 复审返工）：把「这道门禁怎么才算没被改绿」变成判据。

历史（留着，防止下一个人再写绝对化结论）：
- r1 版宣称「不可能被改成总是绿」，复审造 19 条变异**漏 11 条**（最要命：`available` 恒 false ⇒ 门永久
  关闭而每次绿）—— 根因是判据只做字符串存在性检查、自测步被整条豁免、`is True` 挡不住 `'true'`。
- r2 版改成 6 条判据后，复审 40 条新变异**仍漏 11 条**：job outputs 的 `available` 绑到 `reason`/字面量
  （门永久关、每次绿）、判据步 `|| echo "先忽略"` / `; :` / `| cat` / `if !` / `true # 原行`（**绿而没判**，
  真 bash 实测 exit 0）、自检步被指向另一个脚本（自检根本不跑）、`on:` 触发面被删。
  教训：**「不吞退出码」不能靠标记表**，得把 `run` 原文真跑一遍。

现在十条判据，能结构化的不靠字符串、能跑的不靠读文本：

| # | 判据 | 怎么判 |
|---|------|--------|
| 1 | 五步必须在场 | 按 `id:` 认 `vectors`/`exitcodes`/`wiring`/`detect`/`judge`，各恰好一步；前四步不许有 `if:`（step 或 job 级）；`exitcodes` 必须真断言 `expect 0/1/2` |
| 2 | 自测步跑的是对脚本、且是单条命令 | `wiring` 的 `run` 必须含 `gpu_shot_ci_selftest.py`；`vectors`/`wiring` 的 `run` 不许有换行、`\|`、`&`、`;`、反引号、`#`、`$(` —— 否则红了之后一行 `\|\| echo 先忽略` 就能关掉它 |
| 3 | 判定必须挂门 | **解析 `if:` 表达式后求值**：`available` 取 `'true'` 必须真、`'false'`/`''` 必须假；job 级与 step 级各自都要满足；认不出的表达式一律红 |
| 4 | 判定用探测给的产物 | `run` 里两种调用形态都在场（`"$LOG" "$PNG"` 与 `"$LOG" --expect-refuse`），`env` 来自 `needs.probe.outputs.*` |
| 5 | 判据步**真跑两次** | `judge` 的 `run` 原文实跑：正例成对必须 `exit 0`，CPU 档日志（`renderer=tiny-skia`）必须 `exit≠0` —— 一次封掉整类「吞码」 |
| 6 | 跳过路径**真跑 7 种输入** | `detect` 的 `run` 原文实跑：`available` 的值、summary 文案、`::warning::`、三条红线必须 `exit≠0` 且不写 outputs |
| 7 | 禁 `continue-on-error` | 按 YAML 布尔语义：`true` / `'true'` / `${{ true }}` / 任意表达式都算开启 |
| 8 | 不吞退出码（标记表） | 扫所有 `python`+`packaging/` 的 `run`：`\|\| true` / `; true` / `\|\| exit 0` / 裸 `exit 0`（判据 5 的功能断言才是主守，这张表只是兜底） |
| 9 | `probe.outputs` 必须绑到 `detect` | `available`/`log`/`png` 三个键的值必须**逐个**等于 `${{ steps.detect.outputs.<同名> }}` |
| 10 | 触发面 | `on:` 必须同时有 `push`（含 `main`）、`pull_request`、`workflow_call` |

## 覆盖的「改绿」类别（封闭清单：10 类，33 条变异向量）

变异数据在 `packaging/tests/gpu_shot_ci_vectors.json`，**按解析后的结构注入**（`step_delete` /
`job_delete` / `step_set` / `job_set` / `path_set` / `trigger_set` / `run_append` / `run_replace`，
`also` 可叠加），每条都必须让对应判据当场红、基线必须全绿。类别与来源：

1. 步骤被删 / 被 `if:` 挡住 / 被 `continue-on-error` 兜住（r1 M7/M9/M10/M13/M15/M16/M17/M18、r2 N15/N18）；
2. 自测步指向别的脚本、或写成能被 `|| echo` 关掉的多行/带运算符命令（r2 N40/N41/N23）；
3. 判定的门被写成恒真（`|| always()`、`if: true`、step 级 `if: ${{ always() }}`、删门）（r1 M3/M19、r2 N4/N5）；
4. 判定的产物路径被写死或调用形态被换掉（r1 M20、r2 N9）；
5. 判据步的退出码被吞 / 分支没走到（r1 M4、r2 N22/N24/N25/N26/N8'—— 都用真 bash 实跑判据步验证）；
6. `detect` 的结论被改坏（`available` 恒 false、跳过不留痕、删 `$GITHUB_OUTPUT`）（r1 M11/M12/M5/M6/M14、r2 N6/N7/N20）；
7. `probe.outputs.*` 被改指向别的 step 输出或字面量（**r2 N28/N29**）；
8. `on:` 触发面被削（删 `push`/`pull_request`/`workflow_call`）（**r2 N11/N12**）；
9. 排版变动**不误红**（加注释/空行基线仍绿）、变异锚点坏掉**报「向量失效」并红**（r2 N16/N17）；
10. `if:` 求值器不接受近似写法（`!=`、`!(…)`、`fromJSON`、未知标识符一律红）（r2 N1/N2/N3）。

## 明确**不覆盖**（边界，逐条给理由）

- **改判据脚本本身**：能改这个文件的人也能改判据，它不自守。它只防「顺手改绿」，不防恶意改绿。
- **载体依赖**：把整个 `gpu-evidence.yml` 删掉/换成别的文件，就没有任何东西会跑这个自检 —— 守门人住在
  被守对象自己的文件里。`guards.yml` 已经 399/400 行（`file-size` 那道门就扫 `*.yml`），塞不进去。
- **`if:` 求值器与 GitHub 语义的差异**：只认 `==`/`!=`/`&&`/`||`/括号/字符串/`needs.<job>.outputs.<名>`/
  `true|false|null`/`always()|success()|failure()|cancelled()`（后四者取常量）；其余认不出就是红。
- **判据 4 的「合法调用替换」**：把正例分支换成另一种**合法**调用（`"$PNG" --expect-refuse`）抓不到；
  后果是假红不是假绿，判据 5 也会把它判红（正例跑成非 0）。
- **`exitcodes` 那一步的断言被削弱**（例如末尾加 `|| echo`）：它是**见证步**，不是门；门的绿由判据 5/6
  的功能断言守住，见证步被削只影响「退出码语义也被验过」这条证据。
- **判据 6 的 7 种输入之外的分支**（例如 `detect` 里新增的第五个分支）。
- **真 Actions 的行为**：`if:` 的真求值、job skip 的呈现、required status check 与 skipped 的交互，
  本脚本一概不管（`if:` 语义是本脚本的小求值器模拟的）。

## 自我定位与止步线

本文件**不宣称**「不可能被改成总是绿」，只宣称「上面那张封闭清单（10 类 / 33 条向量）会被抓」。
**这条门禁自 2026-11-11 起停止加固**：再往下就会无限递归（守卫守守卫守守卫），边际收益已很低。
后续若要扩覆盖面，必须由**人类新开 mission**，不要在同一轮里继续加判据。

    python3 packaging/tests/gpu_shot_ci_selftest.py
"""

from __future__ import annotations

import copy
import json
import re
import sys
from pathlib import Path

PACKAGING = Path(__file__).resolve().parent.parent
WORKFLOW = PACKAGING.parent / ".github/workflows/gpu-evidence.yml"
VECTORS_PATH = Path(__file__).resolve().parent / "gpu_shot_ci_vectors.json"
sys.path.insert(0, str(PACKAGING))
sys.path.insert(0, str(Path(__file__).resolve().parent))
from check_workflows_yaml import load_yaml  # noqa: E402
from gpu_shot_ci_expr import ConditionError, evaluate_condition  # noqa: E402
from gpu_shot_ci_probe import judge_problems, probe_problems  # noqa: E402

#: 必须在场、且必须无条件的步骤（按 `id:` 认，不按步骤名认）。
REQUIRED_STEPS = {
    "vectors": "判据实现自测（`--self-test`，19 条向量）",
    "exitcodes": "退出码语义（真 CLI 走 0/1/2）",
    "wiring": "接线自检（本脚本，必须在场）",
    "detect": "产物探测（probe 的唯一步骤）",
    "judge": "真实产物判据（evidence 的步骤）",
}
UNCONDITIONAL = ("vectors", "exitcodes", "wiring", "detect")
PROBE_OUTPUT_KEYS = ("available", "log", "png")
SINGLE_COMMAND_STEPS = ("vectors", "wiring")
SHELL_OPERATOR = re.compile(r"[|&;`#$()<>\n]")
SWALLOW_MARKS = ("|| true", "; true", "|| exit 0", "exit 0")
PYTHON_RE = re.compile(r"\bpython3?\b")


def coerce_flag(value) -> bool:
    """YAML 里的开关按**真语义**判：`true` / `'true'` / `${{ true }}` 都算开启。"""
    if value is True:
        return True
    if value is False or value is None:
        return False
    if isinstance(value, (int, float)):
        return bool(value)
    return str(value).strip().lower() not in {"false", "${{ false }}", "0", "no", "off", ""}


def steps(doc: dict) -> list[tuple[str, dict, dict]]:
    found: list[tuple[str, dict, dict]] = []
    for job_id, job in (doc.get("jobs") or {}).items():
        for step in (job or {}).get("steps") or []:
            if isinstance(step, dict):
                found.append((job_id, job, step))
    return found


def step_by_id(doc: dict, sid: str) -> tuple[str, dict, dict] | None:
    found = [entry for entry in steps(doc) if entry[2].get("id") == sid]
    return found[0] if len(found) == 1 else None


def run_of(doc: dict, sid: str) -> str:
    entry = step_by_id(doc, sid)
    return (entry[2].get("run") or "") if entry else ""


def check_presence(doc: dict) -> list[str]:
    problems: list[str] = []
    counts = {sid: len([e for e in steps(doc) if e[2].get("id") == sid]) for sid in REQUIRED_STEPS}
    for sid, label in REQUIRED_STEPS.items():
        if counts[sid] != 1:
            problems.append(f"`id: {sid}`（{label}）必须恰好一步，实际 {counts[sid]} 步 —— 删掉或复制都算红")
            continue
        job_id, job, step = step_by_id(doc, sid)
        if sid in UNCONDITIONAL and ("if" in step or "if" in job):
            problems.append(f"`id: {sid}`（{label}）带 `if:`（步骤或 job 级）—— 这四步必须无条件跑，否则门禁可被静默关掉")
    asserted = set(re.findall(r"\bexpect\s+([0-9]+)\b", run_of(doc, "exitcodes")))
    missing = {"0", "1", "2"} - asserted
    if step_by_id(doc, "exitcodes") and missing:
        problems.append(f"`id: exitcodes` 没有断言退出码 {'/'.join(sorted(missing))}（`expect <码>`）—— 三个码就是门禁的接口")
    return problems


def check_selftest_commands(doc: dict) -> list[str]:
    problems: list[str] = []
    wiring = run_of(doc, "wiring").strip()
    if wiring and "gpu_shot_ci_selftest.py" not in wiring:
        problems.append("`id: wiring` 的 `run` 里没有 `gpu_shot_ci_selftest.py` —— 它可能指向别的脚本（自检根本没跑）")
    for sid in SINGLE_COMMAND_STEPS:
        run = run_of(doc, sid).strip()
        if not run:
            continue
        if SHELL_OPERATOR.search(run):
            problems.append(
                f"`id: {sid}` 的 `run` 必须是**单条命令**（不许换行 / `|` / `&` / `;` / 反引号 / `#` / `$(`）"
                f"—— 否则红了之后一行 `|| echo 先忽略` 就能把它关掉：`{run}`"
            )
    return problems


def check_judge_gate(doc: dict) -> list[str]:
    entry = step_by_id(doc, "judge")
    if entry is None:
        return []  # presence 已经报过
    job_id, job, step = entry
    gates: list[tuple[str, object]] = []
    if "if" in job:
        gates.append((f"job `{job_id}` 的 `if`", job.get("if")))
    if "if" in step:
        gates.append(("`id: judge` 步的 `if`", step.get("if")))
    if not gates:
        return ["`judge` 步与它的 job 都没有 `if:` —— 无产物时也会照跑，「跳过」与「通过」分不开"]
    problems: list[str] = []
    for label, raw in gates:
        for available, want in (("true", True), ("false", False), ("", False)):
            try:
                got = evaluate_condition(raw, available)
            except ConditionError as error:
                problems.append(f"{label} 认不出（{error}）—— 认不出即红：`{raw}`")
                break
            if got is not want:
                problems.append(f"{label} 在 `available={available!r}` 时求值为 {got}，应为 {want}：`{raw}`")
    return problems


def check_judge_invocation(doc: dict) -> list[str]:
    entry = step_by_id(doc, "judge")
    if entry is None:
        return []
    run = entry[2].get("run") or ""
    env = entry[2].get("env") or {}
    problems: list[str] = []
    for form in ('check_gpu_shot.py "$LOG" "$PNG"', 'check_gpu_shot.py "$LOG" --expect-refuse'):
        if form not in run:
            problems.append(f"`id: judge` 的 `run` 里缺调用形态 `{form}` —— 该分支可以「什么都没判」就绿")
    for name, ref in (("LOG", "needs.probe.outputs.log"), ("PNG", "needs.probe.outputs.png")):
        if ref not in str(env.get(name, "")):
            problems.append(f"`judge` 的 `{name}` 不是来自 `{ref}`（写死的路径会在跳过那条路上判错东西）")
    return problems


def check_probe_outputs(doc: dict) -> list[str]:
    outputs = (doc.get("jobs") or {}).get("probe", {}).get("outputs") or {}
    problems: list[str] = []
    for key in PROBE_OUTPUT_KEYS:
        want = "${{ steps.detect.outputs.%s }}" % key
        got = str(outputs.get(key, "")).strip()
        if got != want:
            problems.append(
                f"`probe.outputs.{key}` 必须恰好是 `{want}`，实际 {got!r} —— "
                "改指向（`reason` / 字面量 `'false'`）会让门永久关掉，而每次运行都绿"
            )
    return problems


def check_triggers(doc: dict) -> list[str]:
    on_block = doc.get("on", doc.get(True))
    if not isinstance(on_block, dict):
        return ["`on:` 不是一个映射 —— 触发面认不出（认不出即红）"]
    problems: list[str] = []
    if "push" not in on_block:
        problems.append("`on:` 少了 `push` —— main 上的门禁会停摆")
    else:
        branches = (on_block.get("push") or {}).get("branches") if isinstance(on_block.get("push"), dict) else None
        if branches is not None and "main" not in [str(branch) for branch in branches]:
            problems.append("`on.push.branches` 里没有 `main` —— main 上的门禁会停摆")
    if "pull_request" not in on_block:
        problems.append("`on:` 少了 `pull_request` —— PR 上的门禁会停摆")
    if "workflow_call" not in on_block:
        problems.append("`on:` 少了 `workflow_call` —— `dev-guards.yml` 的 `uses:` 会加载失败")
    return problems


def check_continue_on_error(doc: dict) -> list[str]:
    problems: list[str] = []
    for job_id, job in (doc.get("jobs") or {}).items():
        if coerce_flag((job or {}).get("continue-on-error")):
            problems.append(f"job `{job_id}` 带 `continue-on-error: {job.get('continue-on-error')!r}` —— 判据红也会变绿")
        for step in (job or {}).get("steps") or []:
            if isinstance(step, dict) and coerce_flag(step.get("continue-on-error")):
                problems.append(f"job `{job_id}` 的 `{step.get('id') or step.get('name')}` 带 `continue-on-error: {step.get('continue-on-error')!r}`")
    return problems


def check_no_swallow(doc: dict) -> list[str]:
    problems: list[str] = []
    for job_id, _job, step in steps(doc):
        run = step.get("run") or ""
        if not (PYTHON_RE.search(run) and "packaging/" in run):
            continue
        hits = [mark for mark in SWALLOW_MARKS if mark in run]
        if hits:
            problems.append(f"job `{job_id}` 的 `{step.get('id') or step.get('name')}` 里出现 {'/'.join(f'`{h}`' for h in hits)} —— 退出码被吞掉，判据红也绿")
    return problems


CHECKS = {
    "1 五步必须在场": check_presence,
    "2 自测步跑的是对脚本且是单条命令": check_selftest_commands,
    "3 判定必须挂门": check_judge_gate,
    "4 判定用探测给的产物": check_judge_invocation,
    "5 判据步真跑（正例 0 / CPU 档非 0）": lambda doc: judge_problems(run_of(doc, "judge")),
    "6 跳过路径真跑（7 种输入）": lambda doc: probe_problems(run_of(doc, "detect")),
    "7 禁 continue-on-error": check_continue_on_error,
    "8 不吞退出码（标记表，兜底）": check_no_swallow,
    "9 probe.outputs 绑定 detect": check_probe_outputs,
    "10 触发面（push/pull_request/workflow_call）": check_triggers,
}


def evaluate(doc: dict) -> dict[str, list[str]]:
    if not isinstance(doc, dict) or not isinstance(doc.get("jobs"), dict):
        return {name: ["workflow 不是一个带 jobs 的 YAML 映射"] for name in CHECKS}
    return {name: check(doc) for name, check in CHECKS.items()}


def _walk(doc: dict, path: list) -> tuple[dict, object]:
    node = doc
    for key in path[:-1]:
        node = node[key]
    return node, path[-1]


def apply_vector(doc: dict, vector: dict) -> dict:
    """按**解析后的结构**注入变异（不碰文本格式，改动 workflow 的排版不会让向量过期）。"""
    op = vector["op"]
    target = str(vector.get("target", ""))
    on_key = "on" if "on" in doc else True
    if op in ("step_delete", "step_set", "run_append", "run_replace"):
        entry = step_by_id(doc, target)
        if entry is None:
            raise KeyError(f"找不到唯一的 `id: {target}`")
        step = entry[2]
        if op == "step_delete":
            entry[1]["steps"].remove(step)
        elif op == "step_set":
            step[vector["key"]] = vector["value"]
        else:
            text = step.get("run") or ""
            if op == "run_append":
                step["run"] = text + vector["value"]
            else:
                find, replacement, occurrence = vector["find"], vector["replace"], int(vector.get("occurrence", 1))
                pos = -1
                for _ in range(occurrence):
                    pos = text.find(find, pos + 1)
                    if pos < 0:
                        raise KeyError(f"`id: {target}` 的 run 里找不到第 {occurrence} 处 `{find}`")
                step["run"] = text[:pos] + replacement + text[pos + len(find) :]
    elif op in ("job_delete", "job_set"):
        if target not in (doc.get("jobs") or {}):
            raise KeyError(f"没有 job `{target}`")
        if op == "job_delete":
            del doc["jobs"][target]
        else:
            doc["jobs"][target][vector["key"]] = vector["value"]
    elif op == "path_set":
        parent, key = _walk(doc, vector["path"])
        if key not in parent:
            raise KeyError(f"路径不存在：{vector['path']}")
        parent[key] = vector["value"]
    elif op == "trigger_set":
        doc[on_key] = vector["value"]
    else:
        raise KeyError(f"不认识的变异操作 `{op}`")
    for extra in vector.get("also", []):
        apply_vector(doc, extra)
    return doc


def main() -> int:
    yaml = load_yaml()
    if not WORKFLOW.is_file():
        print(f"::error file={WORKFLOW}::接线自检的靶子不存在", file=sys.stderr)
        return 1
    doc = yaml.safe_load(WORKFLOW.read_text(encoding="utf-8"))
    failures = 0
    for name, problems in evaluate(doc).items():
        if problems:
            failures += 1
            print(f"FAIL: 判据「{name}」没过：", file=sys.stderr)
            for problem in problems:
                print(f"  - {problem}", file=sys.stderr)
        else:
            print(f"OK: 判据「{name}」")

    vectors = json.loads(VECTORS_PATH.read_text(encoding="utf-8"))["vectors"]
    for vector in vectors:
        label = f"{vector['id']} {vector['label']}"
        try:
            mutated = apply_vector(copy.deepcopy(doc), vector)
        except Exception as error:  # noqa: BLE001 - 向量失效（锚点没了）同样算红，逼着人更新它
            failures += 1
            print(f"FAIL: 变异「{label}」注入失败（向量失效）：{error}", file=sys.stderr)
            continue
        broken = [name for name, problems in evaluate(mutated).items() if problems]
        if vector["expect"] not in broken:
            failures += 1
            print(f"FAIL: 变异「{label}」没被抓到（应红「{vector['expect']}」，实际红的是 {broken or '无'}）", file=sys.stderr)
        else:
            print(f"OK: 变异「{label}」→ 「{vector['expect']}」当场红")

    if failures:
        print(f"FAIL：{failures} 处没过（{WORKFLOW.name}）", file=sys.stderr)
        return 1
    print(f"ok: {len(CHECKS)} 条接线判据全过，{len(vectors)} 条变异向量全部被抓（{WORKFLOW.name}）")
    print("   封闭清单 10 类（见 docstring）；不覆盖：改判据脚本 / 删掉整个 workflow（载体依赖）/ 求值器语义差 / 合法调用替换。")
    print("   本门禁自 2026-11-11 起停止加固，扩项须由人类新开 mission。")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
