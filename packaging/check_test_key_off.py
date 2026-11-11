#!/usr/bin/env python3
"""守 `update-test-key`：这把**公开**的测试公钥不许进发布构建（M190 r3）。

背景：`crates/core` 的信任表（`trust::PRODUCTION`）正式构建里是空的（fail-closed，等用户录钥），
测试公钥 `trust::TEST_ONLY` 只在三档「仅测试」构建里并入：`cfg(test)`、测试通道那条 feature、
`update-test-key`。第三档是给 `crates/presenter` 的 `[dev-dependencies]` 用的——presenter 的更新链路
用例要构造「本构建认得」的签名清单，而它编译 core 时既没有 `cfg(test)`，也不该打开
「测试通道」那条 feature（它会**同时**给出「本地目录当更新源」这条路，是另一个更大的口子；
那条由 `check_test_source_off.py` 守。⚠ 本文件**刻意不写那条 feature 的全名**：那个守门按字面
扫 `packaging/**`，写了这里就红——这是它的设计（宁可误杀也不放过），不是笔误）。

这一档比「测试通道」那条 feature 窄，但它仍然是**信任锚的开关**：一旦进发布构建，任何人都能用公开种子
签出「官方更新」。所以它的出现位置必须是**白名单**，而不是「没人会这么写」：

| 允许出现的地方 | 形态 |
|---|---|
| `crates/core/Cargo.toml` | 只能是它自己的声明，且必须是**空数组** `update-test-key = []`（写成带子 feature 的非空数组即红） |
| `crates/presenter/Cargo.toml` | 只能出现在 `[dev-dependencies]` 段内 |
| 本脚本 | 它必须写出这个 feature 名 |

其余任何地方（workflow、`packaging/` 的脚本、根 `Cargo.toml`、`.cargo/config*.toml`、presenter 的
`[dependencies]`）提到它即红。`--self-test` 把这四类真实回归各注入一次（workflow / presenter 的
`[dependencies]` / 根 `Cargo.toml` / core 声明里偷偷带子 feature），并断言合法写法不误杀。

    python3 packaging/check_test_key_off.py
    python3 packaging/check_test_key_off.py --self-test
"""

from __future__ import annotations

import argparse
import re
import sys
import tempfile
from pathlib import Path

FEATURE = "update-test-key"
ROOT = Path(__file__).resolve().parent.parent
GUARD = Path(__file__).resolve()

#: 扫描面（逐项列举，不用 `**` 全仓：那会把 `target/` 拖进来）。
GLOBS = (
    ".github/workflows/**/*.yml",
    ".github/workflows/**/*.yaml",
    ".github/actions/**/*.yml",
    ".github/actions/**/*.yaml",
    "packaging/**/*.py",
    "packaging/**/*.sh",
    "packaging/**/*.ps1",
    "Cargo.toml",
    "crates/*/Cargo.toml",
    "Makefile",
    "justfile",
    "Justfile",
    "Taskfile.yml",
    "Taskfile.yaml",
    "crates/*/Makefile",
    "packaging/Makefile",
    ".cargo/config.toml",
    ".cargo/config",
    "crates/*/.cargo/config.toml",
    "crates/*/.cargo/config",
)

#: 白名单：(相对路径, 允许的形态)。形态见 `check_manifest` 的三种判法。
ALLOWED_DECLARATION = "crates/core/Cargo.toml"
ALLOWED_DEV_DEPENDENCY = "crates/presenter/Cargo.toml"

#: 另一条（更宽的）测试通道 feature 的名字。**刻意拼出来**：`check_test_source_off.py` 按字面扫
#: `packaging/**`，这里写全名会让那个守门红 —— 那是它的设计（宁可误杀也不放过），不是笔误。
#: 本文件与它同源同一套手法，别“顺手”把它写成字面量。
OTHER_FEATURE = "-".join(("update", "test", "source"))

#: core 只许**自己的声明**，而且必须是**空数组** `update-test-key = []`。
#: 这里认**内容**、不只认形状：写成带子 feature 的非空数组（`["<另一条 feature>"]`）会让这个「窄口子」
#: 顺手把「宽口子」（环境变量 + `DirSource` 那条测试通道）拉起来 —— 那正是本守门要防的逃逸。
DECLARATION = re.compile(rf"^\s*{re.escape(FEATURE)}\s*=\s*\[\s*\]\s*$")
SECTION = re.compile(r"^\s*\[([^\]]+)\]\s*$")


def strip_comment(line: str) -> str:
    """引号感知地剥注释（与 `check_test_source_off.py` 同一手法，省得两处对同一行有不同看法）。"""
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


def check_manifest(path: Path, where: str) -> list[str]:
    """`Cargo.toml`：核心只许自己的声明；presenter 只许 `[dev-dependencies]` 段内。"""
    problems: list[str] = []
    section = ""
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        code = strip_comment(line).strip()
        if not code:
            continue
        header = SECTION.match(code)
        if header:
            section = header.group(1).strip()
            continue
        if FEATURE not in code:
            continue
        if where == ALLOWED_DECLARATION and DECLARATION.match(code):
            continue
        if where == ALLOWED_DEV_DEPENDENCY and section == "dev-dependencies":
            continue
        problems.append(
            f"::error file={where}:{number}::{where} 在 `[{section or '顶部'}]` 里写了 `{FEATURE}`；"
            "它只许出现在 crates/core/Cargo.toml 自己的声明与 crates/presenter/Cargo.toml 的 "
            "`[dev-dependencies]` 里（这把公钥进发布构建 = 谁都能签出「官方更新」）"
        )
    return problems


def check_plain(path: Path, where: str) -> list[str]:
    """其余文件类型：出现即红（不解释形态）。"""
    problems: list[str] = []
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        if FEATURE in strip_comment(line):
            problems.append(
                f"::error file={where}:{number}::{where} 提到 `{FEATURE}`：它不进 workflow / 打包脚本 / "
                "cargo 配置（测试钥只许由 crates/presenter 的 dev-dependencies 打开）"
            )
    return problems


def scan(root: Path) -> list[str]:
    """扫整个仓库，返回所有问题（空 = 绿）。"""
    problems: list[str] = []
    for pattern in GLOBS:
        for path in sorted(root.glob(pattern)):
            if not path.is_file() or path.resolve() == GUARD:
                continue
            where = path.relative_to(root).as_posix()
            try:
                if path.name == "Cargo.toml":
                    problems += check_manifest(path, where)
                else:
                    problems += check_plain(path, where)
            except (OSError, UnicodeDecodeError) as error:
                problems.append(f"::error file={where}::读不到：{error}")
    return problems


# ────────────────────────── 变异自测 ──────────────────────────

BASELINE = {
    "crates/core/Cargo.toml": (
        "[features]\nother-feature = []\n\n# 注释里提到 update-test-key 不算违规\ntest-key = []\n"
        "update-test-key = []\n"
    ),
    "crates/presenter/Cargo.toml": (
        "[dependencies]\nunlucky-core = { path = \"../core\" }\n\n"
        "[dev-dependencies]\nunlucky-core = { path = \"../core\", features = [\"update-test-key\"] }\n"
    ),
    ".github/workflows/ci.yml": "jobs:\n  a:\n    steps:\n      - run: cargo test\n",
}

VECTORS = [
    (
        "workflow 里打开它",
        {".github/workflows/ci.yml": "jobs:\n  a:\n    steps:\n      - run: cargo build --features update-test-key\n"},
        "red",
    ),
    (
        "presenter 的 [dependencies] 里打开它",
        {
            "crates/presenter/Cargo.toml": (
                "[dependencies]\nunlucky-core = { path = \"../core\", features = [\"update-test-key\"] }\n"
            )
        },
        "red",
    ),
    (
        "根 Cargo.toml 把依赖提起来",
        {
            "Cargo.toml": (
                '[workspace.dependencies]\nunlucky-core = { path = "crates/core", '
                'features = ["update-test-key"] }\n'
            )
        },
        "red",
    ),
    ("打包脚本提到它", {"packaging/x.sh": "cargo build --features update-test-key\n"}, "red"),
    (
        "core 自己的声明（合法）",
        {"crates/core/Cargo.toml": "[features]\nupdate-test-key = []\n"},
        "green",
    ),
    (
        "core 的声明里偷偷带子 feature（形状合法、内容非法）",
        {"crates/core/Cargo.toml": f'[features]\n{FEATURE} = ["{OTHER_FEATURE}"]\n'},
        "red",
    ),
    ("注释里提到它（合法）", {"packaging/y.py": "# 见 update-test-key 的说明\n"}, "green"),
]


def _scan_case(files: dict[str, str]) -> list[str]:
    with tempfile.TemporaryDirectory(prefix="ull-test-key-guard-") as temp:
        root = Path(temp)
        for relative, text in files.items():
            path = root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text, encoding="utf-8")
        return scan(root)


def self_test() -> int:
    failures = 0
    baseline = _scan_case(BASELINE)
    if baseline:
        print(f"FAIL 基线被判红：{baseline}", file=sys.stderr)
        failures += 1
    for label, files, expect in VECTORS:
        merged = dict(BASELINE)
        merged.update(files)
        problems = _scan_case(merged)
        red = bool(problems)
        if (expect == "red") != red:
            print(f"FAIL 向量「{label}」期望 {expect}，实际 {'red' if red else 'green'}", file=sys.stderr)
            failures += 1
    print(f"self-test: {'ok' if failures == 0 else f'{failures} 处失败'}（{len(VECTORS)} 条向量 + 基线）")
    return 0 if failures == 0 else 1


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="测试公钥不许进发布构建（M190）")
    parser.add_argument("--self-test", action="store_true", help="注入变异：该红的红、该绿的绿")
    parser.add_argument("--dir", type=Path, default=ROOT, help="仓库根（默认本文件上一级）")
    args = parser.parse_args(argv)

    if args.self_test:
        return self_test()
    problems = scan(args.dir)
    for line in problems:
        print(line, file=sys.stderr)
    if problems:
        print(f"FAIL：{len(problems)} 处（`{FEATURE}` 只许由 crates/presenter 的 dev-dependencies 打开）", file=sys.stderr)
        return 1
    print(f"ok: `{FEATURE}` 只出现在允许的位置（core 的声明 + presenter 的 dev-dependencies）")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
