#!/usr/bin/env python3
"""Sign / verify the update manifest, and check the pinned keyring (M190).

Why Ed25519 + a **detached** `.sig`: the manifest's `sha256` only says "the package matches the
manifest"; it says nothing about who wrote the manifest. A signature over the manifest's *exact
bytes* answers that, and a separate file avoids JSON canonicalisation games (any byte change
breaks the signature).

Key discipline: the **private key** lives only in the repository Secret
`ULL_MANIFEST_SIGNING_KEY` (never in git, logs or artifacts); the **public key** is pinned
*inside the binary* — `crates/core/src/update/trust.rs`, between the `>>> PINNED-KEYS >>>` /
`<<< PINNED-KEYS <<<` markers. `--check-pinned-keyring` fails the release when the secret's key
is not pinned (a release signed with a key clients do not trust is worse than no release).
Manual: `packaging/docs/signing.md`.

Usage: `--generate-key [--key-id K]` / `--sign dist/manifest.json`（钥从环境变量
`ULL_MANIFEST_SIGNING_KEY` 取）/ `--check-pinned-keyring`（钥必须已钉住）/ `--verify-pinned
dist/manifest.json`（用 trust.rs 里钉的公钥回验产出，复现客户端判据）/ `--self-test`。
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

FORMAT = 1
ALG = "ed25519"
KEY_ENV = "ULL_MANIFEST_SIGNING_KEY"
SIG_SUFFIX = ".sig"
TRUST_RS = Path(__file__).resolve().parent.parent / "crates/core/src/update/trust.rs"
RUSTFMT_TOML = Path(__file__).resolve().parent.parent / ".rustfmt.toml"
PIN_BEGIN = ">>> PINNED-KEYS >>>"
PIN_END = "<<< PINNED-KEYS <<<"
# 粘贴块排版：`trust.rs` 里数组元素缩进 4 空格、结构体字段再缩进 4（= 元素缩进 + 4）。
PASTE_INDENT = "    "
KID_RE = re.compile(r'key_id:\s*"([^"]+)"\s*,\s*public_key_hex:\s*"([0-9a-fA-F]{64})"')
# Ed25519 的参考实现搬到了 `packaging/ed25519_ref.py`（M190 r1：本文件贴着 400 行红线）。
# 目的与路径都写在那边的文件头：发布侧签名必须零第三方依赖，而仓里验签用的是 ed25519-dalek ——
# 两套实现互相独立，`--self-test` 用 RFC 8032 的同一组向量把两边各自钉住。
sys.path.insert(0, str(Path(__file__).resolve().parent))

from ed25519_ref import ALG, sign, verify, public_key  # noqa: E402

def load_key(text: str) -> tuple[str, bytes]:
    """Parse the secret JSON (`{"key_id":…,"seed_hex":…}`) or a bare 64-char hex seed."""
    text = text.strip()
    try:
        payload = json.loads(text)
    except json.JSONDecodeError:
        payload = None
    if isinstance(payload, dict):
        key_id = str(payload.get("key_id", "")).strip()
        seed_hex = str(payload.get("seed_hex", "")).strip()
    else:
        key_id, seed_hex = "", text
    if not key_id:
        raise SystemExit("error: 密钥缺少 `key_id`（清单签名要靠它查信任表）")
    try:
        seed = bytes.fromhex(seed_hex)
    except ValueError as error:
        raise SystemExit(f"error: `seed_hex` 不是十六进制（{error}）") from error
    if len(seed) != 32:
        raise SystemExit(f"error: `seed_hex` 必须 32 字节（实际 {len(seed)}）")
    return key_id, seed

def key_from_env_or_arg(path: str | None) -> tuple[str, bytes]:
    if path:
        secret = Path(path).read_text(encoding="utf-8")
    else:
        secret = os.environ.get(KEY_ENV, "")
        if not secret.strip():
            raise SystemExit(
                f"error: 没有签名密钥：设环境变量 {KEY_ENV}（内容 = --generate-key 打出来的那一行 JSON），"
                "或显式给 --key <文件>。**绝不**把私钥写进仓库/日志/产物。"
            )
    return load_key(secret)

def pinned_keys(trust_text: str) -> list[tuple[str, str]]:
    """Extract `(key_id, public_key_hex)` pairs between the PINNED-KEYS markers."""
    begin = trust_text.find(PIN_BEGIN)
    end = trust_text.find(PIN_END)
    if begin < 0 or end < 0 or end < begin:
        raise SystemExit(f"error: {TRUST_RS} 里找不到 {PIN_BEGIN} / {PIN_END} 标记")
    return KID_RE.findall(trust_text[begin:end])

def render_pinned_key(key_id: str, public_key_hex: str) -> str:
    """渲染一条**粘贴即可、`cargo fmt` 不再改形**的 `PinnedKey`（M265）。

    为什么必须多行：单行形态即使 `key_id` 为空也有 113 字符，超出 `.rustfmt.toml` 的 `max_width = 100`，
    所以 `cargo fmt` **必然**把它拆成下面这个形状 —— T30 的 fmt 红就是这么进的 main（人贴单行 → 下一轮
    fmt 改动 → 谁忘了跑 fmt 就带红进主干）。反过来说这个形状过一遍 rustfmt 不变，`--self-test` 会验。
    """
    pad = PASTE_INDENT
    return (
        f"{pad}PinnedKey {{\n"
        f'{pad}    key_id: "{key_id}",\n'
        f'{pad}    public_key_hex: "{public_key_hex}",\n'
        f"{pad}}},"
    )

def rustfmt_max_width() -> int:
    """`.rustfmt.toml` 里的 `max_width`（读不到就用 rustfmt 的默认 100）。"""
    try:
        for line in RUSTFMT_TOML.read_text(encoding="utf-8").splitlines():
            key, _, value = line.partition("=")
            if key.strip() == "max_width":
                return int(value.strip())
    except (OSError, ValueError):
        pass
    return 100

def pinned_entry_blocks(trust_text: str) -> list[str]:
    """标记之间每条 `PinnedKey { … }` 的**原样文本**（含缩进与结尾逗号）。

    与 `pinned_keys()` 互补：那个只看字段值（跨行跨不跨行都吃），这个看**排版**。
    """
    begin, end = trust_text.find(PIN_BEGIN), trust_text.find(PIN_END)
    if begin < 0 or end < 0 or end < begin:
        return []
    return re.findall(r"^[ \t]*PinnedKey \{.*?^[ \t]*\},", trust_text[begin:end], re.MULTILINE | re.DOTALL)

def rustfmt_keeps_shape(snippet: str) -> bool | None:
    """把 `snippet` 按 `trust.rs` 的上下文贴进临时副本、真跑一遍 `rustfmt`：形态被保持 ⇒ True。

    `rustfmt` 不在 PATH 上 ⇒ None（自测退化成结构断言，并把「跳过」说出来，不装成验过）。
    """
    rustfmt = shutil.which("rustfmt")
    if not rustfmt:
        return None
    args = [rustfmt, "--edition", "2021"]
    if RUSTFMT_TOML.is_file():
        args += ["--config-path", str(RUSTFMT_TOML)]
    with tempfile.TemporaryDirectory() as tmp:
        src = Path(tmp) / "pasted.rs"
        src.write_text(
            "struct PinnedKey {\n    key_id: &'static str,\n    public_key_hex: &'static str,\n}\n\n"
            f"const PRODUCTION: &[PinnedKey] = &[\n    // {PIN_BEGIN}\n{snippet}\n    // {PIN_END}\n];\n",
            encoding="utf-8",
            newline="\n",
        )
        result = subprocess.run([*args, str(src)], capture_output=True, text=True, timeout=120)
        if result.returncode != 0:
            raise RuntimeError(f"rustfmt 跑挂了：{result.stderr.strip()}")
        # 比 `snippet in text` 更硬：fmt 之后标记之间那一条必须**逐字节**还是原样（含缩进与结尾逗号）。
        return pinned_entry_blocks(src.read_text(encoding="utf-8")) == [snippet]

def do_generate_key(key_id: str) -> int:
    seed = os.urandom(32)
    pk = public_key(seed)
    pk_hex = pk.hex()
    if not verify(pk, b"self-check", sign(seed, b"self-check")):
        print("error: 自己签自己验不过——不要用这把钥", file=sys.stderr)
        return 1
    secret = json.dumps({"key_id": key_id, "seed_hex": seed.hex()}, ensure_ascii=False)
    print("# ① 私钥（**只**贴进仓库 Secret " + KEY_ENV + "，别写进任何文件/日志/提交）：")
    print(secret)
    print("# ② 公钥（整段贴进 crates/core/src/update/trust.rs 的 PINNED-KEYS 标记之间；")
    print("#    形状就是 `cargo fmt` 会保持的那一种 —— 贴完直接提交，不用再跑 fmt）：")
    print(render_pinned_key(key_id, pk_hex))
    print(f"# 公钥十六进制：{pk_hex}")
    return 0

def do_sign(manifest: Path, key_path: str | None, out: Path | None, key_id_override: str | None) -> int:
    if not manifest.is_file():
        print(f"error: 没有清单文件 {manifest}", file=sys.stderr)
        return 1
    raw = manifest.read_bytes()
    # 签的必须是**能解析**的清单：签一串垃圾只会把事故推迟到用户机器上。
    try:
        parsed = json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        print(f"error: {manifest} 不是合法 JSON（{error}）", file=sys.stderr)
        return 1
    if not isinstance(parsed, dict) or "channel" not in parsed or "releases" not in parsed:
        print(f"error: {manifest} 不像发布清单（缺 channel/releases）", file=sys.stderr)
        return 1
    key_id, seed = key_from_env_or_arg(key_path)
    if key_id_override:
        key_id = key_id_override
    signature = sign(seed, raw)
    if not verify(public_key(seed), raw, signature):
        print("error: 签出来的签名自己验不过（实现有问题），拒绝落盘", file=sys.stderr)
        return 1
    envelope = {
        "format": FORMAT,
        "signatures": [
            {
                "alg": ALG,
                "key_id": key_id,
                "sig": signature.hex(),
                "manifest_sha256": hashlib.sha256(raw).hexdigest(),
            }
        ],
    }
    out = out or manifest.with_name(manifest.name + SIG_SUFFIX)
    out.write_text(json.dumps(envelope, ensure_ascii=False, indent=2) + "\n", encoding="utf-8", newline="\n")
    print(f"signed: {manifest} → {out}")
    print(f"  key_id          = {key_id}")
    print(f"  public_key      = {public_key(seed).hex()}")
    print(f"  manifest_sha256 = {envelope['signatures'][0]['manifest_sha256']}")
    return 0

def do_verify(manifest: Path, sig: Path, public_key_hex: str) -> int:
    raw = manifest.read_bytes()
    envelope = json.loads(sig.read_text(encoding="utf-8"))
    pk = bytes.fromhex(public_key_hex)
    for item in envelope.get("signatures", []):
        if item.get("alg") != ALG:
            continue
        if verify(pk, raw, bytes.fromhex(item.get("sig", ""))):
            print(f"ok: {sig} 里的签名（key_id={item.get('key_id')}）通过")
            return 0
    print("error: 没有任何一条签名能验过", file=sys.stderr)
    return 1

def do_check_pinned(key_path: str | None) -> int:
    key_id, seed = key_from_env_or_arg(key_path)
    pk_hex = public_key(seed).hex()
    keys = pinned_keys(TRUST_RS.read_text(encoding="utf-8"))
    if not keys:
        # 「说人话的红」：因为什么、缺什么、怎么补，一行一行写清楚（不涉密，只回显公钥）。
        print("error: 信任表是空的（fail-closed）：客户端一把公钥都没有，任何清单都验不过，所以拒绝发布。", file=sys.stderr)
        print(f"error:   本次签名钥 = key_id {key_id} / 公钥 {pk_hex}", file=sys.stderr)
        print("error:   补法：把下面这段贴进 crates/core/src/update/trust.rs 的 PINNED-KEYS 标记之间 ——", file=sys.stderr)
        for line in render_pinned_key(key_id, pk_hex).splitlines():
            print(f"error:   {line}", file=sys.stderr)
        print("error:   手册：packaging/docs/signing.md §3", file=sys.stderr)
        return 1
    for pinned_id, pinned_hex in keys:
        if pinned_hex.lower() == pk_hex:
            print(f"ok: 签名钥 {key_id} 已被钉住（trust.rs 里叫 {pinned_id}）")
            if pinned_id != key_id:
                print(
                    f"warning: 同一个公钥在 trust.rs 里叫 {pinned_id}、密钥文件里叫 {key_id}；"
                    "清单会带后者，客户端按**公钥字节**验签，所以两种写法都能过（但建议改名对齐）",
                    file=sys.stderr,
                )
            return 0
    pinned_ids = [k[0] for k in keys]
    print(
        f"error: 签名钥（key_id={key_id} / 公钥={pk_hex}）不在信任表里 —— 签出来的清单没有任何客户端会认，所以拒绝发布。",
        file=sys.stderr,
    )
    print(f"error:   trust.rs 现在钉的是：{pinned_ids}", file=sys.stderr)
    for pinned_id, pinned_hex in keys:
        print(f"error:     {pinned_id} = {pinned_hex}", file=sys.stderr)
    print("error:   二选一：", file=sys.stderr)
    print("error:   ① 用已钉住的那把钥签（换回对应的 Secret）；或", file=sys.stderr)
    print(
        "error:   ② 换钥：把上面的公钥按「旧钥签新钥」贴进 crates/core/src/update/trust.rs 的 PINNED-KEYS 之间，"
        "并让本次清单同时带旧钥签名",
        file=sys.stderr,
    )
    print("error:   手册：packaging/docs/signing.md §3/§5", file=sys.stderr)
    return 1

def do_verify_pinned(manifest: Path) -> int:
    """用 trust.rs 里钉住的公钥回验 `<manifest>.sig` —— 复现「客户端会怎么判」，不需要私钥。"""
    keys = pinned_keys(TRUST_RS.read_text(encoding="utf-8"))
    if not keys:
        print("error: 信任表是空的，无从回验（先钉公钥，见 --check-pinned-keyring）", file=sys.stderr)
        return 1
    sig_path = manifest.with_name(manifest.name + SIG_SUFFIX)
    if not sig_path.is_file():
        print(f"error: 没有签名文件 {sig_path}", file=sys.stderr)
        return 1
    raw = manifest.read_bytes()
    envelope = json.loads(sig_path.read_text(encoding="utf-8"))
    for item in envelope.get("signatures", []):
        if item.get("alg") != ALG:
            continue
        try:
            signature = bytes.fromhex(item.get("sig", ""))
        except ValueError:
            continue
        for pinned_id, pinned_hex in keys:
            if verify(bytes.fromhex(pinned_hex), raw, signature):
                print(f"ok: {sig_path.name} 用钉住的公钥 {pinned_id} 能验过（客户端会接受）")
                return 0
    print(
        "error: 拿 trust.rs 里钉住的公钥回验不过 —— 这份 Release 的用户会全部拒装。别发版",
        file=sys.stderr,
    )
    return 1


def do_self_test() -> int:
    vectors = [
        ("9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60", "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a", "",
         "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b"),
        ("4ccd089b28ff96da9db6c346ec114e0f5b8a319f35aba624da8cf6ed4fb8a6fb", "3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c", "72",
         "92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00"),
        ("c5aa8df43f9f837bedb7442f31dcb7b166d38535076f094b85ce3a2e0b4458f7", "fc51cd8e6218a1a38da47ed00230f0580816ed13ba3303ac5deb911548908025", "af82",
         "6291d657deec24024827e69c3abe01a30ce548a284743a445e3680d7db5ac3ac18ff9b538d16f290ae67f760984dc6594a7c15e9716ed28dc027beceea1ec40a"),
    ]
    failures = 0

    def fail(message: str) -> None:
        nonlocal failures
        print(f"FAIL: {message}", file=sys.stderr)
        failures += 1

    for index, (seed_hex, pk_hex, message_hex, sig_hex) in enumerate(vectors, 1):
        seed, pk, message = bytes.fromhex(seed_hex), bytes.fromhex(pk_hex), bytes.fromhex(message_hex)
        if public_key(seed).hex() != pk_hex:
            fail(f"vector {index}: public key mismatch")
        if sign(seed, message).hex() != sig_hex:
            fail(f"vector {index}: signature mismatch")
        if not verify(pk, message, bytes.fromhex(sig_hex)):
            fail(f"vector {index}: verify rejected")
        tampered = bytearray(message)
        tampered.append(0)
        if verify(pk, bytes(tampered), bytes.fromhex(sig_hex)):
            fail(f"vector {index}: tampered message still verifies")
    # 标记解析：信任表解析器必须在示例上命中一对，且在空表上返回空。
    sample = f'{PIN_BEGIN}\n    PinnedKey {{ key_id: "k1", public_key_hex: "{"ab" * 32}" }},\n{PIN_END}'
    if pinned_keys(sample) != [("k1", "ab" * 32)]:
        fail("pinned_keys() 没解析出示例条目")
    # 多行形态（M261）：`trust.rs` 里那一条是 `cargo fmt` 生成的，字段一行一个 —— 靠 `KID_RE` 的 `\s*`
    # 跨行命中。若解析器退化成按行匹配，一次 fmt 就会把整张信任表**静默**读成空表（= 全量客户端验签失败）。
    multiline = f'{PIN_BEGIN}\n    PinnedKey {{\n        key_id: "k1",\n        public_key_hex: "{"ab" * 32}",\n    }},\n{PIN_END}'
    if pinned_keys(multiline) != [("k1", "ab" * 32)]:
        fail("pinned_keys() 没解析出多行（rustfmt）形态的条目")
    if pinned_keys(f"{PIN_BEGIN}\n{PIN_END}") != []:
        fail("pinned_keys() 在空表上应返回空")
    # 粘贴块形态（M265）：`--generate-key` 与「信任表是空的」报错里给人贴的那段 `PinnedKey` 必须是
    # rustfmt **不会改形**的多行形态。单行形态是合法 Rust、也照样能解析，所以这条路一旦回退成单行，
    # 除了「谁忘了跑 fmt」之外没有任何东西会红 —— T30 的 fmt 红就是这么进的 main。
    hex64 = "ab" * 32
    rendered = render_pinned_key("k1", hex64)
    want = f'    PinnedKey {{\n        key_id: "k1",\n        public_key_hex: "{hex64}",\n    }},'
    if rendered != want:
        fail(f"粘贴块不是多行形态：{rendered!r}")
    # 为什么 rustfmt 必然把它排成这样：单行（哪怕 `key_id` 为空）就超出 `max_width`，而多行每行都不超。
    width = rustfmt_max_width()
    one_line = f'    PinnedKey {{ key_id: "", public_key_hex: "{hex64}" }},'
    if len(one_line) <= width or any(len(line) > width for line in rendered.splitlines()):
        fail(f"粘贴块与 max_width={width} 的关系变了（单行 {len(one_line)} 字符应当超宽）")
    # 判据的核心：形状 = `trust.rs` 里**现存**条目的排班（字段值抹平后逐行比对）。
    shape = lambda text: [re.sub(r'"[^"]*"', '""', line) for line in text.splitlines()]
    blocks = pinned_entry_blocks(TRUST_RS.read_text(encoding="utf-8") if TRUST_RS.is_file() else "")
    if not blocks:
        print("note: trust.rs 标记之间没有条目，跳过「与现存排版一致」那条", file=sys.stderr)
    for block in blocks:
        if shape(block) != shape(rendered):
            fail(f"粘贴块排版与 trust.rs 现存条目不一致：{block!r}")
    if pinned_keys(f"{PIN_BEGIN}\n{rendered}\n{PIN_END}") != [("k1", hex64)]:
        fail("粘贴块贴进标记之间后解析不出来（贴了等于没贴）")
    keeps = rustfmt_keeps_shape(rendered)
    if keeps is None:
        print("note: PATH 里没有 rustfmt，跳过「真跑一遍 fmt 不改形」那条", file=sys.stderr)
    elif not keeps:
        fail("真跑一遍 rustfmt 改了粘贴块的形态")
    print("self-test:", "ok" if failures == 0 else f"{failures} 处失败")
    return 0 if failures == 0 else 1

def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="更新清单的 Ed25519 签名/验签与信任表检查（M190）")
    parser.add_argument("--generate-key", action="store_true", help="生成一把新钥（打印 Secret 与 PinnedKey 片段）")
    parser.add_argument("--key-id", default="ull-release", help="生成/签名时用的 key_id")
    parser.add_argument("--sign", metavar="MANIFEST", type=Path, help="签这份清单（产出 <清单>.sig）")
    parser.add_argument("--out", metavar="SIG", type=Path, help="签名输出路径（默认 <清单>.sig）")
    parser.add_argument("--key", metavar="FILE", help=f"密钥文件（不给则读环境变量 {KEY_ENV}）")
    parser.add_argument("--verify", nargs=2, metavar=("MANIFEST", "SIG"), help="手工验一份签名")
    parser.add_argument("--public-key", metavar="HEX", help="--verify 用的公钥")
    parser.add_argument("--check-pinned-keyring", action="store_true", help="检查密钥是否已被钉进 trust.rs")
    parser.add_argument("--verify-pinned", metavar="MANIFEST", type=Path, help="用 trust.rs 里钉的公钥回验产出")
    parser.add_argument("--self-test", action="store_true", help="跑 RFC 8032 向量与解析器自检")
    args = parser.parse_args(argv)

    if args.self_test:
        return do_self_test()
    if args.generate_key:
        return do_generate_key(args.key_id)
    if args.sign:
        return do_sign(args.sign, args.key, args.out, args.key_id if args.key_id != "ull-release" else None)
    if args.verify:
        if not args.public_key:
            print("error: --verify 需要 --public-key", file=sys.stderr)
            return 1
        return do_verify(Path(args.verify[0]), Path(args.verify[1]), args.public_key)
    if args.check_pinned_keyring:
        return do_check_pinned(args.key)
    if args.verify_pinned:
        return do_verify_pinned(args.verify_pinned)
    parser.print_help()
    return 2

if __name__ == "__main__":
    raise SystemExit(main())
