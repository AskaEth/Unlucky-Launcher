# 自更新的信任锚（`PRODUCTION` 表）到底钉了什么

M308 纠偏。本文只做两件事：**作废一条过时读数**，并把「现在钉着什么、怎么当场地看一眼」写清楚。

## M254 审计的「正式表为空」已作废

M254（CLAIMAUDIT）当时读到 `crates/core/src/update/trust.rs` 的 `PRODUCTION` 表在
`PINNED-KEYS` 标记之间**是空的**，于是断言「任何正式二进制都验不过任何清单」。**那条读数
作废**：commit `68362e5`（`feat(core): 钉入生产签名公钥（key_id ull-release）`）已经往
标记之间钉了一条：

```text
PinnedKey { key_id: "ull-release", public_key_hex: "1c3687ec…" }
```

从那以后，**由 `ull-release` 私钥签过的清单，正式构建会照常验签通过**。M308 之前 `trust.rs`
顶部那段文档还写着「正式表当前是空的（fail-closed）」，与代码直接矛盾——这正是 M307 盘查抓到的
三处文档不一致之一，M308 已改。

「表里有钥」**不**降低 fail-closed 的性质：签名缺失 / 签名文件坏 / `key_id` 不认得 / 签名对不上
（`UpdateErrorKind` 的签名五兄弟）一律拒装，绝不「没钥就放行」。表里那把钥只是把「验得过」这条
路打开，不是把闸门拆掉。

## 现在的表长什么样

| 表 | 内容 | 谁看得见 |
|---|---|---|
| `PRODUCTION` | 1 条：`ull-release` | **所有**构建（含官方发布） |
| `TEST_ONLY` | 1 条：`test-only-key`（种子公开写在 `tests/support.rs`） | 只在 `cfg(test)` / `update-test-source` / `update-test-key` 三档构建里**编进去**；其余构建里它是空表，那串十六进制连字节都不在产物里 |

三档守门脚本：`packaging/check_test_source_off.py` / `check_test_key_off.py`（发布流水线跑）。

## 当场看一眼（只读命令）

```bash
# ① 就绪度播报：第一行就是「标记之间有几条 PinnedKey」，只读、不需要任何私钥
python3 packaging/check_signing_ready.py
#   ② 直接读表（最原始的那一眼）
sed -n '/PINNED-KEYS/,/PINNED-KEYS/p' crates/core/src/update/trust.rs
```

`check_signing_ready.py` 的 ① 那一行是**读表**的正确入口：它按标记解析本文件、数条目并点名
`key_id` 与公钥前缀。它的退出码是**就绪度**（① 表非空 **且** ② 环境里有同源私钥才 exit 0），
不是「表读到了没有」——没配 Secret 的机器上它照样打出 ① 那一行，只是整体 exit 1。

⚠ **别拿 `sign_manifest.py --check-pinned-keyring` 当读表工具**：它要私钥
（`ULL_MANIFEST_SIGNING_KEY` 或 `--key <文件>`），语义是「**Secret 里那把钥**是否已钉住」这条
**发布前门禁**；没钥时它直接 `error: 没有签名密钥…` + exit 1（M308 r1 实测）。想只读看表就用上面两条。

`check_signing_ready.py` 的 ① 那一行的判据是「标记之间有几条 `PinnedKey`」，**不是**「表是不是
空的」——所以哪天有人把标记之间清空，它会立刻喊未就绪（那句文案里的「空表」说的是那一天）。

## 加/换钥的纪律（没变）

只能**旧钥签新钥**：那次发布的清单同时带旧钥签名，changelog 点名新钥的 `key_id` 与指纹，
然后往 `PINNED-KEYS` 标记之间加一条。步骤全文见 `packaging/docs/signing.md`。

## 同一轮纠偏的其它两处（M308）

* `crates/core/src/update/target.rs` 的 `PackageFormat` 文档此前写「portable-zip / appimage /
  portable-exe 才走自替换」，与 `job::self_replaceable`（只认裸 exe）直接矛盾；现在判据是
  **便携 exe + 便携归档**两种，两边逐字一致。
* Linux/macOS 的 `.tar.gz` 以前「查得到更新、按钮画得出来、点下去必失败」；M308 在
  `crates/core/src/update/unpack.rs` 里补了「校验通过之后拆出启动器本体」这一跳，于是那两平台
  的自更新**真的能装**。顺序不变：清单的 `size`/`sha256` 仍然只对**归档字节**做双检。
* 界面「重试」按钮现在按 `retryable` 分流：验签类失败不画重试，改说「这次更新已被拒绝
  （重试没用）」（`Key::UpdateBannerFailedFinal`）。
