#!/usr/bin/env python3
"""守注释里的 `文件.rs::符号`（M402）：只判「文件存在 + 符号在那个文件里定义过」。

为什么只守这一种形态：M401 survey（`DevDocs/artifacts/deadpaths/REPORT.md` §5.3）把注释里的路径引用
分成四种形态，只有**形态④ `x.rs::symbol`** 能机械判定且几乎零假红。判据：① `X.rs` 必须存在（注释常
省略中间层，如 `presenter/host/addon/pack/import.rs` 少了 `src/`，所以按**段序**匹配，不是只比后缀）；
② `symbol` 必须在**这个文件**的定义索引里：`fn`/`struct`/`enum`/`trait`/`union`/`type`/`const`/`static`/
`mod`/`macro_rules!`，含内联 `mod` 里的项、`impl` 块内的方法与关联常量、结构体字段、枚举变体；或在这个
文件里被 **`pub use` 再导出**（那条路径真的可达）；③ `xx_*` 这种通配引用按**前缀**找；「把 X 搬进…」
那种假设句登记在 `BASELINE` 里（逐条带理由）。

**故意不做**——**失效方式不一样，别混读**：① `a::b::c` 全路径与 ② `Type::member` 直接不判（前者要 `#[path]`
子树重挂 + 内联 `mod` 嵌套链，否则 `crate::app::components::*` 那一族对的注释会被判红；后者要先区分同名类型
——按 `struct`/`enum` 定义数，本树 `State` 9 个、`Theme`/`Settings` 各 1 个（M401 报告里写的 3/2/2 与树对不上））；
③ 外部 crate（iced / winit / rig / `std`）与 ⑥ 裸文件名（`gone.rs::x`）**只 `--all` 列、不红**；④ 宏生成的名字
（`macro_rules!` 体、`derive`、`include!`）**会假红**（索引里没有 ⇒ `absent` ⇒ fatal，`--all` 也不列）；⑤ 行号
`x.rs:NN`（含「行号还在、内容已换」）**根本不匹配**（`REF` 要 `::`）。**别拿它当全覆盖**：只守形态④ 一个形状。

    python3 packaging/check_comment_paths.py   # crates/ 全仓（CI 用这条）；[--all | --file <path>] 见 --help
    python3 packaging/check_comment_paths.py --self-test
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from collections import Counter
from pathlib import Path
from typing import NamedTuple

ROOT = Path(__file__).resolve().parent.parent
#: 自测数据与执行器（判据函数以参数传入，执行器不反向 import 本文件）。
VECTORS_FILE = Path(__file__).resolve().parent / "tests" / "comment_paths_vectors.json"

#: 形态④：`path/to/x.rs::symbol`，允许文件名带空格。前导断言让 `toggles.rs::yolo_row` 不被当成 `a::b::c`
#: 链的一段（M401 §1.1 踩过）。字符类**必须 ASCII**：`\w` 是 Unicode 语义，`见app/x.rs::y` 会被整条吞进
#: group(1) ⇒ 判成「外部路径」静默放行（M402 r1 P2-1）；符号组同理，否则中文会被吃进符号名、把对的判死。
REF = re.compile(r"(?<![A-Za-z0-9_./-])([A-Za-z0-9_./-][A-Za-z0-9_ ./\\-]*?\.rs)::([A-Za-z_][A-Za-z0-9_]*)")
#: 缩略写法（`presenter/.../pack/import.rs`）与模板占位——不是路径断言，不判。
ABBREVIATED = re.compile(r"\.\.\.|…|[<>$]")
#: 定义。**不区分嵌套层级**：内联 `mod` 与 `impl` 块里的项都进同一份文件索引（M401 §5.3 的要求），
#: 顺手消掉「嵌套链只匹配一层」那类假红。`fn` 单列一条：`const fn progress_spec()` 若走
#: `(?:fn|…|const|…)` 那条路会在 `const` 处匹配、把 `fn` 当成名字吃掉，真存在的符号会被判死。
FUNC = re.compile(r"\bfn\s+([A-Za-z_]\w*)")
DEF = re.compile(r"\b(?:struct|enum|trait|union|type|const|static(?:\s+mut)?|mod)\s+([A-Za-z_]\w*)")
MACRO_DEF = re.compile(r"\bmacro_rules\s*!\s*([A-Za-z_]\w*)")
#: `static mut X` 这类限定词会落进 `DEF` 的捕获组：这些不是名字。
QUALIFIERS = {"mut", "unsafe", "extern", "impl", "where", "const", "dyn"}
#: `use` 语句（跨行拼接：本仓大量 `use x::{a, b,\n c};`）。
USE = re.compile(r"(?P<vis>\bpub(?:\s*\([^)]*\))?\s+)?\buse\s+(?P<tree>[^;]+);")
#: 结构体 / 枚举的**花括号体**（字段与变体只在各自的 `{}` 里算；`Foo { a: 1 }` 那种字面量不算）。
BODY = re.compile(r"\b(struct|enum)\s+[A-Za-z_]\w*[^;{]*\{")


def _quoted_end(text: str, index: int) -> int:
    """`index` 处的字符串/字符字面量结束位置；`'a` 这种生命周期返回 `index`（不算字面量）。"""
    char, size = text[index], len(text)
    if char == "r" and (raw := re.match(r'r(#*)"', text[index : index + 64])):  # `r"…"` / `r#"…"#`
        stop = text.find('"' + raw.group(1), index + len(raw.group(0)))
        return size if stop < 0 else stop + 1 + len(raw.group(1))
    if char == "r":
        return index
    if char == '"':
        stop = index + 1
        while stop < size and text[stop] != '"':
            stop += 2 if text[stop] == "\\" else 1
        return min(stop + 1, size)
    if text[index + 1 : index + 2] == "\\":  # `'\n'`
        stop = text.find("'", index + 2)
        return size if stop < 0 else stop + 1
    return index + 3 if text[index + 2 : index + 3] == "'" else index


def mask(text: str) -> tuple[bytearray, str]:
    """`(注释位图, 无注释无字符串的字面代码)`，与 `text` 等长、索引对齐：位图判「这条引用在注释里吗」
    （字符串里的 `http://x/y.rs::z` 不算），代码副本供定义索引与 `use` 解析（注释里的 `fn foo` 不是定义）。
    """
    size, comment, code, index = len(text), bytearray(len(text)), list(text), 0
    while index < size:
        two = text[index : index + 2]
        if two in ("//", "/*"):
            if two == "//":
                stop = text.find("\n", index)
                stop = size if stop < 0 else stop
            else:
                depth, stop = 1, index + 2
                while stop < size and depth:
                    if text.startswith("/*", stop):
                        depth, stop = depth + 1, stop + 2
                    elif text.startswith("*/", stop):
                        depth, stop = depth - 1, stop + 2
                    else:
                        stop += 1
                stop = min(stop, size)
            comment[index:stop] = b"\x01" * (stop - index)
        elif text[index] in "\"r'":
            stop = _quoted_end(text, index)
            if stop == index:
                index += 1
                continue
        else:
            index += 1
            continue
        code[index:stop] = " " * (stop - index)
        index = stop
    return comment, "".join(code)


def split_tree(tree: str) -> list[str]:
    """按**顶层**逗号切开 `use` 树（`{a, b, c::d}` 里的逗号不切）。"""
    parts, depth, current = [], 0, ""
    for char in tree:
        depth += {"{": 1, "(": 1, "}": -1, ")": -1}.get(char, 0)
        if char == "," and depth == 0:
            parts.append(current)
            current = ""
        else:
            current += char
    return [part.strip() for part in parts + [current] if part.strip()]


def use_names(tree: str) -> list[str]:
    """一棵 `use` 树 → 「这些名字在这个文件里可见」。`*` / `_` 枚举不出来，跳过。"""
    names: list[str] = []
    for part in split_tree(tree.strip("{}")):
        prefix, _, inner = part.partition("::{")
        if inner:  # `a::b::{c, d}`：进括号里展开，里面的 `self` 指回上一层模块名
            parent = prefix.split("::")[-1].strip()
            names += [parent if name == "self" else name for name in use_names(inner.rstrip("}"))]
        elif part.startswith("{"):
            names += use_names(part.strip("{}"))
        elif " as " in part:  # `use a::b as c;`
            names.append(part.split(" as ")[1].strip())
        elif not (part.endswith("::*") or part in {"*", "_"}):
            last, parents = part.split("::")[-1].strip(), part.split("::")[:-1]
            if last == "self" and parents and parents[-1] not in {"crate", "self", "super"}:
                names.append(parents[-1])  # `use a::b::{self}` ⇒ 名字 `b` 可见
            elif re.fullmatch(r"[A-Za-z_]\w*", last):
                names.append(last)
    return [name for name in names if name]


def body_spans(code: str, kind: str) -> list[str]:
    """`struct` / `enum` 的花括号**体**原文（花括号按字符深度配对）。"""
    bodies: list[str] = []
    for match in (match for match in BODY.finditer(code) if match.group(1) == kind):
        depth, stop = 1, match.end()
        while stop < len(code) and depth:
            depth += {"{": 1, "}": -1}.get(code[stop], 0)
            stop += 1
        bodies.append(code[match.end() : stop - 1])
    return bodies


class Index:
    """一个 `.rs` 文件的定义索引。"""

    def __init__(self, text: str) -> None:
        _comment, code = mask(text)
        self.defs = {name for name in FUNC.findall(code) + DEF.findall(code) if name not in QUALIFIERS}
        self.defs |= set(MACRO_DEF.findall(code))
        self.fields = {name for body in body_spans(code, "struct") for name in re.findall(r"\b([a-z_]\w*)\s*:", body)}
        self.variants = {name for body in body_spans(code, "enum")
                         for name in re.findall(r"(?:^|[{,])\s*([A-Z]\w*)", body, re.M)}
        self.public, self.private = set(), set()
        for match in USE.finditer(code):
            (self.public if match.group("vis") else self.private).update(use_names(match.group("tree")))

    def verdict(self, symbol: str, prefix: bool = False) -> str:
        """`defined` / `pub_use` / `private_use` / `absent`。`prefix` = 通配引用（`the_x_*`）。"""
        for verdict, names in (("defined", self.defs | self.fields | self.variants), ("pub_use", self.public), ("private_use", self.private)):
            if symbol in names or (prefix and any(name.startswith(symbol) for name in names)):
                return verdict
        return "absent"


def _in_order(pattern: list[str], haystack: list[str]) -> bool:
    """`pattern` 是否按序落在 `haystack` 里（允许中间少几层：`presenter/host` 命中 `crates/presenter/src/host`）。"""
    index = 0
    for segment in haystack:
        if index < len(pattern) and segment == pattern[index]:
            index += 1
    return index == len(pattern)


class Tree:
    """`git ls-files` 的全集 + 按**段序**匹配的路径回退。按需缓存符号索引。"""

    def __init__(self, root: Path, files: list[str]) -> None:
        self.root, self.files = root, set(files)
        self.by_name: dict[str, list[str]] = {}
        #: 全仓出现过的**目录名**（任意层级）：分开「仓内相对路径写错」与「外部源码的一截」。
        self.dir_names = {piece for path in self.files for piece in path.split("/")[:-1]}
        self._index: dict[str, Index] = {}
        for path in self.files:
            self.by_name.setdefault(path.split("/")[-1], []).append(path)

    def candidates(self, path: str) -> list[str]:
        """这个引用可能指的**全体**文件（空 = 一个都对不上）。

        注释常写**省略中间层**的路径（`presenter/host/addon/pack/import.rs` 少了 `src`），所以按段序匹配。
        命中多个时判死口径是「**所有**候选都没有这个符号」，不挑一个猜测再红（M401 §1.2 形态②）。
        """
        segments = path.split("/")
        pool = self.by_name.get(segments[-1], [])
        if not pool or len(segments) == 1:
            return pool
        return [item for item in pool if _in_order(segments[:-1], item.split("/")[:-1])]

    def resolve(self, path: str) -> list[str]:
        """容忍路径前面粘着的英文词（`in my file.rs`）：去掉首词再试，直到对上或没有空格。"""
        while path and not self.candidates(path) and " " in path:
            path = path.split(" ", 1)[1]
        return self.candidates(path)

    def external(self, path: str) -> bool:
        """这个名字像**外部源码**的一截吗（`iced_tiny_skia/src/lib.rs`、winit 的 `window_state.rs`）：
        首段不是仓里出现过的目录名 ⇒ 不判死（仓里没有 iced/winit 源码，M401 §5.3 已把「外部 crate
        判不了」写成结论）。代价：整个删掉的仓内文件只剩裸文件名（`gone.rs::x`）也落进这一类。
        """
        return path.split("/")[0] not in self.dir_names

    def index(self, path: str) -> Index:
        if path not in self._index:
            self._index[path] = Index((self.root / path).read_text(encoding="utf-8", errors="replace"))
        return self._index[path]


class Finding(NamedTuple):
    """一条判死的引用。`key` 给 `BASELINE` 用（行号不进 key：注释挪行不该让基线失效）。"""

    where: str
    line: int
    snippet: str
    path: str
    symbol: str
    reason: str
    detail: str
    kind: str

    @property
    def key(self) -> tuple[str, str, str]:
        return (self.where, self.path, self.symbol)

    def text(self, announce: bool = True) -> str:
        head = f"::error file={self.where},line={self.line}::" if announce else f"{self.where}:{self.line}: "
        return f"{head}{self.snippet} ⇒ {self.reason}：{self.detail}"


def scan_text(text: str, where: str, tree: Tree) -> list[Finding]:
    """一个文件的全部 `文件::符号` 引用 → 判死清单（判解的引用不进清单）。"""
    comment, _code = mask(text)
    findings: list[Finding] = []
    for match in REF.finditer(text):
        if not comment[match.start()]:  # 只在注释里管；代码里不会写出 `x.rs::y` 这种形态
            continue
        path, symbol = match.group(1), match.group(2)
        line = text.count("\n", 0, match.start()) + 1
        snippet = text[text.rfind("\n", 0, match.start()) + 1 :].split("\n")[0].strip()[:160]
        prefix = text[match.end() : match.end() + 1] == "*"  # 通配前缀（`m349.rs::the_x_*`）
        if ABBREVIATED.search(path) or symbol in {"self", "super", "crate"}:  # 没有可判的身份，只 --all 列
            findings.append(Finding(where, line, snippet, path, symbol, "形态判不了",
                                    "路径含 `...` / 占位符，或符号是 `self`/`super`/`crate`", "abbreviated"))
            continue
        candidates = tree.resolve(path)
        if not candidates:
            if tree.external(path):
                findings.append(Finding(where, line, snippet, path, symbol, "外部路径（不判死，仅列举）",
                                        f"`{path}` 首段不是仓内目录名，仓里没有它的源码", "external"))
            else:
                findings.append(Finding(where, line, snippet, path, symbol, "文件不存在",
                                        f"`{path}` 不在 `git ls-files` 里", "missing"))
            continue
        verdicts = {name: tree.index(name).verdict(symbol, prefix) for name in candidates}
        if {"defined", "pub_use"} & set(verdicts.values()):
            continue
        what = f"`{symbol}*` 前缀" if prefix else f"`{symbol}`"
        aliased = [name for name, verdict in verdicts.items() if verdict == "private_use"]
        if aliased:  # X9/X11 那一族：写的是使用点，不是定义点
            reason, detail, kind = "该文件只私有 `use` 过它（写的是使用点）", f"`{aliased[0]}` 里 {what} 不是定义", "alias"
        else:  # 同名文件多个时判死 = **所有**候选都没有它，不挑一个猜测再红
            lead = (f"`{candidates[0]}` 的定义索引里没有" if len(candidates) == 1
                    else f"{len(candidates)} 个同名文件（{', '.join(sorted(candidates)[:2])} …）里都没有")
            reason, detail, kind = "符号不在该文件里", f"{lead} {what}", "absent"
        findings.append(Finding(where, line, snippet, path, symbol, reason, detail, kind))
    return findings


#: `(引用所在文件, 引用里的路径原文, 符号)` → 为什么留。CI 用**全仓**扫描 + 这份基线：**新的三元组**立刻
#: 红，下表每一条都是 M402 `rg` 核过的存量（条数就数行，别在这行写死数字）；同一个三元组在注释里再写几遍
#: 不红（只给一条 `::notice`）。**修掉任一条就该删掉对应行**——不删会在 CI 上得到一条
#: `::warning::基线条目已失效`（不判红：修注释的 mission 通常没有本文件的 scope）。
BASELINE = {}


def git_ls(root: Path, *patterns: str) -> list[str]:
    done = subprocess.run(["git", "ls-files", *patterns], cwd=root, capture_output=True, text=True, check=False)
    if done.returncode != 0:  # 门禁不降级：git 缺失/路径写错要红，不许静默变绿
        raise SystemExit(f"error: git ls-files 失败（{done.stderr.strip()}）——门禁不降级")
    return [line for line in done.stdout.splitlines() if line]


def scan(root: Path, targets: list[str]) -> list[Finding]:
    tree = Tree(root, git_ls(root))
    findings: list[Finding] = []
    for path in targets:
        try:
            findings += scan_text((root / path).read_text(encoding="utf-8", errors="replace"), path, tree)
        except OSError as error:
            findings.append(Finding(path, 1, "", path, "", "读不到文件", str(error), "io"))
    return findings


def report(findings: list[Finding], show_all: bool, label: str, whole_repo: bool) -> int:
    quiet = {"external", "abbreviated"}  # 判不了的两类：只 --all 列，不判红
    fatal = [finding for finding in findings if finding.key not in BASELINE and finding.kind not in quiet]
    for finding in fatal:
        print(finding.text(), file=sys.stderr)
    if show_all:
        for finding in findings:
            if finding.key in BASELINE:
                print(f"allow: {finding.text(False)}  ← {BASELINE[finding.key]}", file=sys.stderr)
            elif finding.kind in quiet:
                print(f"skip:  {finding.text(False)}", file=sys.stderr)
    counts = Counter(finding.key for finding in findings)  # 同一个 key 写几遍：--file 自查也看得见
    for key, count in counts.items():
        if count > 1 and key in BASELINE:
            print(f"::notice::{key[0]} 里 `{key[1]}::{key[2]}` 写了 {count} 遍（BASELINE 只登记一次）", file=sys.stderr)
    if whole_repo:  # 基线条目失效：不判红（修注释的 mission 通常没有本文件的 scope），但要喊一声
        for key in sorted(set(BASELINE) - set(counts)):
            print(f"::warning::BASELINE 里这条已对不上任何引用（修好了？）——请删掉：{key}", file=sys.stderr)
    if fatal:
        print(f"FAIL：{label.strip()} {len(fatal)} 处没过判据（判据见 packaging/check_comment_paths.py）", file=sys.stderr)
        return 1
    print(f"ok: {label}注释路径全部落在存在的文件里且符号在该文件定义/`pub use` 过"
          f"（BASELINE 已登记 {len({finding.key for finding in findings if finding.key in BASELINE})} 条）")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="注释里的 `文件.rs::符号` 必须真存在且真在那个文件里")
    parser.add_argument("--file", type=Path, help="只扫这一个文件（相对仓根或绝对路径）")
    parser.add_argument("--all", action="store_true", help="连判不了的两类（外部路径/缩略写法）与 BASELINE 一起列")
    parser.add_argument("--self-test", action="store_true", help="向量自测")
    args = parser.parse_args(argv)

    if args.self_test:
        return self_test()
    if args.file:
        target = args.file if args.file.is_absolute() else Path.cwd() / args.file
        try:
            relative = target.resolve().relative_to(ROOT.resolve()).as_posix()
        except ValueError:
            print(f"error: {target} 不在仓根 {ROOT} 下", file=sys.stderr)
            return 2
        return report(scan(ROOT, [relative]), args.all, f"`{relative}` 里", whole_repo=False)
    # 扫描面用 `crates` 目录树 + 后缀筛：git pathspec 的 `crates/**/*.rs` **不匹配** `crates/x.rs`
    # （AGENTS.md §8.1 #2 记过同一个坑），直接落在 `crates/` 下的一层 .rs 会被静默漏掉。
    return report(scan(ROOT, [path for path in git_ls(ROOT, "crates") if path.endswith(".rs")]),
                  args.all, "crates/ 下 ", whole_repo=True)


def self_test() -> int:
    """把判据交给 `tests/comment_paths_selftest.py`（同 `gpu_shot_selftest.py` 的分工：脚手架与判据分家）。"""
    sys.path.insert(0, str(Path(__file__).resolve().parent / "tests"))
    try:
        from comment_paths_selftest import run_selftest  # noqa: PLC0415
    except ImportError as error:
        print(f"::error::读不到自测执行器 tests/comment_paths_selftest.py：{error}", file=sys.stderr)
        return 1
    return run_selftest(scan_text, Tree, git_ls, VECTORS_FILE, ROOT)


if __name__ == "__main__":
    raise SystemExit(main())
