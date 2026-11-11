#!/usr/bin/env python3
"""GitHub `if:` 表达式的小求值器（M253 / r2 复审返工，判据 3 用）。

为什么需要它：字符串存在性判据被两轮复审证伪过 —— `if: needs.probe.outputs.available == 'true' || always()`
用 `in` 判照样绿，`if: true` 也绿。所以这里**真求值**：给 `needs.probe.outputs.available` 分别取
`'true'` / `'false'` / `''`，门必须分别判真 / 假 / 假 —— 门到底还挂不挂，由语义决定，不由子串决定。

**只认一小撮语法，其余一律抛 `ConditionError`（调用方按红处理，宁可假红）**：
`==` / `!=` / `&&` / `||` / 括号 / 单引号字符串 / 数字 / `needs.<job>.outputs.<名>` / `true|false|null` /
`always()|success()|failure()|cancelled()`（后四个按「假定上游成功」取常量）。**不支持 `!`**：GitHub 与
Python 的 `!` 优先级不同，认不出比误判安全。不是 GitHub 语义的替身，是它的一个保守子集。
"""

from __future__ import annotations

import re

AVAILABLE_IDENT = "needs.probe.outputs.available"
CALL_FUNCS = {"always": True, "success": True, "failure": False, "cancelled": False}
_TOKEN_RE = re.compile(r"(?P<string>'[^']*')|(?P<number>\d+)|(?P<ident>[A-Za-z_][\w.\-]*)|(?P<op>==|!=|&&|\|\||[()!])")


class ConditionError(Exception):
    """`if:` 表达式认不出 —— 认不出即判红，绝不默认放过。"""


def evaluate_condition(raw, available: str) -> bool:
    """把 GitHub 的 `if:` 表达式求值。只认一小撮语法，其余抛 `ConditionError`（= 判红）。"""
    text = str(raw).strip()
    if text.startswith("${{") and text.endswith("}}"):
        text = text[3:-2].strip()
    for name, value in CALL_FUNCS.items():
        text = re.sub(rf"\b{name}\s*\(\s*\)", "true" if value else "false", text)
    if re.search(r"\b(" + "|".join(CALL_FUNCS) + r")\s*\(", text):
        raise ConditionError("函数只支持不带参数的形式")
    parts: list[str] = []
    pos = 0
    while pos < len(text):
        if text[pos].isspace():
            pos += 1
            continue
        match = _TOKEN_RE.match(text, pos)
        if not match:
            raise ConditionError(f"认不出的字符 {text[pos]!r}")
        pos = match.end()
        token = match.group()
        if match.lastgroup in ("string", "number"):
            parts.append(token)
        elif match.lastgroup == "ident":
            if token == AVAILABLE_IDENT:
                parts.append("__available")
            elif token in ("true", "false", "null"):
                parts.append({"true": "True", "false": "False", "null": "None"}[token])
            else:
                raise ConditionError(f"认不出的标识符 `{token}`")
        else:
            if token == "!":
                raise ConditionError("不支持 `!`（GitHub 与 Python 的优先级不同，宁可认不出）")
            parts.append({"&&": "and", "||": "or"}.get(token, token))
    source = " ".join(parts)
    try:
        return bool(eval(source, {"__builtins__": {}}, {"__available": available}))  # noqa: S307 - 词表是白名单，见上
    except Exception as error:  # noqa: BLE001 - 任何求值失败都按「认不出」处理
        raise ConditionError(f"求值失败（{error}）：`{source}`") from None
