#!/usr/bin/env python3
"""发布名单守卫（M164）：**名单声明与实际产物必须对得上**，两个方向都管。

起因：`PUBLISH_SLOTS` 是唯一决定「哪几个槽位进 Release」的开关，而它此前只有一半是守住的 ——
「名单里有、产物没有」会被 `release-publish.yml` 的齐全性检查拦下，但反方向**静默**：某个槽位（例如
`windows-arm64`）真编出了产物、名单却还没声明它时，那些文件会在裁剪步被 `rm` 掉，日志里只有一行
`drop:`，没有任何注解、也没有任何非零退出码。修复的那个 tag 里「arm64 其实已经能出包了」这件事就
这么被吞掉，没人会注意到（M164 的任务清单点名要这条）。

四条断言：

1. **名单非空且可解析**：`release-publish.yml` 里必须有 `PUBLISH_SLOTS: "..."`。开关消失时不要静默
   变成「一个都不发」。
2. **名单里每一项都是矩阵槽位**：`release.yml` 的槽位表（`dict(slot=... )`）才是槽位的唯一定义。
   名单里写错一个字（`windows-aarch64`）此前只会表现为「Release 侧什么也不匹配、齐全性检查红在别处」，
   根因要翻两层日志；这条在 PR 阶段就把它点出来。
3. **名单里每一项的 `mode` 都必须是 `ci`**：`mode: probe` 的槽位默认构建根本不跑，写进发布名单等于
   每一轮都缺一档 —— 一个自己写死要红的组合。
4. **产物 ⇄ 名单两个方向**（`--dist`，只在发布侧有产物时跑）：
   - 名单声明了但没有 GUI 或 CLI 产物 ⇒ `::error::` + exit 1（少一档不发版）。声明为发布的多半就是 `mode: ci`
     的槽位，所以这条同时是「**声明为 ci 的槽位构建红了必须挡住发布**」的判据 —— 红在 build 矩阵里的那一档不会
     让发布跳过（`release` job 是 `always()`），挡住它的就是这里；
   - 有产物但名单没声明 ⇒ `::warning::` 明说「未发布、会被裁掉」，**不许静默**（M184 起 `linux-arm64` 正好是
     这一档：它 `mode: ci`、每轮都编，但运行时未验所以不在名单，产物留着取证）；
   - `mode: ci` 却本次没有产物（且不在名单）⇒ 也打 `::warning::`：那一档的构建很可能红了，不能只表现为「表里少一行」；
   - 无论哪种，最后都打一张全槽位表：发布 / 未发布（无产物）/ 未发布（名单没声明）。

用法（零第三方依赖，本地与 CI 同一份代码）：

    python3 packaging/check_publish_slots.py                       # 源码级断言（PR CI）
    python3 packaging/check_publish_slots.py --dist dist --slots "$PUBLISH_SLOTS"
    python3 packaging/check_publish_slots.py --self-test           # 注入回归，断言每条都变红

`--self-test` 是这条守门的自测证据：把两个 workflow 复制进临时目录、造合成 dist，逐项注入真实回归
（漏一个资产 / 名单拼错 / 名单写了 probe 档 / 名单声明了但产物缺 / 名单没声明却有产物 /
`mode: ci` 却没产物 / 声明的 `linux-x64` 构建红了 / 提前把 `linux-arm64` 写进名单），断言守门
该红的红、该只警告的不红。守门抓不住回归时它自己先红。
"""

from __future__ import annotations

import re
import shutil
import sys
import tempfile
from pathlib import Path

PACKAGING = Path(__file__).resolve().parent
ROOT = PACKAGING.parent

MATRIX_YML = Path(".github/workflows/release.yml")
PUBLISH_YML = Path(".github/workflows/release-publish.yml")

#: 槽位表条目（`plan` job 里那份）：`dict(slot="windows-x64", ..., mode="ci")`，一条可能跨两行。
SLOT_RE = re.compile(r'dict\(\s*slot="(?P<slot>[^"]+)"(?P<rest>.*?)\)', re.S)
MODE_RE = re.compile(r'mode="(?P<mode>[^"]+)"')
#: 发布白名单那一行（yaml 里是 `PUBLISH_SLOTS: "windows-x64 linux-x64"`）。
PUBLISH_RE = re.compile(r'^\s*PUBLISH_SLOTS:\s*"(?P<value>[^"]*)"', re.M)

#: 每个槽位必须**两样都有**：GUI 与 CLI 各自一个资产（只按「槽位名出现过」判会被其中一个掩盖）。
ASSET_PREFIXES = (("unlucky-launcher", "GUI"), ("ull", "CLI"))


def stem(slot: str) -> str:
    """槽位键落到文件名里是下划线形态（`windows-arm64` → `windows_arm64`）。"""
    return slot.replace("-", "_")


# ────────────────────────── 读两个 workflow ──────────────────────────


def matrix_slots(text: str) -> dict[str, str]:
    """`release.yml` 的槽位表 → {槽位: mode}。表在 `plan` job 里，是槽位的唯一定义。"""
    slots: dict[str, str] = {}
    for match in SLOT_RE.finditer(text):
        mode = MODE_RE.search(match.group("rest"))
        slots[match.group("slot")] = mode.group("mode") if mode else ""
    return slots


def declared_slots(text: str) -> list[str] | None:
    """`release-publish.yml` 的 `PUBLISH_SLOTS` → 槽位列表；整行不见了返回 `None`（区别于「空名单」）。"""
    match = PUBLISH_RE.search(text)
    return None if match is None else match.group("value").split()


def parse_slots_arg(raw: str) -> list[str]:
    """`--slots "windows-x64 windows-arm64"` 的取值解析（CI 直接透传 `$PUBLISH_SLOTS`）。"""
    return raw.replace(",", " ").split()


# ────────────────────────── 1~3. 源码级断言 ──────────────────────────


def check_source(root: Path, expected: list[str] | None = None) -> list[str]:
    problems: list[str] = []
    matrix_path = root / MATRIX_YML
    publish_path = root / PUBLISH_YML
    for path in (matrix_path, publish_path):
        if not path.is_file():
            return [f"找不到 {path.relative_to(root)}（流水线搬家了？这条守门要跟着搬）"]

    matrix = matrix_slots(matrix_path.read_text(encoding="utf-8"))
    if not matrix:
        return [f"{MATRIX_YML} 里读不到任何槽位（`dict(slot=...)` 的写法变了？槽位表是槽位的唯一定义）"]

    declared = declared_slots(publish_path.read_text(encoding="utf-8"))
    if declared is None:
        return [f"{PUBLISH_YML} 里没有 `PUBLISH_SLOTS:` 这一行：发布白名单消失了（别让它静默变成「一个都不发」）"]
    if not declared:
        problems.append("`PUBLISH_SLOTS` 是空名单：任何一个槽位都不会进 Release 与 SHA256SUMS")
    if expected is not None and expected != declared:
        # 运行期实际用的名单必须就是文件里那一行（有人在步内覆盖 PUBLISH_SLOTS 时就对不上）。
        problems.append(f"运行期名单 {expected} 与 {PUBLISH_YML} 里声明的 {declared} 不一致")

    for slot in declared:
        if slot not in matrix:
            problems.append(
                f"名单里的 `{slot}` 不是矩阵槽位（矩阵只有 {' / '.join(matrix)}）："
                "写错一个字，裁剪步就一个也匹配不到，而齐全性检查会红在别处、根因埋两层"
            )
        elif matrix[slot] != "ci":
            problems.append(
                f"名单里的 `{slot}` 在矩阵里是 mode={matrix[slot]!r}（默认不构建）："
                "声明为发布却默认不编，等于每一轮都缺一档"
            )
    return problems


# ────────────────────────── 4. 产物 ⇄ 名单（发布侧） ──────────────────────────


def assets_for(dist: Path, slot: str) -> dict[str, list[str]]:
    """{资产前缀: 命中的文件名}——按 `make_artifacts.py::asset_stem()` 的命名规则找。"""
    out: dict[str, list[str]] = {}
    for prefix, _role in ASSET_PREFIXES:
        out[prefix] = sorted(p.name for p in dist.glob(f"{prefix}_*_{stem(slot)}*") if p.is_file())
    return out


def check_dist(dist: Path, declared: list[str], matrix: dict[str, str]) -> tuple[list[str], list[str], list[str]]:
    """返回 (errors, warnings, 表格行)。errors 非空 ⇒ 调用方 exit 1。"""
    errors: list[str] = []
    warnings: list[str] = []
    rows: list[str] = ["槽位                 名单      本次产物        结论"]
    rows.append("-" * 78)
    wanted = set(declared)

    # 顺序：先按矩阵顺序（对外口径稳定），再有名单里多出来的（check_source 已负责报它是错字）。
    order = list(matrix) + [s for s in declared if s not in matrix]
    for slot in order:
        found = assets_for(dist, slot)
        present = [f"{prefix}({len(found[prefix])})" for prefix, _ in ASSET_PREFIXES if found[prefix]]
        is_declared = slot in wanted
        if is_declared:
            missing = [role for prefix, role in ASSET_PREFIXES if not found[prefix]]
            for role in missing:
                suffix = (
                    "（它是 mode=ci 的槽位：构建红了就必须挡住本次发布，别把缺一档的 Release 发出去）"
                    if matrix.get(slot) == "ci"
                    else ""
                )
                errors.append(f"发布白名单缺产物：{slot} 的 {role}（`{prefix_for(role)}_*_{stem(slot)}*`）{suffix}")
            verdict = "发布" if not missing else "缺产物 ⇒ 不发版"
            wanted.discard(slot)
        elif present:
            # **反方向**：真编出来了却没声明。裁剪步会把它 rm 掉，此前这里一句注解都没有。
            warnings.append(
                f"{slot} 有产物（{'、'.join(present)}）但不在 PUBLISH_SLOTS 里：本次不发布，产物会被裁掉。"
                "若这是修复后的首次成功构建，把槽位加进名单即可（齐全性检查与 SHA256SUMS 自动跟着扩）"
            )
            verdict = "未发布（名单没声明）"
        elif matrix.get(slot) == "ci":
            # 声明为「默认在 CI 里构建」的槽位本次却没产物：那一档的构建红了（或矩阵被改窄了）。它不在名单里
            # ⇒ **不该**让发布变红（arm64 的策略就是「红了不挡发布」），但也不能静默：否则「又红了」只剩翻 job 列表。
            warnings.append(
                f"{slot} 是 mode=ci 的槽位（tag 推送默认就编）却本次没有产物：那一档的构建红了或被裁了，"
                "本次未发布。它不在 PUBLISH_SLOTS 里，故不影响本次发布"
            )
            verdict = "未发布（ci 槽位本次无产物）"
        else:
            verdict = "未发布（本次没有产物）"
        rows.append(f"{slot:<20} {'声明' if is_declared else '未声明':<8} {'、'.join(present) or '无':<15} {verdict}")

    for slot in sorted(wanted):
        errors.append(f"发布白名单里的 `{slot}` 既不是矩阵槽位、也没有产物（详见上面的源码级断言）")
    return errors, warnings, rows


def prefix_for(role: str) -> str:
    return next(prefix for prefix, name in ASSET_PREFIXES if name == role)


# ────────────────────────── 汇总 ──────────────────────────


def run_checks(root: Path, dist: Path | None = None, expected: list[str] | None = None) -> tuple[list[str], list[str], list[str]]:
    problems = check_source(root, expected)
    if dist is None or problems:
        return problems, [], []
    matrix = matrix_slots((root / MATRIX_YML).read_text(encoding="utf-8"))
    declared = declared_slots((root / PUBLISH_YML).read_text(encoding="utf-8")) or []
    errors, warnings, rows = check_dist(dist, declared, matrix)
    return problems + errors, warnings, rows


def report(root: Path, dist: Path | None, expected: list[str] | None = None) -> int:
    errors, warnings, rows = run_checks(root, dist, expected)
    # 全部走 **stdout**：Actions 在 stdout 上同样识别 `::error::` / `::warning::`，而分两个流写会让
    # 「表格 → 注解」的顺序在 CI 日志里不可控（stderr 往往整段先刷出来，注解就跑到它解释的那张表前面）。
    for row in rows:
        print(row)
    for item in warnings:
        print(f"::warning::{item}")
    for item in errors:
        print(f"::error::{item}")
    if errors:
        print(f"发布名单守卫：{len(errors)} 条不通过")
        return 1
    print(f"OK: 发布名单守卫全绿（{len(warnings)} 条「未发布」提示）")
    return 0


# ────────────────────────── 自测：注入回归，断言每条都红 ──────────────────────────


def snapshot(dst: Path) -> None:
    for rel in (MATRIX_YML, PUBLISH_YML):
        target = dst / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / rel, target)


def fake_dist(parent: Path, slots: tuple[str, ...]) -> Path:
    """合成 dist：每槽位一个 GUI + 一个 CLI 资产（名字形状与 `make_artifacts.py` 一致）。"""
    dist = parent / "dist"
    dist.mkdir(parents=True, exist_ok=True)
    version = "0.0.0"
    for slot in slots:
        platform, arch = slot.split("-", 1)
        suffix = ".exe" if platform == "windows" else ".tar.gz"
        (dist / f"unlucky-launcher_{version}_{platform}_{arch}{suffix}").write_bytes(b"gui")
        (dist / f"ull_{version}_{platform}_{arch}{suffix}").write_bytes(b"cli")
    return dist


def set_declared(root: Path, value: str) -> None:
    path = root / PUBLISH_YML
    text = path.read_text(encoding="utf-8")
    replaced, count = PUBLISH_RE.subn(f'PUBLISH_SLOTS: "{value}"', text)
    if count != 1:
        raise SystemExit(f"::error::自测自身失效：{PUBLISH_YML} 里找到 {count} 处 PUBLISH_SLOTS")
    path.write_text(replaced, encoding="utf-8")


def self_test() -> int:
    failures = 0
    with tempfile.TemporaryDirectory(prefix="ull-publish-slots-") as tmp:
        base = Path(tmp) / "base"
        snapshot(base)
        declared = declared_slots((base / PUBLISH_YML).read_text(encoding="utf-8")) or []
        if not declared:
            print("::error::自测的基线读不出 PUBLISH_SLOTS", file=sys.stderr)
            return 1

        cases: list[tuple[str, Path, Path | None, list[str] | None, bool]] = []

        # 0. 基线：名单齐全、产物齐全 ⇒ 绿。
        cases.append(("基线（名单与产物都齐）", base, fake_dist(Path(tmp) / "c0", tuple(declared)), None, False))

        # 1. 名单声明了，但缺 CLI 资产 ⇒ 必须红（这条就是「少一档不发版」）。
        dist = fake_dist(Path(tmp) / "c1", tuple(declared))
        for stale in dist.glob("ull_*"):
            stale.unlink()
        cases.append(("名单声明了但缺 CLI 产物", base, dist, None, True))

        # 2. 名单声明了，但整个槽位没有产物 ⇒ 必须红。
        cases.append(("名单声明了但整个槽位没产物", base, fake_dist(Path(tmp) / "c2", ()), None, True))

        # 3. 有产物、名单没声明 ⇒ 只警告不红（反方向的静默必须被打破，但发布本身没错）。
        #    名单内的槽位照旧齐全，只是多出一档没人声明的产物。
        cases.append(("产物在但名单没声明（arm64 修复后的典型场景）", base, fake_dist(Path(tmp) / "c3", (*declared, "windows-arm64")), None, False))

        # 4. 名单拼错一个字 ⇒ 必须红（此前要翻两层日志才能看出根因）。
        typo = Path(tmp) / "c4"
        snapshot(typo)
        set_declared(typo, "windows-aarch64")
        cases.append(("名单拼错成 windows-aarch64", typo, fake_dist(Path(tmp) / "c4d", ("windows-aarch64",)), None, True))

        # 5. 名单写了 probe 档（默认不构建）⇒ 必须红。**要挑仍是 probe 的槽位**：`linux-x64` 已翻成 `mode: ci`，
        #    再拿它当样例，这条用例就从「必须红」变成「本来该绿」——自测跟着矩阵走，否则它自己先变成假证据。
        probe = Path(tmp) / "c5"
        snapshot(probe)
        set_declared(probe, "windows-x64 macos-arm64")
        cases.append(("名单写了 mode=probe 的槽位", probe, fake_dist(Path(tmp) / "c5d", ("windows-x64", "macos-arm64")), None, True))

        # 6. 运行期名单与文件里那一行不一致 ⇒ 必须红（防步内覆盖）。
        cases.append(("运行期名单与文件不一致", base, fake_dist(Path(tmp) / "c6", tuple(declared)), ["windows-arm64"], True))

        # 7. 「把 arm64 加回名单」这一行改动本身是干净的：名单+产物都对得上 ⇒ 绿。
        flip = Path(tmp) / "c7"
        snapshot(flip)
        set_declared(flip, " ".join([*declared, "windows-arm64"]))
        cases.append(("名单加上 windows-arm64 且产物齐全（下一 tag 的翻牌预演）", flip, fake_dist(Path(tmp) / "c7d", (*declared, "windows-arm64")), None, False))

        # 8. （M184 核心）名单里的 `linux-x64` 构建红了（没有它的产物）⇒ **必须挡住发布**：`release` job 是
        #    `always()`，构建矩阵里红一档不会把发布跳过；挡住它的只能是这条。
        dist = fake_dist(Path(tmp) / "c8", tuple(s for s in declared if s != "linux-x64") + ("linux-arm64",))
        cases.append(("名单声明的 linux-x64 构建红了（缺产物）", base, dist, None, True))

        # 9. （M184 真实场景）`linux-arm64` 构建成功但不在名单 ⇒ 只警告、不挡发布（它是 `mode: ci` 的槽位，每轮都编）。
        cases.append(("linux-arm64 构建成功但不在名单（只警告）", base, fake_dist(Path(tmp) / "c9", (*declared, "linux-arm64")), None, False))

        # 10. 反过来：有人把 `linux-arm64` 提前写进名单、而它没构建出来 ⇒ 必须红（解锁条件还没满足）。
        early = Path(tmp) / "c10"
        snapshot(early)
        set_declared(early, " ".join([*declared, "linux-arm64"]))
        cases.append(("名单提前声明 linux-arm64 但缺产物", early, fake_dist(Path(tmp) / "c10d", tuple(declared)), None, True))

        for label, root, dist, expected, want_red in cases:
            errors, warnings, _rows = run_checks(root, dist, expected)
            got_red = bool(errors)
            if got_red != want_red:
                want = "红" if want_red else "绿"
                detail = "；".join(errors) or "（无 error）"
                print(f"::error::自测失败：{label} 应该{want}，实际 {'红' if got_red else '绿'} —— {detail}", file=sys.stderr)
                failures += 1
            else:
                print(f"OK: {label} → {'红' if got_red else '绿'}（{len(warnings)} 条未发布提示）")

        # 第 3 / 9 条还要断言**确实打了警告**：不静默才是这两条规则的重点。
        for ghost in ("windows-arm64", "linux-arm64"):
            _e, warnings, _r = run_checks(base, fake_dist(Path(tmp) / f"c-warn-{ghost}", (*declared, ghost)), None)
            if not any(ghost in w for w in warnings):
                print(f"::error::自测失败：产物在但名单没声明时没有针对 {ghost} 的 ::warning::", file=sys.stderr)
                failures += 1
            else:
                print(f"OK: 产物在但名单没声明 → 有 {ghost} 的 ::warning::")

        # 第 9 条的另一半：`mode: ci` 的槽位**连产物都没有**（构建红了）时也要有注解，不能只在表里少一行。
        ci_slots = [slot for slot, mode in matrix_slots((base / MATRIX_YML).read_text(encoding="utf-8")).items() if mode == "ci"]
        missing_ci = [slot for slot in ci_slots if slot not in declared]
        if not missing_ci:
            print("::error::自测失败：没有「mode=ci 但不在名单」的槽位，这条用例失去意义（矩阵是不是被改窄了？）", file=sys.stderr)
            failures += 1
        else:
            _e, warnings, _r = run_checks(base, fake_dist(Path(tmp) / "c-ci-red", tuple(declared)), None)
            for slot in missing_ci:
                if not any(slot in w and "ci" in w for w in warnings):
                    print(f"::error::自测失败：{slot} 是 mode=ci 却没产物时没有 ::warning::（红在哪都不知道）", file=sys.stderr)
                    failures += 1
                else:
                    print(f"OK: {slot} 是 mode=ci 却无产物 → 有 ::warning::")

    if failures:
        return 1
    print("OK: 发布名单守卫自测全部通过（该红的红、该绿的不红）")
    return 0


def main(argv: list[str]) -> int:
    for stream in (sys.stdout, sys.stderr):
        if hasattr(stream, "reconfigure"):
            stream.reconfigure(errors="backslashreplace")

    root = ROOT
    dist: Path | None = None
    expected: list[str] | None = None
    args = list(argv)
    while args:
        arg = args.pop(0)
        if arg == "--root" and args:
            root = Path(args.pop(0)).resolve()
        elif arg == "--dist" and args:
            dist = Path(args.pop(0)).resolve()
        elif arg == "--slots" and args:
            expected = parse_slots_arg(args.pop(0))
        elif arg == "--self-test":
            return self_test()
        elif arg in ("-h", "--help"):
            print(__doc__)
            return 0
        else:
            print(f"::error::不认识的参数：{arg}", file=sys.stderr)
            return 2

    if dist is not None and not dist.is_dir():
        print(f"::error::--dist {dist} 不是目录（下载 artifact 那一步没跑成？）", file=sys.stderr)
        return 1
    return report(root, dist, expected)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
