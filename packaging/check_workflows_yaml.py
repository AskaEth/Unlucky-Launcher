#!/usr/bin/env python3
"""Parse every workflow file with a **real YAML parser** (M190 r1 复审 P1-1).

Why this exists: a raw 0x01 byte slipped into `.github/workflows/release-build.yml` (a `sed` replacement
string that should have been `\\1`). GitHub refuses to parse such a file, so the whole run would not even
start — and no other guard noticed, because they all read workflow files as *text* (`check_release_semantics.py`
does `yaml_scalars()`, the file-size job only counts lines). A text scanner cannot see a control byte that
a YAML 1.2 reader rejects outright.

So: hand every `.github/workflows/*.yml` to PyYAML. Two checks, both cheap:
  ① it parses (real error message with position, not a mystery red);
  ② no C0 control character other than TAB/LF/CR is present (YAML 1.2 forbids them; parsers differ in
     whether they reject, so this is the belt-and-braces half).

PyYAML is preinstalled on GitHub-hosted runners; if it is not, this script installs it. If that fails too
the gate **fails** rather than degrading to a text scan ("工具缺失也 fail" — same rule as the other guards).

    python3 packaging/check_workflows_yaml.py
    python3 packaging/check_workflows_yaml.py --self-test
"""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path

WORKFLOW_DIR = Path(__file__).resolve().parent.parent / ".github/workflows"
ALLOWED_CONTROL = {0x09, 0x0A, 0x0D}


def load_yaml():
    """Import PyYAML, installing it once if the runner lacks it."""
    try:
        import yaml  # noqa: PLC0415

        return yaml
    except ImportError:
        print("PyYAML 缺失，尝试安装一次（pip install --quiet pyyaml）…", file=sys.stderr)
        done = subprocess.run(
            [sys.executable, "-m", "pip", "install", "--quiet", "pyyaml"],
            capture_output=True,
            text=True,
            check=False,
        )
        if done.returncode != 0:
            print(
                "error: 装不上 PyYAML，YAML 门禁无法运行（门禁不降级）。输出：\n" + done.stdout + done.stderr,
                file=sys.stderr,
            )
            raise SystemExit(2) from None
        import yaml  # noqa: PLC0415

        return yaml


def control_characters(data: bytes) -> list[tuple[int, int]]:
    """`[(位置, 字节)]`：YAML 1.2 不许出现的 C0 控制字符（TAB/LF/CR 除外）。"""
    return [(index, byte) for index, byte in enumerate(data) if byte < 0x20 and byte not in ALLOWED_CONTROL]


def check_file(path: Path, yaml) -> list[str]:
    """返回该文件没过的判据（空 = 通过）。"""
    failures: list[str] = []
    data = path.read_bytes()
    for index, byte in control_characters(data):
        line = data[:index].count(b"\n") + 1
        failures.append(
            f"::error file={path},line={line}::第 {line} 行含裸控制字符 {byte:#04x}（YAML 1.2 禁止；"
            "常见成因是 sed/printf 把 `\\1` 写成了 0x01）"
        )
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError as error:
        failures.append(f"::error file={path}::不是合法 UTF-8（{error}）")
        return failures
    try:
        yaml.safe_load(text)
    except Exception as error:  # noqa: BLE001 - 解析器的异常类型随版本变，这里只关心「没通过」
        first = str(error).splitlines()[0] if str(error) else repr(error)
        failures.append(f"::error file={path}::YAML 解析失败：{first}")
    return failures


def self_test() -> int:
    """注入一枚 0x01 到临时副本，断言门禁**会红**（否则这条门禁对任何输入都绿）。"""
    import tempfile

    yaml = load_yaml()
    failures = 0
    sample = "name: x\non: push\njobs:\n  a:\n    runs-on: ubuntu-latest\n    steps:\n      - run: |\n          echo hi\n"
    with tempfile.TemporaryDirectory() as tmp:
        good = Path(tmp) / "good.yml"
        good.write_text(sample, encoding="utf-8")
        if check_file(good, yaml):
            print("FAIL: 合法 YAML 被判红（误杀）", file=sys.stderr)
            failures += 1
        bad = Path(tmp) / "bad.yml"
        bad.write_bytes(sample.replace('"/\\1/p"', "").encode("utf-8") + b'x\n          v=$(s/"' + bytes([1]) + b'/p)\n')
        if not check_file(bad, yaml):
            print("FAIL: 含裸 0x01 的 YAML 竟然通过（判据失效）", file=sys.stderr)
            failures += 1
        # 只让 PyYAML 判的那一半也要能红：把控制字节换成合法字符，但结构坏掉。
        broken = Path(tmp) / "broken.yml"
        broken.write_text("a: [1, 2\nb: 3\n", encoding="utf-8")
        if not check_file(broken, yaml):
            print("FAIL: 结构坏的 YAML 竟然通过", file=sys.stderr)
            failures += 1
    print("self-test:", "ok" if failures == 0 else f"{failures} 处失败")
    return 0 if failures == 0 else 1


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="用真 YAML 解析器 parse 全部 workflow（M190 r1 P1-1）")
    parser.add_argument("--self-test", action="store_true", help="自检：注入 0x01 / 坏结构必须红")
    parser.add_argument("--dir", type=Path, default=WORKFLOW_DIR, help="workflow 目录（默认 .github/workflows）")
    args = parser.parse_args(argv)

    if args.self_test:
        return self_test()

    yaml = load_yaml()
    files = sorted(args.dir.glob("*.yml")) + sorted(args.dir.glob("*.yaml"))
    if not files:
        print(f"error: {args.dir} 里一个 workflow 都没有（路径错了？门禁不降级）", file=sys.stderr)
        return 2

    failures: list[str] = []
    for path in files:
        failures.extend(check_file(path, yaml))

    for line in failures:
        print(line, file=sys.stderr)
    if failures:
        print(f"FAIL：{len(failures)} 条判据没过（{len(files)} 个文件）", file=sys.stderr)
        return 1
    print(f"ok: {len(files)} 个 workflow 全部能被真 YAML 解析器 parse，且无裸控制字符")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
