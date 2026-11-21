#!/usr/bin/env python3
"""发布标记（`gh release ... --prerelease`）的**唯一定义处**（M163）。

# 三种触发方式各创建什么

| 触发 | `github.event.inputs.prerelease` | 新建 Release（create） | 已存在 Release（edit） |
| --- | --- | --- | --- |
| push tag `v*`（以及它的 Re-run） | 空串（该字段不存在） | **稳定版**：不带任何标记 | 不传标记（**不碰**） |
| `workflow_dispatch` + `prerelease=true` | `"true"` | 预发布 | 显式改成预发布 |
| `workflow_dispatch` + `prerelease=false` | `"false"` | 稳定版 | 显式改成稳定版 |

空串必须是稳定版：GitHub 的 `releases/latest` **只指向非 prerelease 的最新 Release**，而自更新
客户端把清单地址钉在 `releases/latest/download/manifest.json`（D50）。v0.2.2 之前 push tag 默认打
`--prerelease`，于是 latest 一直 404、更新通道取不到清单——M163 修的就是这条语义。

edit 那一列的空串是「不碰」而不是「改成稳定版」：同 tag 重跑会落到 edit 分支，而用户手工把某个
Release 提成稳定版之后，重跑**不应该**把它翻回预发布。

# 流水线里唯一的走法

    python3 packaging/release_marker.py --emit-bash --prerelease "${PRERELEASE:-}" > "$marks"
    source "$marks"          # 得到 create_mark=(...) / edit_mark=(...)

先落文件再 `source`，而不是 `source <(...)`：非法的 `prerelease` 取值必须让这一步**当场红**
（`set -e` 只认前一条命令的退出码，进程替换里的失败会被吞掉）。

守门在 `packaging/check_release_semantics.py`：它调本文件的 `marks()` 逐条对表，再真的把
`--emit-bash` 灌进 bash 看展开出来的两个数组。本文件的 `--self-test` 只查自己这一层。
"""

from __future__ import annotations

import argparse
import sys

#: 打预发布标记。
PRERELEASE = "--prerelease"
#: 显式摘掉预发布标记（`gh release edit` 用；`create` 不带标记就已经是稳定版）。
STABLE = "--prerelease=false"

#: （触发方式，输入原始值，create 标记，edit 标记）——本仓对外承诺的那张表，逐行可对。
CASES: tuple[tuple[str, str, tuple[str, ...], tuple[str, ...]], ...] = (
    ("push tag v*（含其 Re-run）", "", (), ()),
    ("workflow_dispatch + prerelease=true", "true", (PRERELEASE,), (PRERELEASE,)),
    ("workflow_dispatch + prerelease=false", "false", (), (STABLE,)),
)


def marks(raw: str) -> tuple[tuple[str, ...], tuple[str, ...]]:
    """`github.event.inputs.prerelease` 的原始字符串 → (create 标记, edit 标记)。

    只认三种取值（空串 / `"true"` / `"false"`），其它一律抛 :class:`ValueError`——发布流水线
    宁可红在一次错误的 dispatch 输入上，也不要把一个自己没预期的值当成「稳定版」发出去。
    """
    value = (raw or "").strip()
    if value == "":
        return (), ()
    if value == "true":
        return (PRERELEASE,), (PRERELEASE,)
    if value == "false":
        return (), (STABLE,)
    raise ValueError(
        f"非预期的 github.event.inputs.prerelease 取值：{raw!r}（只认 空串 / true / false）"
    )


def bash_snippet(raw: str) -> str:
    """把两个数组渲染成可直接 `source` 的 bash 片段。"""
    create, edit = marks(raw)
    return (
        "# 由 packaging/release_marker.py 生成，别手改：标记的全部判据都在那个脚本里。\n"
        f"create_mark=({' '.join(create)})\n"
        f"edit_mark=({' '.join(edit)})\n"
    )


def table() -> str:
    """把 :data:`CASES` 渲染成 markdown 表（`--table`；报告与文档直接抄它）。"""

    def cell(flags: tuple[str, ...], empty: str) -> str:
        return "、".join(f"`{flag}`" for flag in flags) if flags else empty

    rows = ["| 触发 | `prerelease` 输入 | create | edit |", "| --- | --- | --- | --- |"]
    for trigger, raw, create, edit in CASES:
        shown = "空串" if raw == "" else f'`"{raw}"`'
        rows.append(f"| {trigger} | {shown} | {cell(create, '（不带标记）')} | {cell(edit, '不碰标记')} |")
    return "\n".join(rows)


def self_test() -> int:
    """本文件自己这一层的断言（完整守门见 check_release_semantics.py）。"""
    failures: list[str] = []

    for trigger, raw, create, edit in CASES:
        got = marks(raw)
        if got != (create, edit):
            failures.append(f"{trigger}：表里写 {(create, edit)}，marks({raw!r}) 给的是 {got}")

    # 最关键的一条：push tag（空串）的新建路径**不许**带预发布标记。它是 v0.2.2 至今
    # `releases/latest` 404 的根因，任何一次重构都不许把它带回来。
    push_create, push_edit = marks("")
    if PRERELEASE in push_create:
        failures.append(f"push tag 的 create 标记里出现了 {PRERELEASE}：稳定通道会一直是 404")
    if push_edit:
        failures.append(f"push tag 的 edit 标记应为「不碰」，实际是 {push_edit}")

    # dispatch 显式 false 时，新建同样是稳定版（create 不需要 `--prerelease=false`：不带就已经是了）。
    false_create, false_edit = marks("false")
    if PRERELEASE in false_create:
        failures.append(f"dispatch prerelease=false 的 create 标记里出现了 {PRERELEASE}")

    # 非法值必须炸，不许静默当稳定版（`True` 是 Python 的写法，Actions 的 boolean 输入给的是小写）。
    for bad in ("yes", "1", "TRUE", "False", "0", "prerelease"):
        try:
            marks(bad)
        except ValueError:
            continue
        failures.append(f"非法取值 {bad!r} 被当成了合法输入：{marks(bad)}")

    if failures:
        for item in failures:
            print(f"::error::release_marker 自检失败：{item}", file=sys.stderr)
        return 1
    print("OK: release_marker 标记表自检通过（push tag → 稳定版）")
    print(table())
    return 0


def main(argv: list[str]) -> int:
    # Windows 控制台是 GBK：本文件的输出里有中文与箭头，缺了这行会在打印时抛
    # UnicodeEncodeError（与判据无关的崩溃）。CI 是 UTF-8，不受影响。
    for stream in (sys.stdout, sys.stderr):
        if hasattr(stream, "reconfigure"):
            stream.reconfigure(errors="backslashreplace")

    parser = argparse.ArgumentParser(
        description="发布标记（--prerelease）的唯一定义处：打印 gh release 要带的参数。"
    )
    parser.add_argument(
        "--prerelease",
        default="",
        metavar="VALUE",
        help='github.event.inputs.prerelease 的原值：空串 / "true" / "false"',
    )
    group = parser.add_mutually_exclusive_group()
    group.add_argument(
        "--emit-bash",
        action="store_true",
        help="打印可直接 source 的 bash 片段（create_mark / edit_mark 两个数组）",
    )
    group.add_argument("--table", action="store_true", help="打印触发方式 ↔ 标记的对照表")
    group.add_argument("--self-test", action="store_true", help="只查本文件这一层的断言")
    args = parser.parse_args(argv)

    if args.self_test:
        return self_test()
    if args.table:
        print(table())
        return 0

    try:
        sys.stdout.write(bash_snippet(args.prerelease))
    except ValueError as error:
        # 走 stderr：stdout 在这种情况下会被重定向进 marks 文件，注解会丢。
        print(f"::error::{error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
