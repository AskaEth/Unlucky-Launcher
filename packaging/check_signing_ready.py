#!/usr/bin/env python3
"""签名就绪度播报（M255）：**只读**、零第三方依赖，三行说清「今天能不能签发一份客户端会认的清单」。

为什么要它：M254（CLAIMAUDIT）查出 `crates/core/src/update/trust.rs` 的 `PRODUCTION` 表在
`PINNED-KEYS` 标记之间是**空的**，而空表一律 `KeyringEmpty` 拒装 —— 于是**今天任何正式二进制都验不过
任何清单**（fail-closed 是成立的，但在用户机器上表现为「更新永远失败」）。工具链早就齐了
（`sign_manifest.py` 的 `--generate-key` / `--check-pinned-keyring` / `--sign` / `--verify-pinned`），
缺的是**操作步骤与可见性**。本脚本只负责「现在差什么」，它不生成钥、不写任何文件、不改 `trust.rs`。

三行就绪度（各一条判据，互不替代）：

| 行 | 判据 | 未就绪意味着 |
|---|---|---|
| ① | `PRODUCTION` 表非空：解析 `PINNED-KEYS` 标记之间的 `PinnedKey` 条目，0 条即未就绪 | 客户端一把公钥都没有，任何清单都验不过（今天就是这个状态） |
| ② | 环境里有签名钥：`ULL_MANIFEST_SIGNING_KEY`（或 `--key <文件>`）能解出 32 字节种子，**且**它的公钥在 ① 的表里 | 没钥签不了；钥与信任表不是同一把 ⇒ 发布时 `--check-pinned-keyring` 当场红 |
| ③ | 最近一次发布的 `manifest.json` / `.sig` 痕迹 | **不参与就绪判定**。取不到证据就写「未知」——「本地没留档」不等于「上次没签」 |

②里的对账（`公钥 ∈ 表`）是刻意加的：只说「有钥」会给出一个**看着就绪、发布却红**的假象，与本仓
「别再造假绿的尺子」的血泪账直接冲突。公钥是公开物，回显它是安全的；**种子（`seed_hex`）在本脚本里
一次都不打印**。

②还有第三种读数：**未判定**。CI 的播报 workflow 刻意只在 `main` ref 上注入 Secret（生产签名种子等于
「给全量已装客户端签可信清单」的能力，既有设计把它锁在 tag ref 后面），非 main ref 的运行里这一行会写
「未判定 —— 非 main ref（按设计不注入）」，**不写「没配」**：把「策略不注入」说成「没配」就是一次假读数。

退出码是对外接口（改动即行为变化，`packaging/tests/signing_ready_selftest.py` 钉着它）：
`0` 就绪 / `1` 未就绪（含 ② 未判定） / `2` 用法错（trust.rs 或 `--key` 读不到、标记丢了 ——
那时脚本形不成判断，不是「未就绪」）。

**它不是门禁**：CI 里的 `.github/workflows/signing-status.yml` 只把这三行搬进 job summary，
不因「未就绪」变红。理由见 `packaging/docs/signing-runbook.md` §6。

    python3 packaging/check_signing_ready.py
    python3 packaging/check_signing_ready.py --trust <副本 trust.rs> --key <钥文件> --dist dist
"""

from __future__ import annotations

import argparse
import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SIGN = ROOT / "packaging/sign_manifest.py"
RUNBOOK = "packaging/docs/signing-runbook.md"
#: 未就绪时的固定措辞：判据与「人类一眼看懂后果」绑在一起（用例按字面断言这一句）。
FAIL_CLOSED = "这会让已安装用户永远验不过（fail-closed）"
#: 「这一趟按设计没有钥」的原因（人话）。CI 的 `signing-status.yml` 在非 main ref 上把它设成一句话，
#: 于是 ② 读成「未判定」而不是「没配」—— 把**策略不注入**说成**没配置**是一次假读数（M255 r1 P2-1）。
WITHHELD_ENV = "ULL_SIGNING_KEY_WITHHELD"


sys.path.insert(0, str(ROOT / "packaging"))

# 标记 / 环境变量名 / 信任表路径一律**从 sign_manifest.py 取**，不在这里写第二份字面量：
# 两份标记各自漂开时，`--check-pinned-keyring` 与就绪度会给出互相矛盾的读数。
from sign_manifest import KEY_ENV, KID_RE, PIN_BEGIN, PIN_END, TRUST_RS, load_key  # noqa: E402
from ed25519_ref import public_key  # noqa: E402

Line = tuple[bool, str]  # (就绪?, 该行文本)


def table_state(path: Path) -> tuple[bool, str, list[tuple[str, str]]]:
    """① 判据：`PINNED-KEYS` 标记之间有至少一条 `PinnedKey`。

    标记缺失 / 文件读不到 ⇒ `RuntimeError`（调用方翻成退出码 2：脚本无法形成判断）。
    """
    text = path.read_text(encoding="utf-8")
    begin, end = text.find(PIN_BEGIN), text.find(PIN_END)
    if begin < 0 or end < begin:
        raise RuntimeError(
            f"{path} 里找不到 {PIN_BEGIN} / {PIN_END} 标记（标记本身不许动 —— sign_manifest.py 也按它解析）"
        )
    inside = text[begin:end]
    keys = KID_RE.findall(inside)
    line_no = text[:begin].count("\n") + 1
    if keys:
        names = "、".join(f"{kid}（{pk[:16]}…）" for kid, pk in keys)
        return True, f"就绪 —— 标记之间 {len(keys)} 条：{names}（{path}:{line_no} 起）", keys
    # 空表就是今天的状态。顺手查一个真实易错点：有人把条目贴到标记**外面**（`];` 之后）。
    # ⚠ 本仓另有一把 `TEST_ONLY` 测试钥**本来就在标记之外**（那是它的正确位置），所以这里只**点名**，
    # 不替人下结论 —— 上一版写成「贴错位置，等于没钉」，在真实 trust.rs 上是误报（实测抓出来的）。
    strays = [kid for kid, _ in KID_RE.findall(text[:begin]) + KID_RE.findall(text[end:])]
    hint = ""
    if strays:
        hint = (
            f"；文件里另有 {len(strays)} 条在标记之外（key_id：{'、'.join(strays[:3])}）"
            "—— `TEST_ONLY` 那条本来就在外面，属正常；**生产钥**贴在外面等于没钉"
        )
    return False, f"未就绪 —— 标记之间 0 条 PinnedKey{hint}（{path}:{line_no} 起）", []


def read_key_text(key_file: str | None) -> str:
    """② 的输入文本：`--key <文件>` 或环境变量 `ULL_MANIFEST_SIGNING_KEY`。

    读不了（不存在 / 是目录 / 无权限 / 非 UTF-8）⇒ `OSError` / `UnicodeDecodeError`，由 `main` 翻成
    **退出码 2**（用法错）。⚠ 这里必须抛而不是吞：吞掉就会变成 rc=1，与「未就绪」同码，
    而事实是「你给的路径就是错的」——两者混了，退出码就不再可信（M255 r1 P2-2）。
    """
    if key_file:
        return Path(key_file).read_text(encoding="utf-8")
    return os.environ.get(KEY_ENV, "")


def key_state(raw: str, source: str, pinned: list[tuple[str, str]]) -> tuple[bool, str, str | None]:
    """② 判据：环境里有钥、解析得出、且它的公钥在 ① 的表里。

    返回 `(就绪?, 该行文本, 可直接贴进 trust.rs 的那一行 PinnedKey)`。
    **任何分支都不打印种子**；回显的只有 `key_id` 与公钥十六进制。
    """
    if not raw.strip():
        reason = os.environ.get(WITHHELD_ENV, "").strip()
        if reason:
            # 「策略上不注入」≠「没配置」。这一行必须能分辨，否则非 main ref 的运行会报假的「没配」。
            return False, f"未判定（未注入）—— 这一趟的钥按设计没有注入：{reason}；这不是「没配」", None
        return False, f"未就绪 —— {source} 未设置/为空（没有私钥就签不出任何清单）", None
    try:
        key_id, seed = load_key(raw)
    except SystemExit as error:  # load_key 用 SystemExit 报错（它会 sys.exit）；这里降级成一行读数
        return False, f"未就绪 —— {source} 读不出钥：{error}", None
    pk_hex = public_key(seed).hex()
    paste = f'PinnedKey {{ key_id: "{key_id}", public_key_hex: "{pk_hex}" }},'
    for pinned_id, pinned_hex in pinned:
        if pinned_hex.lower() == pk_hex:
            note = "" if pinned_id == key_id else f"（表里叫 {pinned_id}，钥文件叫 {key_id}；判据是公钥字节，能过）"
            return True, f"就绪 —— {source} 里有钥 {key_id}，公钥 {pk_hex} 已在信任表里{note}", paste
    if pinned:
        return False, f"未就绪 —— {source} 里有钥 {key_id}（公钥 {pk_hex}），但**不在**信任表里，发布时会被拦", paste
    return False, f"未就绪 —— {source} 里有钥 {key_id}（公钥 {pk_hex}），但信任表是空的，没处钉", paste


def release_state(dist: Path | None, assets: Path | None) -> tuple[bool | None, str]:
    """③ 判据：最近一次发布有没有 `manifest.json` / `.sig` 痕迹。**只取证，不参与就绪判定。**

    `None` = 未知（既没给 `--dist` 里的留档，也没给 `--release-assets`）。「本地没留档」不是
    「上次没签」的证据，所以这一档只写「未知」，不写「无」。
    """
    if dist is not None and dist.is_dir():
        manifest = dist / "manifest.json"
        signature = dist / f"{manifest.name}.sig"
        if manifest.is_file() and signature.is_file():
            return True, f"有 —— 本地留档 {manifest} + {signature}"
        if manifest.is_file():
            return False, f"无 —— 本地 {dist} 里有 manifest.json 但**没有** .sig（那份清单客户端会拒收）"
    if assets is not None and assets.is_file():
        text = assets.read_text(encoding="utf-8", errors="replace")
        # 先剥掉 `.sig` 再判清单：`manifest.json.sig` 里含 `manifest.json`，只做子串匹配时
        # 「资产里只有签名」会被读成「清单和签名都在」（r1 复审实测抓到的假读数）。
        has_sig = "manifest.json.sig" in text
        has_manifest = "manifest.json" in text.replace("manifest.json.sig", "")
        if has_manifest and has_sig:
            return True, f"有 —— {assets} 的资产清单里同时有 manifest.json 与 manifest.json.sig"
        if has_manifest:
            return False, f"无 —— {assets} 的资产清单里有 manifest.json，但没有 manifest.json.sig（没签名）"
        return False, f"无 —— {assets} 的资产清单里既没有 manifest.json 也没有 .sig（那次发布没走上签名这条链）"
    looked = [str(p) for p in (dist, assets) if p is not None]
    where = f"（看过：{'、'.join(looked)}）" if looked else "（没给 --dist / --release-assets，也没本地 dist/ 留档）"
    return None, f"未知 —— 取不到任何发布产物证据{where}"


def fix_hint(table_ok: bool, key_ok: bool, paste: str | None) -> str:
    """未就绪时的**一行**修复命令（不带任何密钥值；钥的事只指向 Secret 名与手册）。

    指针必须落在手册**真实**的章节上：`packaging/tests/signing_ready_wiring_selftest.py` 会拿这里的
    `§N` 去对 `signing-runbook.md` 的标题集合，漂了当场红（M255 r1 P2-3：`signing-ready.yml` 那个
    文件名根本不存在，而指针正是本文件的交付物）。
    """
    if not table_ok and paste:
        return f"修复：把这一行贴进 {TRUST_RS.name} 的 PINNED-KEYS 标记之间 —— {paste}（手册 {RUNBOOK} §2 步③）"
    if not table_ok:
        return (
            "修复：python3 packaging/sign_manifest.py --generate-key --key-id <key-id>  "
            f"→ 私钥贴进 Secret {KEY_ENV}，打印的 PinnedKey 那行贴进 trust.rs 标记之间（手册 {RUNBOOK} §2 步①–③）"
        )
    if not key_ok and paste:
        return f"修复：把这一行贴进 {TRUST_RS.name} 的 PINNED-KEYS 标记之间 —— {paste}（手册 {RUNBOOK} §2 步③）"
    return f"修复：把密钥贴进仓库 Secret {KEY_ENV}（内容 = --generate-key 打印的那行 JSON；手册 {RUNBOOK} §2 步②）"


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="签名就绪度播报：三行读数 + 0/1/2 退出码（M255）")
    parser.add_argument("--trust", type=Path, default=TRUST_RS, help=f"trust.rs 路径（默认 {TRUST_RS}）")
    parser.add_argument("--key", metavar="FILE", help=f"钥文件（不给则读环境变量 {KEY_ENV}）")
    parser.add_argument("--dist", type=Path, default=Path("dist"), help="本地留档目录（默认 dist/；不存在则不看）")
    parser.add_argument("--release-assets", type=Path, metavar="FILE", help="某次 Release 的资产清单文本（如 gh release view 的输出）")
    parser.add_argument("--line", action="store_true", help="CI 用：额外把三行写进 $GITHUB_STEP_SUMMARY")
    args = parser.parse_args(argv)

    try:
        table_ok, table_line, pinned = table_state(args.trust)
    except (OSError, UnicodeDecodeError, RuntimeError) as error:
        print(f"error: 读不了信任表：{error}（脚本无法形成判断，按用法错退出）", file=sys.stderr)
        return 2
    try:
        key_text = read_key_text(args.key)
    except (OSError, UnicodeDecodeError) as error:
        # 与 `--trust` 同款：输入读不到 = 用法错（2）。**不能**退化成 1 —— 1 是「签名未就绪」的读数，
        # 混了就会把「我路径写错了」读成「签名没配好」（M255 r1 P2-2）。
        print(f"error: 读不了钥文件 {args.key}：{error}（这是用法错，不是「未就绪」）", file=sys.stderr)
        return 2
    source = f"--key {args.key}" if args.key else f"环境变量 {KEY_ENV}"
    key_ok, key_line, paste = key_state(key_text, source, pinned)
    release_ok, release_line = release_state(args.dist, args.release_assets)
    withheld = not key_ok and not key_text.strip() and bool(os.environ.get(WITHHELD_ENV, "").strip())

    lines: list[Line] = [
        ("① 信任表 PRODUCTION", table_ok, table_line),
        ("② 环境签名钥", key_ok, key_line),
        ("③ 最近一次发布痕迹", release_ok is not False, release_line),
    ]
    for label, _ok, text in lines:
        print(f"{label}：{text}")

    ready = table_ok and key_ok
    if ready:
        print(f"结论：就绪（{args.trust} 里钉了钥，环境里的钥与它同源）—— 可以按 {RUNBOOK} §2 步⑤ 发一次带 .sig 的 Release，再按 §5 回验")
    elif withheld:
        # 「未判定」不许伪装成「未就绪」：这一趟根本没拿到钥，不是仓库缺钥。
        tail = f"；① 仍是空的 ⇒ {FAIL_CLOSED}" if not table_ok else ""
        print(f"结论：未判定（② 这一趟没注入钥，见上面那行的原因）{tail}")
    else:
        blockers = [label for label, ok, _ in lines[:2] if not ok]
        print(f"结论：未就绪（差 {'、'.join(blockers)}）—— {FAIL_CLOSED}")
        print(fix_hint(table_ok, key_ok, paste))

    if args.line:
        summary = os.environ.get("GITHUB_STEP_SUMMARY")
        if summary:
            verdict = "就绪" if ready else ("未判定" if withheld else "未就绪")
            body = ["## 签名就绪度（M255，播报不是门禁）", ""]
            body += [f"- **{label}**：{text}" for label, _ok, text in lines]
            body += [
                "",
                f"- 结论：**{verdict}**",
                f"- 本 job 的绿色**不代表签名就绪**：判据只看上面三行字，退出码恒为 0（见 `{RUNBOOK}` §6）。",
            ]
            Path(summary).write_text("\n".join(body) + "\n", encoding="utf-8")

    return 0 if ready else 1


if __name__ == "__main__":
    raise SystemExit(main())
