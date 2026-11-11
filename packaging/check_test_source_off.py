#!/usr/bin/env python3
"""测试通道守门（M169，r3：allowlist 改成**结构判定 + 默认拒绝**）。

`update-test-source` 让 `core::update::job::default_source` 可被两个环境变量指向任意本地目录，而更新
链路终点是**替换用户盘上的 exe**。「默认关 + 编译期不存在」是唯一可靠边界，所以「谁也没在流水线里
把它打开」必须是自动断言。

四条断言（任一不成立即 exit 1，CI 里红在 `.github/workflows/release-guard.yml`）：

1. **workflow / 复合 action 里不许打开它**。唯一例外：`.github/workflows/ci.yml` 里**名为
   `update-e2e` 的那个列表项**，且该项自己的文本必须带 `--test update_e2e_test`。归属按
   `jobs.*.steps[]` 的**列表项**判（见 `split_items`），不是「最近一行 `- name:`」——后者会把追加到
   文件末尾的无名步骤 / 无名 job 的步骤 / flow 风格步骤算进上一块（r2 复审 P2-1 实测三条全绿）。
   **任何落不到具名步骤上的行（`jobs:` 之前的孤儿行、项外键、无名项）一律判红**：默认拒绝。
2. **不许「打开所有 feature」**：`--all-features`、`cargo hack --feature-powerset|--each-feature|--all`。
3. **不许设成默认**：`Cargo.toml` 里它只许是**自己的声明**（含跨行数组）；进 `default` 或别的
   feature 的依赖数组即红。
4. **扫描面逐项列举**（`WORKFLOW_GLOBS` / `SCRIPT_GLOBS` / `BUILD_ENTRY_GLOBS` / `CONFIG_GLOBS`）：
   含 `.github/actions/**`、`packaging/**` 递归子目录、`Makefile`/`justfile`/`Taskfile`、以及
   `.cargo/config*.toml`（`--cfg feature="…"` 与 `rustc-wrapper` 都能绕过 `--features`，见
   `scan_cargo_config`）。

注释剥离是**引号感知**的（`echo "#"; cargo build --features …` 里那个 `#` 不是注释）。抓不住的都是
**故意手打**或**运行期数据流**（完整清单见 `packaging/README.md` §14）：变量拼接、两步 `$GITHUB_ENV`
交接（第二步没有字面量——静态扫描的固有边界）、feature 改名后接回、`build.rs` 里 spawn cargo、改名复用。

`--self-test` 把每条断言的**真实回归**注入临时副本（数据在 `packaging/tests/guard_vectors.json`），
断言该红的红、该绿的绿。守门自己抓不住回归时它会先红。
"""

from __future__ import annotations

import argparse
import json
import re
import sys
import tempfile
from pathlib import Path

#: 被守的 feature 名（改名就得同步改本文件与各 `Cargo.toml`）。
FEATURE = "update-test-source"
#: 仓库根（本文件在 `packaging/` 下）与本文件自身（扫描时跳过：它必须写出 feature 名）。
ROOT = Path(__file__).resolve().parent.parent
GUARD = Path(__file__).resolve()
#: 扫描面（逐个列举，不用 `**` 全仓：那会把 `target/` 拖进来）。
WORKFLOW_GLOBS = (
    ".github/workflows/**/*.yml",
    ".github/workflows/**/*.yaml",
    ".github/actions/**/*.yml",
    ".github/actions/**/*.yaml",
)
SCRIPT_GLOBS = ("packaging/**/*.py", "packaging/**/*.sh", "packaging/**/*.ps1")
BUILD_ENTRY_GLOBS = (
    "Makefile",
    "justfile",
    "Justfile",
    "Taskfile.yml",
    "Taskfile.yaml",
    "crates/*/Makefile",
    "packaging/Makefile",
)
MANIFEST_GLOBS = ("Cargo.toml", "crates/*/Cargo.toml")
#: cargo 配置：`--cfg feature="…"` 与 `rustc-wrapper` 都能在没有 `--features` 时打开这条通道（P2-2）。
CONFIG_GLOBS = (
    ".cargo/config.toml",
    ".cargo/config",
    "crates/*/.cargo/config.toml",
    "crates/*/.cargo/config",
)
#: 任意 wrapper 都能注入 `--cfg`，文本扫描看不见 ⇒ 一律判红。
WRAPPER_KEY = re.compile(r"^\s*(rustc|rustc-workspace)-wrapper\s*=")
#: 唯一允许出现该 feature 的地方：**文件 + 列表项名 + 必须同在的字符串**，三者不符即红。
ALLOWED_STEP = {
    "workflow": ".github/workflows/ci.yml",
    "name": "update-e2e",
    "must_contain": "--test update_e2e_test",
}
#: 「打开全部 feature」的开关。`--all` 后面不许跟 `-`/字母（否则 `--all-targets` 会被误杀）。
ALL_FEATURES = (
    re.compile(r"--all-features\b"),
    re.compile(r"--feature-powerset\b"),
    re.compile(r"--each-feature\b"),
    re.compile(r"\bcargo\s+hack\b[^\n]*\s--all(?![-\w])"),
)
#: YAML 列表项（任意缩进）；名字只认键位置上的 `name:`（块风格 / flow 风格）。
LIST_ITEM = re.compile(r"^(\s*)-\s+")
NAME_BLOCK = re.compile(r"^['\"]?name['\"]?\s*:\s*(.+)$")
NAME_FLOW = re.compile(r"^\{[^{}]*?\bname\s*:\s*([^,}]+)")
#: Cargo.toml 里「自己的声明」：行首 `update-test-source = [`。
DECLARATION = re.compile(rf"^\s*{re.escape(FEATURE)}\s*=\s*\[")
#: 块标量开头（`run: |` / `run: >-`）：其后的更深缩进行是**内容**，不是键。
BLOCK_SCALAR = ("|", ">", "|-", ">-", "|+", ">+")


def strip_comment(line: str) -> str:
    """引号感知地剥注释：`#` 只有「行首或空白之后」且不在引号内时才开始注释。"""
    quote = None
    for index, char in enumerate(line):
        if quote is not None:
            if char == quote:
                quote = None
        elif char in "\"'":
            quote = char
        elif char == "#" and (index == 0 or line[index - 1].isspace()):
            return line[:index]
    return line


def dangerous_flag(code: str) -> str | None:
    """这一行有没有「打开全部 feature」的开关（返回命中的开关，便于写清原因）。"""
    for pattern in ALL_FEATURES:
        match = pattern.search(code)
        if match:
            return match.group(0)
    return None


def _lines(path: Path) -> list[tuple[int, str]]:
    return list(enumerate(path.read_text(encoding="utf-8").splitlines(), start=1))


def _name_of(key_indent: int, body: list[tuple[int, str]]) -> str | None:
    """列表项的名字：项内**同级**的 `name:` 键（键序无关；块标量内容不当键）。

    「同级」= 项首行 `- ` 之后那一列（`item.end() == key_indent`，r3 复审 P2-1）：更深缩进的
    `- name:`（`with:` / `env:` 子列表、`run: |` 内容）**不算**项名。找不到、或出现两个**不同**的
    `name:` 都返回 `None` ⇒ 落不到具名步骤 ⇒ 带 feature 就红（默认拒绝）。
    """
    in_block = False
    found: str | None = None
    for _number, line in body:
        text = line.strip()
        if not text or text.startswith("#"):
            continue
        indent = len(line) - len(line.lstrip(" "))
        item = LIST_ITEM.match(line)
        if in_block and indent > key_indent:
            continue
        in_block = False
        if item and item.end() == key_indent:
            code = strip_comment(line[item.end() :])  # 项首行：`      - name: x` → `name: x`
        elif not item and indent == key_indent:
            code = strip_comment(text)
        else:
            continue
        for pattern in (NAME_BLOCK, NAME_FLOW):
            match = pattern.match(code.strip())
            if match:
                value = match.group(1).strip().rstrip("}").strip("'\"")
                if found is not None and found != value:
                    return None
                found = value
                break
        if code.rstrip().endswith(BLOCK_SCALAR):
            in_block = True
    return found


def split_items(
    lines: list[tuple[int, str]],
) -> tuple[list[tuple[str | None, list[tuple[int, str]]]], list[tuple[int, str]]]:
    """按 YAML **列表项**切块，返回（(项名, 该项的行), 孤儿行）。

    每个 `- ` 起一项；更深的行归当前项，**同级或更浅的非列表行结束当前项**（job 键 / `steps:` /
    顶层键）。落不到列表项上的行是孤儿——不属于任何步骤，带 feature 就红。
    """
    items: list[tuple[str | None, list[tuple[int, str]]]] = []
    orphans: list[tuple[int, str]] = []
    body: list[tuple[int, str]] = []
    base, key_indent = 0, 0
    for number, line in lines:
        text = line.strip()
        if not text or text.startswith("#"):
            (body if body else orphans).append((number, line))
            continue
        indent = len(line) - len(line.lstrip(" "))
        item = LIST_ITEM.match(line)
        if item and (not body or indent <= base):
            if body:
                items.append((_name_of(key_indent, body), body))
            body, base, key_indent = [(number, line)], indent, item.end()
            continue
        if body and indent > base:
            body.append((number, line))
            continue
        if body:
            items.append((_name_of(key_indent, body), body))
            body = []
        orphans.append((number, line))
    if body:
        items.append((_name_of(key_indent, body), body))
    return items, orphans


def _reason(where: str, name: str | None, text: str) -> str | None:
    """`None` = 这一项被放行；否则返回「为什么红」的一句话（按最具体的那条给）。"""
    if where != ALLOWED_STEP["workflow"]:
        return f"`{FEATURE}` 只许出现在 `{ALLOWED_STEP['workflow']}` 的 `{ALLOWED_STEP['name']}` 步骤里（这里是 `{where}`）"
    if name is None:
        return f"这一项没有同级 `name:`，落不到具名步骤（默认拒绝）；`{where}` 里只有 `{ALLOWED_STEP['name']}` 可以用它"
    if name != ALLOWED_STEP["name"]:
        return f"`{where}` 里只有名为 `{ALLOWED_STEP['name']}` 的步骤可以用 `{FEATURE}`（这一项叫 `{name}`）"
    if ALLOWED_STEP["must_contain"] not in text:
        return f"`{where}` 的 `{name}` 步骤少了 `{ALLOWED_STEP['must_contain']}`"
    return None


def _flag_problems(where: str, body: list[tuple[int, str]]) -> list[str]:
    problems: list[str] = []
    for number, line in body:
        flag = dangerous_flag(strip_comment(line))
        if flag:
            problems.append(
                f"::error file={where}:{number}::{where} 用了 `{flag}`：它会把 `{FEATURE}` 一起打开"
            )
    return problems


def scan_workflow(path: Path, root: Path) -> list[str]:
    """扫 workflow / 复合 action：feature 只许出现在 allowlist 点名的那一项里。"""
    where = path.relative_to(root).as_posix()
    items, orphans = split_items(_lines(path))
    problems = _flag_problems(where, orphans)
    for number, line in orphans:
        if FEATURE in strip_comment(line):
            problems.append(
                f"::error file={where}:{number}::{where} 这一行落不到任何具名步骤（默认拒绝）；"
                "官方发布产物里不许有测试通道"
            )
    for name, body in items:
        problems += _flag_problems(where, body)
        codes = [(number, strip_comment(line)) for number, line in body]
        hits = [(number, code) for number, code in codes if FEATURE in code]
        if not hits:
            continue
        reason = _reason(where, name, "\n".join(code for _number, code in codes))
        if reason is None:
            continue
        for number, _code in hits:
            problems.append(
                f"::error file={where}:{number}::{reason}；"
                "官方发布产物里不许有测试通道（它能把更新源指向任意本地目录）"
            )
    return problems


def scan_code_file(path: Path, root: Path) -> list[str]:
    """扫脚本 / 构建入口 / cargo config：没有 allowlist，字面量或开关出现即红。"""
    where = path.relative_to(root).as_posix()
    problems: list[str] = []
    for number, line in _lines(path):
        code = strip_comment(line)
        if not code:
            continue
        if FEATURE in code:
            problems.append(
                f"::error file={where}:{number}::{where} 打开了 `{FEATURE}`："
                "官方发布产物里不许有测试通道（它能把更新源指向任意本地目录）"
            )
        flag = dangerous_flag(code)
        if flag:
            problems.append(
                f"::error file={where}:{number}::{where} 用了 `{flag}`：它会把 `{FEATURE}` 一起打开"
            )
    return problems


def scan_manifest(path: Path, root: Path) -> list[str]:
    """扫 Cargo.toml：feature 只许出现在自己的声明里（**含跨行数组**），不许进别的列表。"""
    where = path.relative_to(root).as_posix()
    problems: list[str] = []
    in_declaration = False
    for number, line in _lines(path):
        code = strip_comment(line).strip()
        if in_declaration:
            in_declaration = "]" not in code
            continue
        if FEATURE not in code:
            continue
        if DECLARATION.match(code):
            in_declaration = "]" not in code
            continue
        problems.append(
            f"::error file={where}:{number}::{where} 把 `{FEATURE}` 写进了别的 feature 列表："
            "只许它是自己的声明（可跨行），不许进 `default` 或别的 feature"
        )
    return problems


def scan_cargo_config(path: Path, root: Path) -> list[str]:
    """扫 `.cargo/config*.toml`：`--cfg feature="…"` 与 `rustc-wrapper` 都能绕过 `--features`。

    r3 复审 P2-2 实测：`RUSTFLAGS='--cfg=feature="update-test-source"'` 能让 `#[cfg(feature = …)]`
    段在没有 `--features` 时编进来。`[build] rustflags` / `[env]` 靠 feature 字面量抓（走
    `scan_code_file`）；`rustc-wrapper` 抓不住（注入什么由程序决定）⇒ 一律判红，合法用途要 review 同意。
    """
    problems = scan_code_file(path, root)
    where = path.relative_to(root).as_posix()
    for number, line in _lines(path):
        if WRAPPER_KEY.match(strip_comment(line)):
            problems.append(
                f"::error file={where}:{number}::{where} 设了 rustc wrapper：wrapper 能注入任意 "
                f"`--cfg`，静态扫描看不见它是否打开了 `{FEATURE}`（默认拒绝）"
            )
    return problems


def scan(root: Path) -> list[str]:
    """扫整个仓库，返回所有问题（空 = 绿）。"""
    problems: list[str] = []
    for patterns, scanner in (
        (WORKFLOW_GLOBS, scan_workflow),
        (SCRIPT_GLOBS, scan_code_file),
        (BUILD_ENTRY_GLOBS, scan_code_file),
        (CONFIG_GLOBS, scan_cargo_config),
        (MANIFEST_GLOBS, scan_manifest),
    ):
        for path in sorted({p for pattern in patterns for p in root.glob(pattern)}):
            if scanner is scan_code_file and path.resolve() == GUARD:
                continue
            try:
                problems += scanner(path, root)
            except OSError as error:
                problems.append(f"::error file={path}::读不到 {path}：{error}")
    return problems


# ────────────────────────── 变异自测 ──────────────────────────

#: 自测数据（基线 + 向量）单独放一份 JSON：`packaging/tests/manifest.golden.json` 是同一手法。
#: 每个向量 = 一份「覆盖/新增的文件」；期望红的是真实回归，期望绿的是防误杀（合法写法不许红）。
#: 条目标签自带出处（`allowlist:` / `P2-1` / `P2-2` / `误杀` / `Cargo`），红的时候一眼看得出守哪条。
VECTORS_FILE = Path(__file__).resolve().parent / "tests" / "guard_vectors.json"


def _load_vectors() -> tuple[dict[str, str], list[tuple[str, dict[str, str], str]]]:
    data = json.loads(VECTORS_FILE.read_text(encoding="utf-8"))
    return data["baseline"], [
        (item["label"], item["files"], item["expect"]) for item in data["vectors"]
    ]


def _scan_case(files: dict[str, str]) -> list[str]:
    with tempfile.TemporaryDirectory(prefix="ull-test-source-guard-") as temp:
        root = Path(temp)
        for relative, text in files.items():
            path = root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text, encoding="utf-8")
        return scan(root)


def self_test() -> int:
    try:
        base_files, vectors = _load_vectors()
    except (OSError, ValueError, KeyError) as error:
        print(f"::error::读不到自测向量 {VECTORS_FILE}：{error}", file=sys.stderr)
        return 1
    failures: list[str] = []
    baseline = _scan_case(base_files)
    if baseline:
        failures.append(f"基线应当绿，实际红了：{baseline}")
    for label, changes, expect in vectors:
        files = dict(base_files)
        files.update(changes)
        hits = _scan_case(files)
        if expect == "red" and not hits:
            failures.append(f"{label}：注入了真实回归，守门却没红（抓不住）")
        if expect == "green" and hits:
            failures.append(f"{label}：应当放行，实际红了（误杀）：{hits}")
    if failures:
        for failure in failures:
            print(f"::error::{failure}", file=sys.stderr)
        return 1
    reds = sum(1 for _label, _changes, expect in vectors if expect == "red")
    print(f"OK: 守门自测通过（{reds} 条注入全红，{len(vectors) - reds + 1} 条基线/放行保持绿）")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=f"守门：官方发布产物里不许有 {FEATURE}")
    parser.add_argument("--self-test", action="store_true", help="注入变异自测（该红的红、该绿的绿）")
    parser.add_argument("--root", type=Path, default=ROOT, help="仓根（自测用）")
    args = parser.parse_args()

    if args.self_test:
        return self_test()

    problems = scan(args.root)
    if problems:
        for problem in problems:
            print(problem, file=sys.stderr)
        return 1
    print(f"OK: 流水线与打包脚本都没有打开 {FEATURE}（默认关，发布产物不含测试通道）")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
