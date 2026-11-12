#!/usr/bin/env python3
"""`signing-status.yml` 的**接线**自测（M255 r1 从 `signing_ready_selftest.py` 分出）。

拆成两份的唯一原因是全仓 400 行红线（`guards.yml` 的 `file-size`）：判据本体（三行读数 / `0/1/2` /
两个方向的退出码变异）留在 `signing_ready_selftest.py`，这里只管**接线与文案里的指针**。

钉住的判据（每条都对应一次真实会咬人的回归）：

| 判据 | 抓什么 |
|---|---|
| 不是门禁 | 没有别的 workflow `uses:` 它；全文无 error 级注解；就绪度那步抓 `rc=$?` 后 `exit 0`；文件头两句口径都在 |
| **生产种子只在 main 注入** | 注入行必须是 `github.ref == 'refs/heads/main' && secrets.<KEY_ENV>` 这个条件式；`on:` 块里**不许出现 `pull_request`**；非 main 另有 `ULL_SIGNING_KEY_WITHHELD` 说明原因（于是 ② 读「未判定」而不是「没配」） |
| 文档指针真的存在 | `signing-runbook.md` / `signing.md` 里出现的 `packaging/…`、`crates/…`、`.github/…` 路径必须真在仓库里（r1 P2-3：`signing-ready.yml` 那个文件名根本不存在） |
| 变异必须红 | ①去掉 ref 门 ②加回 `pull_request` ③把文档里一个路径改坏 —— 三处各自重放，**对应用例必须报红** |

为什么要 ref 门（这是本文件存在的最重要理由）：生产签名种子能给**全量已安装客户端**签出被信任的清单
（自更新链 = 用户端 RCE）。仓库内分支的 PR 与任意分支的 `workflow_dispatch` 都**拿得到**仓库 Secret
（只有 fork / Dependabot 拿不到），而 CI 会在**那个 PR 自己的代码**上执行 checker。既有设计把种子锁在
tag ref 后面（`release.yml` 的 `if: … github.ref_type == 'tag'`），播报文件不许在旁边开一条新路。

    python3 packaging/tests/signing_ready_wiring_selftest.py
"""

from __future__ import annotations

import re
import sys

from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
WORKFLOW_DIR = ROOT / ".github/workflows"
WORKFLOW = WORKFLOW_DIR / "signing-status.yml"
RUNBOOK = ROOT / "packaging/docs/signing-runbook.md"
SIGNING_MD = ROOT / "packaging/docs/signing.md"

sys.path.insert(0, str(ROOT / "packaging"))
# 环境变量名从 checker 取，不写第二份字面量：两处漂开时，「ref 门注入了 A、checker 读 B」会静默失效。
from check_signing_ready import KEY_ENV, WITHHELD_ENV  # noqa: E402

MAIN_REF = "refs/heads/main"
GATE = f"github.ref == '{MAIN_REF}' && secrets.{KEY_ENV}"
#: 文档里允许出现的「路径」形状：仓库根的四个前缀（相对路径如 `docs/x.md` 不在此列，避免误判）。
PATH_RE = re.compile(r"(?:packaging|crates|\.github|\.cargo)/[A-Za-z0-9_.\-]+(?:/[A-Za-z0-9_.\-]+)*")


def on_block(text: str) -> str:
    """取 `on:` 那一段（到下一个顶格键为止），并**去掉注释行**——注释里会正当地提到 `pull_request`。"""
    lines = text.splitlines()
    start = next((i for i, line in enumerate(lines) if re.match(r"^on:\s*$", line)), None)
    if start is None:
        return ""
    body: list[str] = []
    for line in lines[start + 1 :]:  # 跳过 `on:` 那一行本身（它顶格，会让下面的 break 立刻触发）
        if line.startswith(" ") or not line:
            if not line.strip().startswith("#"):
                body.append(line)
            continue
        break
    return "\n".join(body)


def check_workflow(text: str) -> list[str]:
    """返回该 workflow 文本没过的接线判据（空 = 通过）。变异用例把同一函数套在改坏的副本上。"""
    problems: list[str] = []
    for phrase in ("播报，不是门禁", "不应被设为 required"):
        if phrase not in text:
            problems.append(f"文件头必须写清「{phrase}」（不阻断要是可核的事实，不是靠没接线）")
    if "::error::" in text:
        problems.append("播报 workflow 里不许出现 error 级注解（会让人以为构建挂了，与播报定位冲突）")
    if "rc=$?" not in text or "exit 0" not in text:
        problems.append("就绪度那一步必须抓 `rc=$?` 后 `exit 0`（未就绪不许让 job 变红）")
    # ── 安全口径：生产签名种子只在 main 注入 ──
    if GATE not in text:
        problems.append(f"注入行必须是 `{GATE} || ''` 这个条件式（去掉 ref 门 = 把种子交给任意分支的代码）")
    if f"{WITHHELD_ENV}:" not in text:
        problems.append(f"非 main 分支必须给 `{WITHHELD_ENV}` 一个原因串（否则 ② 会报假的「没配」）")
    if "pull_request" in on_block(text):
        problems.append("`on:` 块里不许出现 `pull_request`（仓库内分支的 PR 拿得到 Secret，而 CI 跑的是 PR 自己的代码）")
    return problems


def wired_elsewhere() -> list[str]:
    """扫全部 workflow：有人 `uses:` 本文件 ⇒ 播报变成了别人的必需检查。"""
    hits: list[str] = []
    for path in sorted(WORKFLOW_DIR.glob("*.y*ml")):
        if path.resolve() == WORKFLOW.resolve():
            continue
        for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
            if re.match(r"^\s*uses:\s*", line) and WORKFLOW.name in line:
                hits.append(f"{path.relative_to(ROOT).as_posix()}:{number}: {line.strip()}")
    return hits


def check_doc_paths(text: str, where: str) -> list[str]:
    """文档里出现的仓库路径必须真实存在（r1 P2-3：指针错会把人送到无关文件/章节）。"""
    problems: list[str] = []
    for token in sorted(set(PATH_RE.findall(text))):
        if not (ROOT / token).exists():
            problems.append(f"{where} 里写了不存在的路径：{token}")
    return problems


def case_wiring(failures: list[str]) -> None:
    """真实文件的接线判据 + 文档路径存在性。"""
    if not WORKFLOW.is_file():
        failures.append(f"接线：{WORKFLOW} 不存在")
        return
    text = WORKFLOW.read_text(encoding="utf-8")
    failures.extend(f"接线：{item}" for item in check_workflow(text))
    hits = wired_elsewhere()
    if hits:
        failures.append("接线：有 workflow `uses:` 了 " + WORKFLOW.name + " ⇒ 播报变成了别人的必需检查：\n  " + "\n  ".join(hits))
    for doc in (RUNBOOK, SIGNING_MD):
        failures.extend(f"指针：{item}" for item in check_doc_paths(doc.read_text(encoding="utf-8"), doc.name))


def case_mutation(failures: list[str]) -> None:
    """变异重放：把接线改坏，**同一组判据原样套上去必须报红**（r1 收尾要求的第 ④ 项）。"""
    real = WORKFLOW.read_text(encoding="utf-8")
    if GATE not in real:
        failures.append("变异：真实文件里就没有 ref 门，先修 P2-1 再谈变异")
        return
    vectors = [
        (
            "去掉 ref 门（种子对任意 ref 可见）",
            real.replace(GATE, f"secrets.{KEY_ENV}"),
            "ref 门",
        ),
        (
            "加回 pull_request 触发",
            real.replace("  workflow_dispatch:\n", "  pull_request:\n  workflow_dispatch:\n", 1),
            "pull_request",
        ),
        (
            "去掉非 main 的 WITHHELD 说明",
            "\n".join(line for line in real.splitlines() if WITHHELD_ENV not in line) + "\n",
            WITHHELD_ENV,
        ),
    ]
    for label, mutated, expect in vectors:
        if mutated == real:
            failures.append(f"变异「{label}」没改到任何字节（变异无处下手，用例什么都没证明）")
            continue
        problems = check_workflow(mutated)
        if not any(expect in item for item in problems):
            failures.append(f"变异「{label}」没有被抓：期望命中「{expect}」，实际 {problems or '零问题'}")
    # 文档指针的变异：把 runbook 里一个真实路径改成不存在的名字，同一判据必须报红。
    runbook = RUNBOOK.read_text(encoding="utf-8")
    broken = runbook.replace("packaging/docs/signing.md", "packaging/docs/signing-oldext.md", 1)
    if broken == runbook:
        failures.append("变异「文档路径」：runbook 里找不到可替换的路径，用例失效")
    elif not check_doc_paths(broken, "mutated-runbook"):
        failures.append("变异「文档路径」：改成不存在的路径后判据仍然是绿的 —— 路径存在性没被判")


def main() -> int:
    failures: list[str] = []
    case_mutation(failures)
    case_wiring(failures)

    for line in failures:
        print(f"FAIL {line}", file=sys.stderr)
    print("signing-status wiring selftest:", "ok" if not failures else f"{len(failures)} 处失败")
    return 0 if not failures else 1


if __name__ == "__main__":
    raise SystemExit(main())
