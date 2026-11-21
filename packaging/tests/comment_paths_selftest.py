#!/usr/bin/env python3
"""`packaging/check_comment_paths.py --self-test` 的执行器（M402）：合成小仓向量 + 真仓回归。

分工同 `gpu_shot_selftest.py`：判据与脚手架分家，本文件**不反向 import** 判据脚本（`scan_text` / `Tree` /
`git_ls` 由调用方以参数传入）。向量数据在 `comment_paths_vectors.json`。

每条向量 = 一份**合成小仓**（`files`）+ 期望命中的判死**种类**（`expect`，多一条少一条都算失败）。带
`repo` 的向量跑**真仓**（不合成），钉的是那两处真实样本：

- `crates/presenter/src/host/settings/ai.rs` 的 `pages/settings/ai/toggles.rs::yolo_row` —— X9 的修法，
  必须绿。这是**防假红**的关键一条：守门把对的判红，就等于逼下一个人去写假绿。
- `crates/app-iced/src/app/update/settings/tests/yolo_toggle.rs` 的 `ai::view::yolo_row` —— X11 的原形是
  形态①（`a::b::c`），本守门**故意不管**。这条向量是钉子：哪天有人顺手把 ①②③ 也纳进守门，它会红，
  逼那次改动先回答「`#[path]` 子树、内联 mod 嵌套链、`Type::member` 消歧」怎么做到零假红。
- `crates/app-iced/src/pages/logs.rs` —— X9 那一族**修净之后必须绿**：M404 清了基线，这一行现在写的是
  `app/view/logs.rs::logs_model`（定义就在那里）。真仓里已经没有 X9 样本，所以这条从「真仓抓得到」
  转成**回归钉子**：再写回使用点，它会红。

一颗诚实的钉子：合成向量守的是**判据**，不守「注释的书写风格」。产品换了写法（比如改用 `Type::member`）
它不会红 —— 那是守门**故意**不管的形态，见判据脚本 docstring 的「故意不做」。
"""

from __future__ import annotations

import json
import sys
import tempfile
from pathlib import Path


def safe_streams() -> None:
    """把 stdout/stderr 钉成「编不出来的字符也不许抛 `UnicodeEncodeError`」。

    M403 的起因：向量标签里有 `⇒`（U+21D2），它不在 cp936 里 —— GBK 控制台上跑到第一条带它的向量就
    traceback，而 CI（ubuntu / UTF-8）全绿。口径与 `check_pe_version.py`、`check_branding_assets.py` 的
    `safe_streams` 一致：真控制台**只放宽 `errors`**（不改编码，否则汉字全变乱码，等于没修好）；管道 /
    重定向里钉 UTF-8（CI 与日志都按 UTF-8 读）。
    """
    for stream in (sys.stdout, sys.stderr):
        reconfigure = getattr(stream, "reconfigure", None)  # Python < 3.7 没有，那就只能用默认行为
        if reconfigure is None:
            continue
        try:
            if stream.isatty():
                reconfigure(errors="backslashreplace")
            else:
                reconfigure(encoding="utf-8", errors="backslashreplace")
        except (ValueError, OSError):
            pass


def _kinds(root: Path, files: dict[str, str], scan_text, Tree) -> list[str]:
    """把 `files` 落盘成一个小仓，扫**每一个**文件里的引用，返回命中的判死种类（已排序）。"""
    for relative, text in files.items():
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
    tree = Tree(root, sorted(files))
    return sorted(finding.kind for _relative, text in files.items()
                  for finding in scan_text(text, _relative, tree))


def run_selftest(scan_text, Tree, git_ls, vectors_path: Path, root: Path) -> int:
    """跑 `comment_paths_vectors.json` 的全部向量，逐条对 `expect`。"""
    safe_streams()
    try:
        cases = json.loads(vectors_path.read_text(encoding="utf-8"))["cases"]
    except (OSError, ValueError, KeyError) as error:
        print(f"::error::读不到自测向量 {vectors_path}：{error}", file=sys.stderr)
        return 1
    failures = 0
    real_tree = None
    for case in cases:
        if "repo" in case:  # 真仓回归：不合成，直接扫那一个真文件
            if real_tree is None:
                real_tree = Tree(root, git_ls(root))
            text = (root / case["repo"]).read_text(encoding="utf-8", errors="replace")
            got = sorted(finding.kind for finding in scan_text(text, case["repo"], real_tree))
        else:
            with tempfile.TemporaryDirectory(prefix="ull-comment-paths-") as temp:
                got = _kinds(Path(temp), case["files"], scan_text, Tree)
        want = sorted(case["expect"])
        if got != want:
            print(f"::error::自测失败：{case['label']} 期望 {want}，实际 {got}", file=sys.stderr)
            failures += 1
        else:
            print(f"OK: {case['label']} → {('命中 ' + ' / '.join(got)) if got else '绿'}")
    if failures:
        return 1
    red = sum(1 for case in cases if case["expect"])
    print(f"OK: 注释路径守门自测通过（{len(cases)} 条向量：判死 {red} / 判解或判不了 {len(cases) - red}）")
    return 0
