#!/usr/bin/env python3
"""Generate `manifest.json` — the update manifest the launcher reads (D50).

Contract and rationale live in `packaging/README.md` §13; this docstring only states what the
code must not get wrong.

* **Shape authority** = `crates/core/src/update/manifest.rs` (`ReleaseManifest`,
  `deny_unknown_fields` + per-field validation). This script only writes that shape.
* **Facts come from one place**: every `sha256` is read out of the Release's own
  `SHA256SUMS.txt`, every `size` is `stat()` of the same file — the two can never disagree.
  The version token inside each asset name is asserted to equal `--version`, so a tag that
  drifted from `Cargo.toml` cannot yield a manifest pointing at files that do not exist.
* **Only launcher artifacts go in.** `ull` does not: the DTO has no role field and
  `Release::artifact_for(target)` returns the *first* match, so listing the CLI beside the GUI
  would make "which file do I overwrite" depend on array order.
* **`url`**: bare asset name by default (the `DirSource` convention in
  `crates/core/src/update/source.rs`); with `--asset-base-url` a tag-pinned absolute URL,
  which is what CI uses.

Usage:
    python3 packaging/make_manifest.py --dist dist --version v0.2.2 --channel stable \\
        --asset-base-url https://github.com/o/r/releases/download/v0.2.2
    python3 packaging/make_manifest.py --self-test        # golden + strict-parse self check
"""

from __future__ import annotations

import argparse
import copy
import json
import re
import sys
from pathlib import Path
from typing import NoReturn

# 本机 Python 可能是 isolated 模式（`safe_path=True`，脚本目录不进 `sys.path`），CI 的标准
# python3 则会自动加入。显式补一次，两种环境下 `import make_artifacts` 都成立。
sys.path.insert(0, str(Path(__file__).resolve().parent))

# 只借常量：产物命名（SLOTS / LAUNCHER_ASSET）、CLI 名与 SHA256SUMS 文件名都是 make_artifacts
# 的单一口径，这里不复制一份表（复制就会漂）；反向依赖不存在（make_artifacts 只用子进程调本脚本）。
import make_artifacts

MANIFEST_NAME = "manifest.json"
CHANNELS = ("stable", "beta", "alpha", "nightly")
# 文件名里的平台段 -> 清单 target 的系统段（`windows_x64` -> `win-x64`）。
PLATFORM_TARGET = {"windows": "win", "macos": "mac", "linux": "linux"}
# 产物扩展名 -> 清单 format。归档（tar.gz）在 DTO 里只有「解压即用」这一档 `portable-zip`：
# 名字带 zip，语义是「非安装器、可原地替换」（PackageFormat::needs_installer() == false）。
EXT_FORMAT = {".exe": "portable-exe", ".tar.gz": "portable-zip"}
GOLDEN = Path(__file__).resolve().parent / "tests" / "manifest.golden.json"
# DTO 侧的取值表镜像（crates/core/src/update/target.rs::parse_os / Arch::parse / PackageFormat::parse）。
TARGET_OS = {"win", "windows", "mac", "macos", "osx", "darwin", "linux"}
TARGET_ARCH = {
    "x64", "x86_64", "x86-64", "amd64", "arm64", "aarch64", "arm64e", "x86", "i686", "i386", "x86_32",
}
FORMATS = {"msi", "dmg", "appimage", "deb", "rpm", "portable-zip", "zip", "portable-exe", "exe"}
SEMVER = re.compile(r"^v?(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?(?:\+([0-9A-Za-z.-]+))?$")
SUMS_LINE = re.compile(r"^([0-9A-Fa-f]{64})[ \t]+\*?(.+)$")


def fail(message: str) -> NoReturn:
    print(f"error: {message}", file=sys.stderr)
    raise SystemExit(1)


def normalize_version(text: str) -> str:
    """`v0.2.2` -> `0.2.2`（tag 带 `v`，产物名与清单都不带）。"""
    version = text.strip()
    version = version[1:] if version.startswith("v") else version
    if not semver_ok(version):
        fail(f"version `{text}` is not a semver")
    return version


def semver_ok(text: str) -> bool:
    """Mirror of `crates/core/src/update/version.rs::Version::parse` on the accepted shapes."""
    match = SEMVER.match(text)
    if not match:
        return False
    if any(len(number) > 1 and number.startswith("0") for number in match.group(1, 2, 3)):
        return False
    pre, build = match.group(4), match.group(5)
    if pre is not None:
        for ident in pre.split("."):
            if not ident or (ident.isdigit() and len(ident) > 1 and ident.startswith("0")):
                return False
    if build is not None and any(not ident for ident in build.split(".")):
        return False
    return True


def read_sums(dist: Path) -> dict[str, str]:
    """`SHA256SUMS.txt` -> {file name: lowercase sha256}. Missing/odd entries are fatal."""
    sums_path = dist / make_artifacts.SUMS_NAME
    if not sums_path.is_file():
        fail(f"missing {sums_path}（清单的 sha256 只从这里取，不另算一份）")
    sums: dict[str, str] = {}
    for lineno, line in enumerate(sums_path.read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip():
            continue
        match = SUMS_LINE.match(line)
        if not match:
            fail(f"{sums_path.name}:{lineno} 不是 `<sha256>  <文件名>` 形态：{line!r}")
        digest, name = match.group(1).lower(), match.group(2)
        if name in sums:
            fail(f"{sums_path.name} 里 {name} 出现两次（无法确定哪个 sha256 是对的）")
        sums[name] = digest
    if not sums:
        fail(f"{sums_path} 是空的：没有产物就没有清单")
    return sums


def split_asset(name: str, version: str) -> tuple[str, str, str, str]:
    """`<name>_<version>_<platform>_<arch>[.exe|.tar.gz]` -> (name, platform, arch, ext)."""
    stem, ext = name, ""
    for candidate in EXT_FORMAT:
        if stem.endswith(candidate):
            stem, ext = stem[: -len(candidate)], candidate
            break
    parts = stem.rsplit("_", 3)
    if len(parts) != 4:
        fail(f"`{name}` 不符合 `<名字>_<版本>_<平台>_<架构>` 命名（见 make_artifacts.asset_stem）")
    product, tag_version, platform_name, arch_name = parts
    if tag_version != version:
        fail(f"`{name}` 里的版本是 {tag_version}，清单要的是 {version}（tag 与 Cargo.toml 漂了？）")
    if (platform_name, arch_name) not in {
        (slot_os, slot_arch) for slot_os, slot_arch, _kind in make_artifacts.SLOTS.values()
    }:
        fail(f"`{name}` 的平台/架构 {platform_name}/{arch_name} 不在发布矩阵里")
    return product, platform_name, arch_name, ext


def build_artifacts(dist: Path, version: str, base_url: str) -> list[dict[str, object]]:
    """Launcher artifacts from `SHA256SUMS.txt`, in matrix order (= priority order)."""
    sums = read_sums(dist)
    by_slot: dict[tuple[str, str], dict[str, object]] = {}
    for name, digest in sums.items():
        product, platform_name, arch_name, ext = split_asset(name, version)
        if product != make_artifacts.LAUNCHER_ASSET:
            continue  # `ull` 不进清单，理由见模块注释
        fmt = EXT_FORMAT.get(ext)
        if fmt is None:
            fail(f"`{name}` 没有可识别的扩展名，无法判定包形态（format）")
        path = dist / name
        if not path.is_file():
            fail(f"{make_artifacts.SUMS_NAME} 提到 {name}，但 {path} 不存在")
        size = path.stat().st_size
        if size == 0:
            fail(f"{path} 是 0 字节：这类产物只可能是打包漏了")
        url = f"{base_url.rstrip('/')}/{name}" if base_url else name
        if (platform_name, arch_name) in by_slot:
            fail(f"{platform_name}/{arch_name} 出现两个启动器产物，清单无法确定该用哪一个")
        by_slot[(platform_name, arch_name)] = {
            "target": f"{PLATFORM_TARGET[platform_name]}-{arch_name}",
            "format": fmt,
            "url": url,
            "size": size,
            "sha256": digest,
        }
    order = [(slot_os, slot_arch) for slot_os, slot_arch, _kind in make_artifacts.SLOTS.values()]
    artifacts_out = [by_slot[key] for key in order if key in by_slot]
    if not artifacts_out:
        fail(f"{make_artifacts.SUMS_NAME} 里没有任何 `{make_artifacts.LAUNCHER_ASSET}_*` 产物，清单会是空的")
    return artifacts_out


def build_manifest(
    dist: Path,
    version: str,
    channel: str,
    *,
    base_url: str = "",
    min_supported: str | None = None,
    notes: str | None = None,
    released_at: str | None = None,
    force: bool = False,
) -> dict[str, object]:
    """Assemble the manifest document (the shape `ReleaseManifest` deserializes)."""
    if channel not in CHANNELS:
        fail(f"未知渠道 `{channel}`（只认 {' / '.join(CHANNELS)}）")
    release: dict[str, object] = {"version": version, "channel": channel}
    if released_at:
        release["released_at"] = released_at
    if notes:
        release["notes"] = notes
    release["force"] = force
    if min_supported:
        release["min_supported"] = normalize_version(min_supported)
    release["artifacts"] = build_artifacts(dist, version, base_url)
    return {"channel": channel, "releases": [release]}


def render(doc: dict[str, object]) -> str:
    """Deterministic JSON: same facts + same args ⇒ byte-identical file (golden-testable)."""
    return json.dumps(doc, ensure_ascii=False, indent=2) + "\n"


# ── 严格解析镜像（DTO 的看门规则）───────────────────────────────────────────────────────
# `manifest.rs` 用 `deny_unknown_fields` + 逐字段校验；这里是它的 Python 镜像，只为了在没有
# Rust 的机器上（本地打包、CI 发布步）也能当场炸掉拼错的字段。字段/取值的权威永远是 .rs。


def check(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def check_keys(where: str, obj: object, allowed: set[str], required: set[str]) -> dict:
    check(isinstance(obj, dict), f"{where} 必须是对象")
    unknown = set(obj) - allowed
    check(not unknown, f"{where} 出现未知字段 {sorted(unknown)}（DTO 是 deny_unknown_fields）")
    missing = required - set(obj)
    check(not missing, f"{where} 缺字段 {sorted(missing)}")
    return obj


def strict_parse(doc: object) -> None:
    """Raise `ValueError` on anything `ReleaseManifest::from_json` would reject."""
    top = check_keys("清单", doc, {"channel", "releases"}, {"channel"})
    check(top["channel"] in CHANNELS, f"清单渠道 `{top['channel']}` 无法识别")
    releases = top.get("releases", [])
    check(isinstance(releases, list), "releases 必须是数组")
    seen: list[str] = []
    for index, raw in enumerate(releases):
        where = f"releases[{index}]"
        release = check_keys(
            where,
            raw,
            {"version", "channel", "released_at", "notes", "force", "min_supported", "artifacts"},
            {"version"},
        )
        version = release["version"]
        check(isinstance(version, str) and semver_ok(version), f"{where}.version 不是合法 semver")
        check(version not in seen, f"{where}.version {version} 在清单里重复出现")
        seen.append(version)
        if "channel" in release:
            check(release["channel"] in CHANNELS, f"{where}.channel 无法识别")
        for key in ("released_at", "notes"):
            check(release.get(key) is None or isinstance(release.get(key), str), f"{where}.{key} 必须是字符串")
        check(isinstance(release.get("force", False), bool), f"{where}.force 必须是布尔")
        if release.get("min_supported") is not None:
            check(semver_ok(str(release["min_supported"])), f"{where}.min_supported 不是合法 semver")
        artifacts_list = release.get("artifacts", [])
        check(isinstance(artifacts_list, list), f"{where}.artifacts 必须是数组")
        for art_index, raw_art in enumerate(artifacts_list):
            art_where = f"{where}.artifacts[{art_index}]"
            art = check_keys(
                art_where,
                raw_art,
                {"target", "format", "url", "size", "sha256", "sha1"},
                {"target", "format", "url", "size"},
            )
            os_part, _, arch_part = str(art["target"]).strip().partition("-")
            check(
                bool(arch_part) and os_part.lower() in TARGET_OS and arch_part.lower() in TARGET_ARCH,
                f"{art_where}.target `{art['target']}` 无法识别（形如 win-x64）",
            )
            check(str(art["format"]).lower() in FORMATS, f"{art_where}.format 无法识别")
            check(isinstance(art["url"], str) and art["url"].strip(), f"{art_where}.url 为空")
            size = art["size"]
            check(isinstance(size, int) and not isinstance(size, bool) and size > 0, f"{art_where}.size 必须 > 0")
            sha256, sha1 = art.get("sha256"), art.get("sha1")
            check(sha256 is not None or sha1 is not None, f"{art_where} 既没有 sha256 也没有 sha1")
            digest_hex = str(sha256 if sha256 is not None else sha1)
            expect = 64 if sha256 is not None else 40
            check(
                len(digest_hex) == expect and all(c in "0123456789abcdefABCDEF" for c in digest_hex),
                f"{art_where} 的摘要应为 {expect} 位十六进制",
            )


def self_test() -> int:
    """Golden 对拍（`.gitattributes` 的 `eol=lf` 保证跨平台同字节）+ 严格解析正负例：证明写出来的东西能被 DTO 吃下。"""
    import tempfile

    with tempfile.TemporaryDirectory() as tmp:
        dist = Path(tmp)
        version = "9.9.9"
        # 两种启动器形态（裸 exe / 归档）+ 一个 CLI 资产：既覆盖 target/format 映射，
        # 也钉住「`ull` 不进清单」这条决定。
        payloads = {
            f"{make_artifacts.LAUNCHER_ASSET}_{version}_windows_x64.exe": b"gui payload\n",
            f"{make_artifacts.LAUNCHER_ASSET}_{version}_linux_x64.tar.gz": b"tar payload\n",
            f"{make_artifacts.CLI_BIN}_{version}_windows_x64.exe": b"cli payload\n",
        }
        for name, payload in payloads.items():
            (dist / name).write_bytes(payload)
        (dist / make_artifacts.SUMS_NAME).write_text(
            "".join(f"{make_artifacts.sha256(dist / name)}  {name}\n" for name in payloads),
            encoding="utf-8",
            newline="\n",
        )
        doc = build_manifest(
            dist,
            version,
            "stable",
            min_supported="9.0.0",
            notes="golden 样例（自验用，不是真实发布）",
            released_at="2026-01-02T03:04:05Z",
        )
        text = render(doc)

    if "--write-golden" in sys.argv:
        GOLDEN.parent.mkdir(parents=True, exist_ok=True)
        GOLDEN.write_text(text, encoding="utf-8", newline="\n")
        print(f"golden:  {GOLDEN} 已重写")
    expected = GOLDEN.read_text(encoding="utf-8")
    if text != expected:
        print("--- generated ---", text, "--- golden ---", expected, sep="\n", file=sys.stderr)
        fail("生成的清单与 golden 不一致（改了口径就 --write-golden 重出并对 diff）")
    print(f"golden:   与 {GOLDEN.name} 逐字节一致")

    strict_parse(doc)
    print("strict:   生成物通过严格解析镜像（deny_unknown_fields + 逐字段校验）")
    # 负例：只测「顺利通过」等于没测——每条篡改都必须被拒。
    for label, mutate in (
        ("未知字段", lambda d: d.update(extra=1)),
        ("未知版本字段", lambda d: d["releases"][0].update(extra=1)),
        ("未知产物字段", lambda d: d["releases"][0]["artifacts"][0].update(extra=1)),
        ("缺 sha256", lambda d: d["releases"][0]["artifacts"][0].pop("sha256")),
        ("摘要长度错", lambda d: d["releases"][0]["artifacts"][0].update(sha256="deadbeef")),
        ("size 为 0", lambda d: d["releases"][0]["artifacts"][0].update(size=0)),
        ("size 是字符串", lambda d: d["releases"][0]["artifacts"][0].update(size="12")),
        ("未知 target", lambda d: d["releases"][0]["artifacts"][0].update(target="solaris-x64")),
        ("未知 format", lambda d: d["releases"][0]["artifacts"][0].update(format="msix")),
        ("空 url", lambda d: d["releases"][0]["artifacts"][0].update(url="  ")),
        ("版本重复", lambda d: d["releases"].append(copy.deepcopy(d["releases"][0]))),
        ("版本号不合法", lambda d: d["releases"][0].update(version="1.0")),
        ("渠道不认识", lambda d: d.update(channel="canary")),
        ("缺顶层 channel", lambda d: d.pop("channel")),
    ):
        tampered = copy.deepcopy(doc)
        mutate(tampered)
        try:
            strict_parse(tampered)
        except ValueError:
            print(f"strict:   负例「{label}」被正确拒绝")
            continue
        fail(f"严格解析放过了负例「{label}」——这条校验形同虚设")
    print("self-test: ok")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--dist", default="dist", help="directory holding the assets + SHA256SUMS.txt (default: dist/)")
    parser.add_argument("--version", help="release version, e.g. v0.2.2 (must match the asset file names)")
    parser.add_argument("--channel", default="stable", choices=CHANNELS, help="manifest channel (D50: stable only for now)")
    parser.add_argument("--out", help=f"output path (default: <dist>/{MANIFEST_NAME})")
    parser.add_argument("--asset-base-url", default="", help="prefix for artifact urls; omit for bare file names")
    parser.add_argument("--min-supported", help="lowest version still supported (omitted when not given)")
    parser.add_argument("--notes", help="plain-text release notes for the update prompt")
    parser.add_argument("--notes-file", help="read --notes from this file")
    parser.add_argument("--released-at", help="RFC3339 timestamp (display only)")
    parser.add_argument("--force", action="store_true", help="mark this release as force-update")
    parser.add_argument("--self-test", action="store_true", help="golden + strict-parse self check (no files written)")
    parser.add_argument("--write-golden", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args()

    if args.self_test:
        return self_test()
    if not args.version:
        parser.error("--version is required (it must match the asset file names)")
    dist = Path(args.dist)
    if not dist.is_absolute():
        dist = make_artifacts.REPO_ROOT / dist
    notes = args.notes
    if args.notes_file:
        notes = (make_artifacts.REPO_ROOT / args.notes_file).read_text(encoding="utf-8").strip()
    doc = build_manifest(
        dist,
        normalize_version(args.version),
        args.channel,
        base_url=args.asset_base_url,
        min_supported=args.min_supported,
        notes=notes,
        released_at=args.released_at,
        force=args.force,
    )
    text = render(doc)
    strict_parse(doc)
    out = Path(args.out) if args.out else dist / MANIFEST_NAME
    if not out.is_absolute():
        out = make_artifacts.REPO_ROOT / out
    out.write_text(text, encoding="utf-8", newline="\n")
    release = doc["releases"][0]
    artifacts_list = release["artifacts"]
    print(f"manifest: {out.name}  channel={doc['channel']} version={release['version']} artifacts={len(artifacts_list)}")
    for item in artifacts_list:
        print(f"  {item['target']:<14} {item['format']:<13} {item['size']:>10} B  sha256={str(item['sha256'])[:12]}  {item['url']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
