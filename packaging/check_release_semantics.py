#!/usr/bin/env python3
"""发布标记语义守门（M163）：**push tag 默认创建稳定版**，这条不是靠注释保证的。

四条断言，任何一条不成立即 exit 1（CI 里红在 `.github/workflows/release-guard.yml`）：

1. **标记表**：直接调 `packaging/release_marker.py::marks()`——
   push tag（空串）的 create 标记里不许出现 `--prerelease`；`prerelease=true` 两个分支都带；
   `prerelease=false` 的 create 稳定、edit 显式 `--prerelease=false`；非法取值必须抛。
2. **bash 往返**：把 `release_marker.py --emit-bash` 的输出真的 `source` 进 bash，读回
   `create_mark` / `edit_mark` 两个数组。第 1 条只证明函数对，这一条证明**流水线跑的那段代码**对
   （数组拼错、`source` 漏了、`set -u` 炸掉都在这条里现原形）。
3. **流水线必须用这份表**：`release-publish.yml` 的**可执行内容**里不许有 `--prerelease` 字面量
   （注释里讲清楚为什么可以），必须调用 `--emit-bash`，且两个数组分别喂给 `gh release create/edit`。
   否则表改得再对，流水线也可能自己写一套。
4. **dispatch 默认值 == false**：读 `release.yml` 的 YAML 结构（`on.workflow_dispatch.inputs.prerelease.default`），
   不是 grep 字符串——键被搬走 / 改名 / 缺省会当场红。

`--self-mutation` 是这条守门的自测：把四个文件复制到临时目录，逐项注入真实回归（默认值改回
`true`、publish 里塞回 `--prerelease`、标记表的 push 分支改成预发布），断言守门**每一项都红**，
基线**绿**。变异自测本身失败也 exit 1——「守门抓不住」与「代码坏了」是两件事，不能混。
"""

from __future__ import annotations

import importlib.util
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

PACKAGING = Path(__file__).resolve().parent
ROOT = PACKAGING.parent

RELEASE_YML = Path(".github/workflows/release.yml")
PUBLISH_YML = Path(".github/workflows/release-publish.yml")
MARKER_REL = Path("packaging/release_marker.py")

#: release.yml 里「dispatch 的 prerelease 默认值」在 YAML 结构中的位置。
DEFAULT_PATH = ("on", "workflow_dispatch", "inputs", "prerelease", "default")

#: 三种触发方式各自该展开成什么（与 release_marker.py 的表同源，这里独立写一份是为了对拍）。
EXPECTED = {
    "": ((), ()),
    "true": (("--prerelease",), ("--prerelease",)),
    "false": ((), ("--prerelease=false",)),
}


# ────────────────────────── 1. 标记表（语义断言） ──────────────────────────


def load_marker(root: Path):
    """从 `root` 里加载 release_marker.py（变异自测要能加载被改坏的那份）。"""
    path = root / MARKER_REL
    spec = importlib.util.spec_from_file_location(f"_release_marker_{hash(path)}", path)
    if spec is None or spec.loader is None:
        raise SystemExit(f"::error::加载不了 {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def check_table(root: Path) -> list[str]:
    module = load_marker(root)
    problems: list[str] = []

    for raw, (create, edit) in EXPECTED.items():
        got_create, got_edit = module.marks(raw)
        trigger = "push tag（空串）" if raw == "" else f'dispatch prerelease="{raw}"'
        if tuple(got_create) != create or tuple(got_edit) != edit:
            problems.append(
                f"{trigger}：期望 create={create} edit={edit}，实际 create={tuple(got_create)} edit={tuple(got_edit)}"
            )

    # 单独立一条：这条是 v0.2.2 至今 releases/latest 404 的根因，报告里点名它。
    push_create, _ = module.marks("")
    if "--prerelease" in push_create:
        problems.append("push tag 的 create 标记带上了 --prerelease：稳定通道会一直是 404")

    for bad in ("yes", "1", "TRUE", "False", "0"):
        try:
            module.marks(bad)
        except ValueError:
            continue
        problems.append(f"非法取值 {bad!r} 没有被拒绝（会静默变成某个发布标记）")

    if not module.CASES:
        problems.append("release_marker.CASES 是空的：对外承诺的那张表不见了")
    return problems


# ────────────────────────── 2. bash 往返（真跑那段代码） ──────────────────────────

BASH_ROUNDTRIP = r"""
set -uo pipefail
marks=$(mktemp)
"$PYTHON" "$MARKER" --emit-bash --prerelease "$RAW" > "$marks" || { echo "emit failed"; exit 1; }
# shellcheck disable=SC1090
source "$marks"
printf 'create=%s\n' "${create_mark[*]:-}"
printf 'edit=%s\n' "${edit_mark[*]:-}"
rm -f "$marks"
"""


def find_bash() -> str | None:
    """找到「真的 bash」，而不是 Windows 上 System32 里那个 WSL 壳（它不认这套环境）。"""
    candidates = [
        os.environ.get("BASH"),
        shutil.which("bash"),
        "C:/Program Files/Git/bin/bash.exe",
        "C:/Program Files/Git/usr/bin/bash.exe",
    ]
    for candidate in candidates:
        if not candidate:
            continue
        if "system32" in candidate.replace("/", "\\").lower():
            continue
        if Path(candidate).is_file() or shutil.which(candidate):
            return candidate
    return None


BASH = find_bash()


def bash_marks(marker: Path, raw: str) -> tuple[str, str] | None:
    if BASH is None:
        return None
    # 解释器用 sys.executable 而不是写死 python3：CI（ubuntu）上两者等价，本机 Windows 只有
    # `python`，写死 python3 会让这条断言在本地跑不起来——一条本地跑不了的守门等于没有。
    env = dict(
        os.environ,
        MARKER=str(marker),
        RAW=raw,
        PYTHON=sys.executable.replace("\\", "/"),
        PYTHONIOENCODING="utf-8",
    )
    proc = subprocess.run(
        [BASH, "-c", BASH_ROUNDTRIP], env=env, capture_output=True, text=True, encoding="utf-8"
    )
    if proc.returncode != 0:
        return None
    values = {}
    for line in proc.stdout.splitlines():
        key, _, value = line.partition("=")
        values[key.strip()] = value.strip()
    if "create" not in values or "edit" not in values:
        return None
    return values["create"], values["edit"]


def check_bash_roundtrip(root: Path) -> list[str]:
    marker = root / MARKER_REL
    problems: list[str] = []
    if BASH is None:
        return ["找不到 bash：这条断言要真跑流水线里那段 source，不能跳过（CI 是 ubuntu-latest，一定有）"]

    for raw, (create, edit) in EXPECTED.items():
        got = bash_marks(marker, raw)
        want = (" ".join(create), " ".join(edit))
        trigger = "push tag（空串）" if raw == "" else f'dispatch prerelease="{raw}"'
        if got is None:
            problems.append(f"{trigger}：--emit-bash 的产物 source 进 bash 失败")
        elif got != want:
            problems.append(f"{trigger}：bash 展开成 create={got[0]!r} edit={got[1]!r}，期望 create={want[0]!r} edit={want[1]!r}")
    return problems


# ────────────────────────── 3. 流水线必须用这份表 ──────────────────────────


def executable_text(text: str) -> str:
    """去掉整行注释后的内容：注释里写 `--prerelease` 是解释，可执行位置上出现才是回归。"""
    return "\n".join(line for line in text.splitlines() if not line.lstrip().startswith("#"))


def check_pipeline(root: Path) -> list[str]:
    path = root / PUBLISH_YML
    if not path.is_file():
        return [f"找不到 {PUBLISH_YML}（发布流水线搬家了？这条守门要跟着搬）"]
    text = path.read_text(encoding="utf-8")
    live = executable_text(text)
    problems: list[str] = []

    hits = [
        line.strip()
        for line in live.splitlines()
        # 唯一允许出现它的位置：把值喂给表自己的那一行（`release_marker.py ... --prerelease "$PRERELEASE"`）。
        # 其它任何可执行位置（`gh release` 的旗子、自己写的 case 分支）都是回归。
        if "--prerelease" in line and "release_marker.py" not in line
    ]
    if hits:
        problems.append(
            "release-publish.yml 的可执行内容里出现了 --prerelease 字面量（标记必须只由 "
            "packaging/release_marker.py 产出）：" + " | ".join(hits)
        )
    if "release_marker.py --emit-bash" not in live:
        problems.append("release-publish.yml 没有调用 packaging/release_marker.py --emit-bash")
    for array in ('"${create_mark[@]}"', '"${edit_mark[@]}"'):
        if array not in live:
            problems.append(f"release-publish.yml 没有把 {array} 喂给 gh release（create / edit 各要一个）")
    if "github.event.inputs.prerelease" not in live:
        problems.append("release-publish.yml 不再从 github.event.inputs.prerelease 取值（三态语义的来源没了）")
    return problems


# ────────────────────────── 4. dispatch 默认值 == false ──────────────────────────


def yaml_scalars(text: str) -> dict[tuple[str, ...], tuple[str, int]]:
    """极简 YAML 结构读取：只认「缩进 → key: value」的标量行，返回 {路径: (值, 行号)}。

    不引第三方依赖（CI 上只有裸 python3）：本守门要读的是「某个键在哪个缩进层级上」，不是
    完整 YAML 语义。块标量（`run: |`）的内容整段跳过，免得脚本里的 `a: b` 混进来。
    """
    out: dict[tuple[str, ...], tuple[str, int]] = {}
    stack: list[tuple[int, str]] = []
    block_indent: int | None = None
    for lineno, raw in enumerate(text.splitlines(), 1):
        if not raw.strip() or raw.lstrip().startswith("#"):
            continue
        indent = len(raw) - len(raw.lstrip(" "))
        if block_indent is not None:
            if indent > block_indent:
                continue
            block_indent = None
        stripped = raw.strip()
        if stripped == "-" or stripped.startswith("- "):
            continue
        if ":" not in stripped:
            continue
        key, _, value = stripped.partition(":")
        key = key.strip().strip("'\"")
        value = re.sub(r"\s+#.*$", "", value.strip())
        while stack and stack[-1][0] >= indent:
            stack.pop()
        path = tuple(k for _, k in stack) + (key,)
        if value in ("", "|", ">", "|-", ">-"):
            stack.append((indent, key))
            if value:
                block_indent = indent
        else:
            out[path] = (value, lineno)
    return out


def check_default(root: Path) -> list[str]:
    path = root / RELEASE_YML
    if not path.is_file():
        return [f"找不到 {RELEASE_YML}"]
    scalars = yaml_scalars(path.read_text(encoding="utf-8"))
    if DEFAULT_PATH not in scalars:
        return [
            "release.yml 里读不到 "
            + ".".join(DEFAULT_PATH)
            + "：dispatch 的 prerelease 输入被搬走/改名/删了（默认值判据必须留在原处）"
        ]
    value, lineno = scalars[DEFAULT_PATH]
    if value.strip().strip("'\"").lower() != "false":
        return [
            f"release.yml:{lineno} dispatch 的 prerelease 默认值是 {value!r}，必须是 false"
            "（默认值一旦回 true，Actions 页面点一次的产物就是预发布）"
        ]
    return []


# ────────────────────────── 汇总 ──────────────────────────


def run_checks(root: Path) -> list[str]:
    problems: list[str] = []
    for name, fn in (
        ("标记表（push tag ⇒ 稳定版）", check_table),
        ("bash 往返（流水线真跑的那段）", check_bash_roundtrip),
        ("流水线只从表里取标记", check_pipeline),
        ("dispatch 默认值 == false", check_default),
    ):
        found = fn(root)
        problems.extend(f"{name}：{item}" for item in found)
    return problems


def report(root: Path) -> int:
    problems = run_checks(root)
    for item in problems:
        print(f"::error::{item}", file=sys.stderr)
    if problems:
        print(f"发布标记语义守门：{len(problems)} 条不通过", file=sys.stderr)
        return 1
    marker = load_marker(root)
    print("OK: 发布标记语义守门全绿")
    print(marker.table())
    return 0


# ────────────────────────── 变异自测 ──────────────────────────


def snapshot(dst: Path) -> None:
    """把守门依赖的四个文件复制进 `dst`（只复制这四个，保持临时树最小）。"""
    for rel in (RELEASE_YML, PUBLISH_YML, MARKER_REL, Path("packaging") / Path(__file__).name):
        target = dst / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / rel, target)


def mutate_default_true(root: Path) -> str:
    path = root / RELEASE_YML
    text = path.read_text(encoding="utf-8")
    scalars = yaml_scalars(text)
    _, lineno = scalars[DEFAULT_PATH]
    lines = text.splitlines()
    lines[lineno - 1] = lines[lineno - 1].replace("false", "true")
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return "release.yml 的 dispatch 默认值改回 true"


def mutate_inline_flag(root: Path) -> str:
    path = root / PUBLISH_YML
    text = path.read_text(encoding="utf-8")
    path.write_text(text + "          create_mark+=(--prerelease)\n", encoding="utf-8")
    return "release-publish.yml 里塞回 --prerelease 字面量"


def mutate_marker_push(root: Path) -> str:
    path = root / MARKER_REL
    text = path.read_text(encoding="utf-8")
    old = '    if value == "":\n        return (), ()\n'
    new = '    if value == "":\n        return (PRERELEASE,), ()\n'
    if old not in text:
        raise SystemExit(f"::error::变异自测自身失效：{MARKER_REL} 里找不到 push 分支的原样代码")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")
    return "标记表的 push tag 分支改成预发布"


MUTATIONS = (mutate_default_true, mutate_inline_flag, mutate_marker_push)


def self_mutation() -> int:
    with tempfile.TemporaryDirectory(prefix="ull-release-guard-") as tmp:
        baseline = Path(tmp) / "baseline"
        snapshot(baseline)
        problems = run_checks(baseline)
        if problems:
            for item in problems:
                print(f"::error::变异自测的基线本该是绿的：{item}", file=sys.stderr)
            return 1
        print("OK: 基线绿")

        failures = 0
        for index, mutate in enumerate(MUTATIONS):
            root = Path(tmp) / f"mutation-{index}"
            snapshot(root)
            label = mutate(root)
            if not run_checks(root):
                print(f"::error::变异自测失败：注入「{label}」后守门仍然绿（这条守门抓不住它）", file=sys.stderr)
                failures += 1
            else:
                print(f"OK: 注入「{label}」→ 守门红")
        if failures:
            return 1
        print(f"OK: {len(MUTATIONS)} 项变异全部被抓红，基线绿")
        return 0


def main(argv: list[str]) -> int:
    for stream in (sys.stdout, sys.stderr):
        if hasattr(stream, "reconfigure"):
            stream.reconfigure(errors="backslashreplace")

    root = ROOT
    args = list(argv)
    if "--root" in args:
        index = args.index("--root")
        root = Path(args[index + 1]).resolve()
        del args[index : index + 2]
    if "--self-mutation" in args:
        return self_mutation()
    unknown = [arg for arg in args if arg not in ("-h", "--help")]
    if unknown:
        print(f"::error::不认识的参数：{unknown}", file=sys.stderr)
        return 2
    return report(root)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
