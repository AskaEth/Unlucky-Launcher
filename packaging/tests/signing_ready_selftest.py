#!/usr/bin/env python3
"""`packaging/check_signing_ready.py` 的判据自测（M255）：让「三行读数 + 0/1/2 退出码」变成可执行的事实。

为什么它必须存在：这个脚本的全部价值是「说清今天差什么」。它一旦退化成「永远 exit 0」，
它就从**播报**变成了本仓栽过四次的**假绿尺子** —— 而且没人会当场发现（输出看起来一模一样）。
所以这里的第一类用例判的是退出码语义，第二类用例判的是**把退出码改绿之后自测会红**（变异重放）。

| 用例 | 抓什么 |
|---|---|
| `empty_table` | ① 表空 ⇒ `exit 1`、三行都打印、有 fail-closed 句 + 修复行 |
| `pinned_and_key` | ② 副本夹假 `PinnedKey` ⇒ `exit 0`（**不碰真实 `trust.rs`**），且不泄种子 |
| `mutation` A/B | 退出码永远 0 ⇒ 判据①必须红；永远 1 ⇒ 判据②必须红（两向都要） |
| `key_mismatch` / `no_key` / `release_*` | ② 对账、③ 有/无/未知（含 `.sig` 子串假读数）、③ 不参与退出码 |
| `key_unreadable` | `--key` 不存在 / 是目录 ⇒ **2**（不是 1，也不是 traceback —— r1 P2-2） |
| `withheld` | 非 main ref ⇒ ② 写「未判定」+ 原因，**不写「没配」**（r1 P2-1 第三条） |
| `section_refs` | 输出里每个 `§N` 都是 runbook 真实章节号（r1 P2-3） |
| `stray_hint` / `no_seed_leak` / `usage` / `summary` | ① 标记外条目只点名不误报、种子不出现、用法错 = 2、`--line` 写 summary |

接线（不是门禁 / 不被 `uses:` / **ref 门** / 文档路径存在性）在
**`packaging/tests/signing_ready_wiring_selftest.py`** —— 拆成两份只因为全仓 400 行红线。
两份都不覆盖「整个 workflow 被删掉」（守门人住在被守对象旁边，与 `gpu_shot_ci_selftest.py` 同一条边界）。

`DUMMY_SEED` 是**由一句固定文本哈希出来的**假种子（同 `trust.rs` 里 `TEST_ONLY` 那把公开测试钥同一路数），
只用来造读数，签不出任何正式清单；文档与本文件都不含任何真实私钥。

    python3 packaging/tests/signing_ready_selftest.py
"""

from __future__ import annotations

import hashlib
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
SCRIPT = ROOT / "packaging/check_signing_ready.py"
RUNBOOK = ROOT / "packaging/docs/signing-runbook.md"
KEY_ENV = "ULL_MANIFEST_SIGNING_KEY"
WITHHELD_ENV = "ULL_SIGNING_KEY_WITHHELD"
FAIL_CLOSED = "这会让已安装用户永远验不过（fail-closed）"
LABELS = ("① 信任表 PRODUCTION", "② 环境签名钥", "③ 最近一次发布痕迹")

sys.path.insert(0, str(ROOT / "packaging"))
from ed25519_ref import public_key  # noqa: E402

DUMMY_SEED = hashlib.sha256(b"ull-m255-signing-ready-selftest-not-a-real-key").hexdigest()
DUMMY_PUB = public_key(bytes.fromhex(DUMMY_SEED)).hex()
OTHER_SEED = hashlib.sha256(b"ull-m255-selftest-the-other-dummy-key").hexdigest()

TRUST_TEMPLATE = """\
pub(crate) const PRODUCTION: &[PinnedKey] = &[
    // >>> PINNED-KEYS >>>
{body}    // <<< PINNED-KEYS <<<
];
"""


def secret_json(seed: str = DUMMY_SEED, key_id: str = "selftest-key") -> str:
    return '{"key_id": "%s", "seed_hex": "%s"}' % (key_id, seed)


def write(path: Path, text: str) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8", newline="\n")
    return path


def run(
    args: list[str],
    env_key: str | None,
    summary: Path | None = None,
    script: Path = SCRIPT,
    withheld: str | None = None,
) -> tuple[int, str]:
    """跑一次脚本，返回 `(退出码, stdout+stderr)`。

    `env_key` = 塞进 `KEY_ENV` 的内容（`None` = 不设）；`withheld` = 塞进 `WITHHELD_ENV` 的内容。
    两个 key 都先**从父环境剔除**再按参数注入：否则本机残留的变量会让用例假绿。
    """
    env = {k: v for k, v in os.environ.items() if k not in (KEY_ENV, WITHHELD_ENV)}
    env.pop("GITHUB_STEP_SUMMARY", None)
    # 子进程与父进程的编码显式对齐成 UTF-8：本机 Windows 默认 gbk，中文读数会 decode 失败，
    # 而 CI（ubuntu）本来就是 UTF-8。这条不是洁癖 —— 编码不一致会让整份自测在 Windows 上假红/假绿。
    env["PYTHONIOENCODING"] = "utf-8"
    if env_key is not None:
        env[KEY_ENV] = env_key
    if withheld is not None:
        env[WITHHELD_ENV] = withheld
    if summary is not None:
        env["GITHUB_STEP_SUMMARY"] = str(summary)
    done = subprocess.run(
        [sys.executable, str(script), *args],
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        env=env,
        cwd=str(ROOT),
        check=False,
    )
    return done.returncode, done.stdout + done.stderr


def case_empty_table(work: Path, failures: list[str], script: Path = SCRIPT) -> None:
    """① 今天的状态：空表 ⇒ exit 1，三行都要打印，且必须点明 fail-closed 后果与修复命令。

    `script` 可换：变异用例把**同一组断言**原样套在「退出码永远 0」的副本上，断言集合必须报红。
    """
    trust = write(work / "empty/trust.rs", TRUST_TEMPLATE.format(body=""))
    rc, out = run(["--trust", str(trust)], None, script=script)
    if rc != 1:
        failures.append(f"判据①：空表应 exit 1，实际 {rc}\n{out}")
    missing = [label for label in LABELS if label not in out]
    if missing:
        failures.append(f"判据①：空表时没打印这三行中的 {missing}\n{out}")
    if FAIL_CLOSED not in out:
        failures.append(f"判据①：未就绪时必须写明「{FAIL_CLOSED}」（后果，不是术语）\n{out}")
    if "修复：" not in out:
        failures.append(f"判据①：未就绪时必须给一行修复命令\n{out}")


def case_pinned_and_key(work: Path, failures: list[str], script: Path = SCRIPT) -> None:
    """② 副本里夹一条假 PinnedKey + 环境里是同一把钥 ⇒ exit 0（真实 trust.rs 一个字节都不碰）。"""
    body = f'    PinnedKey {{ key_id: "selftest-key", public_key_hex: "{DUMMY_PUB}" }},\n'
    trust = write(work / "pinned/trust.rs", TRUST_TEMPLATE.format(body=body))
    rc, out = run(["--trust", str(trust)], secret_json(), script=script)
    if rc != 0:
        failures.append(f"判据②：表非空 + 钥同源应 exit 0，实际 {rc}\n{out}")
    missing = [label for label in LABELS if label not in out]
    if missing:
        failures.append(f"判据②：就绪时也要打印三行，缺 {missing}\n{out}")
    if "就绪" not in out:
        failures.append(f"判据②：就绪时必须明说「就绪」\n{out}")
    if FAIL_CLOSED in out:
        failures.append(f"判据②：就绪时不该出现 fail-closed 那段警告\n{out}")
    # 不泄密：种子的十六进制在输出里一次都不许出现（公钥可以出现，它是公开物）。
    if DUMMY_SEED in out:
        failures.append(f"判据②：输出里出现了**种子**（私钥）—— 这是泄密\n{out}")


def case_key_mismatch(work: Path, failures: list[str]) -> None:
    """② 的对账：表里钉的是 A，环境里是 B ⇒ 未就绪（发布时 --check-pinned-keyring 会红）。"""
    body = f'    PinnedKey {{ key_id: "selftest-key", public_key_hex: "{DUMMY_PUB}" }},\n'
    trust = write(work / "mismatch/trust.rs", TRUST_TEMPLATE.format(body=body))
    rc, out = run(["--trust", str(trust)], secret_json(OTHER_SEED, "other-key"))
    if rc != 1:
        failures.append(f"②对账：钥不在表里应 exit 1，实际 {rc}\n{out}")
    if "不在" not in out:
        failures.append(f"②对账：必须明说这把钥不在信任表里\n{out}")
    if DUMMY_SEED in out or OTHER_SEED in out:
        failures.append(f"②对账：输出里出现了种子\n{out}")


def case_no_key(work: Path, failures: list[str]) -> None:
    """② 没有钥：即便表是好的，也算未就绪（签不出任何清单）。"""
    body = f'    PinnedKey {{ key_id: "selftest-key", public_key_hex: "{DUMMY_PUB}" }},\n'
    trust = write(work / "nokey/trust.rs", TRUST_TEMPLATE.format(body=body))
    rc, out = run(["--trust", str(trust)], None)
    if rc != 1:
        failures.append(f"②无钥：应 exit 1，实际 {rc}\n{out}")
    if KEY_ENV not in out:
        failures.append(f"②无钥：必须点名环境变量 {KEY_ENV}（换个人也知道该配什么）\n{out}")


def case_release_evidence(work: Path, failures: list[str]) -> None:
    """③ 只取证：有留档 / 有清单没签名 / 什么都没给 = 未知；且它不影响退出码。"""
    body = f'    PinnedKey {{ key_id: "selftest-key", public_key_hex: "{DUMMY_PUB}" }},\n'
    trust = write(work / "release/trust.rs", TRUST_TEMPLATE.format(body=body))
    signed = work / "release/signed"
    write(signed / "manifest.json", '{"channel": "stable", "releases": []}\n')
    write(signed / "manifest.json.sig", '{"format": 1, "signatures": []}\n')
    rc, out = run(["--trust", str(trust), "--dist", str(signed)], secret_json())
    if rc != 0 or "有 ——" not in out:
        failures.append(f"③有：留档齐全时应写「有」且退出码仍是 0，实际 rc={rc}\n{out}")

    bare = work / "release/bare"
    write(bare / "manifest.json", '{"channel": "stable", "releases": []}\n')
    rc, out = run(["--trust", str(trust), "--dist", str(bare)], secret_json())
    if "没有" not in out:
        failures.append(f"③无：只有 manifest.json 时必须点明「没有 .sig」\n{out}")

    nowhere = work / "release/absent"
    rc, out = run(["--trust", str(trust), "--dist", str(nowhere)], secret_json())
    if "未知" not in out:
        failures.append(f"③未知：取不到证据时必须写「未知」，不许写成「无」（本地没留档 ≠ 上次没签）\n{out}")
    if rc != 0:
        failures.append(f"③未知：③ 不参与就绪判定，退出码应仍是 0，实际 {rc}\n{out}")


def case_usage(work: Path, failures: list[str]) -> None:
    """用法错 = 2：参数写错、标记丢了（脚本形不成判断）。绝不能混进 1（那是「未就绪」的读数）。"""
    rc, out = run(["--nonsense"], None)
    if rc != 2:
        failures.append(f"用法错：未知参数应 exit 2，实际 {rc}\n{out}")
    broken = write(work / "broken/trust.rs", "pub(crate) const PRODUCTION: &[PinnedKey] = &[];\n")
    rc, out = run(["--trust", str(broken)], None)
    if rc != 2:
        failures.append(f"用法错：标记缺失应 exit 2（不是 1），实际 {rc}\n{out}")
    rc, out = run(["--trust", str(work / "no-such-dir/trust.rs")], None)
    if rc != 2:
        failures.append(f"用法错：trust.rs 不存在应 exit 2，实际 {rc}\n{out}")


def case_summary(work: Path, failures: list[str]) -> None:
    """`--line`：CI 用，把三行搬进 $GITHUB_STEP_SUMMARY；且必须写明「不是门禁」。"""
    trust = write(work / "summary/trust.rs", TRUST_TEMPLATE.format(body=""))
    summary = work / "summary-step.md"
    rc, out = run(["--trust", str(trust), "--line"], None, summary=summary)
    if rc != 1:
        failures.append(f"summary：空表仍应 exit 1，实际 {rc}\n{out}")
    if not summary.is_file():
        failures.append("summary：给了 $GITHUB_STEP_SUMMARY 却没写文件")
        return
    text = summary.read_text(encoding="utf-8")
    missing = [label for label in LABELS if label not in text]
    if missing:
        failures.append(f"summary：三行没全部进 summary，缺 {missing}\n{text}")
    if "不代表签名就绪" not in text:
        failures.append(f"summary：必须写清「绿色不代表签名就绪」（播报 ≠ 门禁）\n{text}")


TARGET_LINE = "    return 0 if ready else 1\n"


def mutant(work: Path, name: str, replacement: str) -> Path | None:
    """造一份「只改了退出码那一行」的副本。副本自成一体（模块一起拷）：脚本按 `__file__` 上一级找
    `packaging/`，不靠 PYTHONPATH（实测本机 3.12 根本不把 PYTHONPATH 放进 sys.path）。"""
    source = SCRIPT.read_text(encoding="utf-8")
    if source.count(TARGET_LINE) != 1:
        return None
    root = work / name
    path = write(root / "packaging/check_signing_ready_mutated.py", source.replace(TARGET_LINE, replacement))
    for module in ("sign_manifest.py", "ed25519_ref.py"):
        write(root / f"packaging/{module}", (ROOT / f"packaging/{module}").read_text(encoding="utf-8"))
    write(root / "trust.rs", TRUST_TEMPLATE.format(body=""))
    return path


def case_stray_hint(work: Path, failures: list[str]) -> None:
    """① 的「标记之外还有条目」提示：只**点名**，不替人断言「贴错了」。

    实测抓到的误报：真实 `trust.rs` 里 `TEST_ONLY` 那一条**本来就在标记之外**（那是它的正确位置），
    上一版措辞「另有 N 条写在标记之外（贴错位置，等于没钉）」会对今天的状态喊狼来了。
    """
    outside = 'pub(crate) const TEST_ONLY: &[PinnedKey] = &[PinnedKey { key_id: "test-only-key", public_key_hex: "%s" }];\n'
    trust = write(
        work / "stray/trust.rs",
        outside % ("ab" * 32) + TRUST_TEMPLATE.format(body=""),
    )
    rc, out = run(["--trust", str(trust)], None)
    if rc != 1:
        failures.append(f"①提示：标记外有条目时表仍是空的 ⇒ 应 exit 1，实际 {rc}\n{out}")
    if "test-only-key" not in out:
        failures.append(f"①提示：标记之外的条目必须**点名**（否则人不知道贴漏了哪一条）\n{out}")
    if "TEST_ONLY" not in out:
        failures.append(f"①提示：必须说清 `TEST_ONLY` 本来就在标记之外（否则这就是一句误报）\n{out}")


def case_key_unreadable(work: Path, failures: list[str]) -> None:
    """② `--key` 指向读不了的路径 ⇒ **2**（用法错），不是 1、也不是 traceback（r1 P2-2）。

    `read_text` 曾在 `main()` 的 try 之外 ⇒ `FileNotFoundError` 直接抛成 rc=1，与「未就绪」同码。
    脚本的全部价值是「退出码可信」：1 和 2 混了，「我路径写错了」会被读成「签名没配好」。
    """
    body = f'    PinnedKey {{ key_id: "selftest-key", public_key_hex: "{DUMMY_PUB}" }},\n'
    trust = write(work / "badkey/trust.rs", TRUST_TEMPLATE.format(body=body))
    directory = work / "badkey/a-directory"
    directory.mkdir(parents=True, exist_ok=True)
    for label, path in (
        ("不存在", work / "badkey/no-such.key"),
        ("是目录", directory),
    ):
        rc, out = run(["--trust", str(trust), "--key", str(path)], None)
        if rc != 2:
            failures.append(f"--key（{label}）应 exit 2，实际 {rc}\n{out}")
        if "Traceback" in out:
            failures.append(f"--key（{label}）不许抛未捕获异常（traceback 会盖掉一行说明）\n{out}")
        if "用法错" not in out:
            failures.append(f"--key（{label}）必须说清这是用法错，不是「未就绪」\n{out}")
        if LABELS[0] in out or LABELS[1] in out:
            failures.append(f"--key（{label}）在形不成判断时不该打印三行读数（会被读成「读数」）\n{out}")


def case_withheld(work: Path, failures: list[str]) -> None:
    """② 非 main ref：写「未判定 + 原因」，**不写「没配」**（r1 P2-1 第三条）。

    同一份空钥输入，有没有 `ULL_SIGNING_KEY_WITHHELD` 是两件事：「策略上不注入」不是「仓库没配置」。
    """
    body = f'    PinnedKey {{ key_id: "selftest-key", public_key_hex: "{DUMMY_PUB}" }},\n'
    trust = write(work / "withheld/trust.rs", TRUST_TEMPLATE.format(body=body))
    reason = "非 main ref（按设计不注入 Secret；要读这一行请对 main 跑）"
    rc, out = run(["--trust", str(trust)], None, withheld=reason)
    if rc != 1:
        failures.append(f"未判定：这一趟没钥可读 ⇒ 不许声称就绪（应 exit 1），实际 {rc}\n{out}")
    if "未判定" not in out or reason not in out:
        failures.append(f"未判定：② 必须写「未判定」+ 原因串（否则会被读成「没配」）\n{out}")
    if "未注入" not in out:
        failures.append(f"未判定：② 必须写出「未注入」这个动作（「策略不注入」与「没配置」是两件事）\n{out}")
    if "未设置" in out:
        failures.append(f"未判定：非 main ref 上不许出现「未设置」（那是「没配」的措辞）\n{out}")
    # 反方向：没有 withheld 时照样得说「未设置/为空」，别把两档并成一档。
    rc, out = run(["--trust", str(trust)], None)
    if "未设置" not in out:
        failures.append(f"未判定：没有 withheld 时仍应报「未设置/为空」（两档不许合并）\n{out}")


def case_section_refs(work: Path, failures: list[str]) -> None:
    """输出里出现的每个 `§N` 都必须是 runbook **真实存在**的章节号（r1 P2-3）。

    上一版把「贴 PinnedKey」指到 §3（那里讲发布时的执行顺序）、把「发布后回验」指到 §4（那里是已知坑）。
    本文件的交付物就是「一行可照抄的指针」，所以指针要机器校验。
    """
    sections = set(re.findall(r"^##\s*(\d+)\.", RUNBOOK.read_text(encoding="utf-8"), re.MULTILINE))
    if not sections:
        failures.append("指针：runbook 里一个 `## N.` 标题都没解析到（章节号格式变了？）")
        return
    body = f'    PinnedKey {{ key_id: "selftest-key", public_key_hex: "{DUMMY_PUB}" }},\n'
    trust = write(work / "pointer/trust.rs", TRUST_TEMPLATE.format(body=body))
    empty = write(work / "pointer/empty.rs", TRUST_TEMPLATE.format(body=""))
    seen: set[str] = set()
    for args, key in (
        (["--trust", str(trust)], secret_json()),  # 就绪：结论指向步⑤ / §5
        (["--trust", str(trust)], None),  # ② 没钥：指向步②
        (["--trust", str(empty)], None),  # ① 空表：修复行指向步①–③
    ):
        seen |= set(re.findall(r"§(\d+)", run(args, key)[1]))
    if not seen:
        failures.append("指针：一条 `§N` 都没从输出里抓到（用例失效了）")
    unknown = sorted(seen - sections)
    if unknown:
        failures.append(f"指针：输出里指向了 runbook 没有的章节 {unknown}（真实章节：{sorted(sections)}）")


def case_mutation(work: Path, failures: list[str]) -> None:
    """变异重放：把被测对象改坏，**同一组断言原样套上去必须报红**（两个方向）。
    （接线/ref 门那一侧的变异在 `signing_ready_wiring_selftest.py`。）"""
    # ── 变异 A：退出码永远 0（「未就绪」伪装成「就绪」）⇒ 判据① 必须红 ──
    always_zero = mutant(work, "mutant-zero", "    return 0\n")
    if always_zero is None:
        failures.append(f"变异 A：{SCRIPT.name} 里找不到唯一的 `{TARGET_LINE.strip()}`，变异无处下手（脚本形状变了就更新这里）")
    else:
        trust = work / "mutant-zero/trust.rs"
        probe: list[str] = []
        case_empty_table(work / "mutant-zero", probe, script=always_zero)
        # 副本必须真的「跑得起来、只有退出码变了」：崩溃（rc=2）也会让 probe 非空，那就变成假绿了。
        rc, out = run(["--trust", str(trust)], None, script=always_zero)
        if rc != 0:
            failures.append(f"变异 A：副本在空表上应返回 0（这就是变异本身），实际 {rc}\n{out}")
        missing = [label for label in LABELS if label not in out]
        if missing:
            failures.append(f"变异 A：副本连三行读数都没打印（缺 {missing}），它不是在测退出码那一条\n{out}")
        if not probe:
            failures.append(f"变异 A：「退出码永远 0」的副本竟然让判据①全过 —— 断言抓不住改绿（副本空表返回 {rc}）")
        elif not any("exit 1" in item for item in probe):
            failures.append(f"变异 A：副本被判红，但不是因为退出码那一条 —— 变异用例没对准判据①：{probe}")

    # ── 变异 B：退出码永远 1（「就绪」伪装成「未就绪」）⇒ 判据② 必须红 ──
    # 反向也要试：只测一个方向的话，「就绪的正路根本不存在」这种回归仍然全绿。
    always_one = mutant(work, "mutant-one", "    return 1\n")
    if always_one is None:
        failures.append(f"变异 B：{SCRIPT.name} 里找不到唯一的 `{TARGET_LINE.strip()}`")
    else:
        probe = []
        case_pinned_and_key(work / "mutant-one", probe, script=always_one)
        if not probe:
            failures.append("变异 B：「退出码永远 1」的副本竟然让判据②全过 —— 就绪的正路没被断言（假绿的反面）")
        elif not any("exit 0" in item for item in probe):
            failures.append(f"变异 B：副本被判红，但不是因为退出码那一条 —— 变异用例没对准判据②：{probe}")


def main() -> int:
    failures: list[str] = []
    with tempfile.TemporaryDirectory(prefix="ull-signing-ready-") as temp:
        work = Path(temp)
        case_empty_table(work, failures)
        case_pinned_and_key(work, failures)
        case_key_mismatch(work, failures)
        case_no_key(work, failures)
        case_release_evidence(work, failures)
        case_stray_hint(work, failures)
        case_key_unreadable(work, failures)
        case_withheld(work, failures)
        case_section_refs(work, failures)
        case_usage(work, failures)
        case_summary(work, failures)
        case_mutation(work, failures)

    for line in failures:
        print(f"FAIL {line}", file=sys.stderr)
    print("signing-ready selftest:", "ok" if not failures else f"{len(failures)} 处失败")
    return 0 if not failures else 1


if __name__ == "__main__":
    raise SystemExit(main())
