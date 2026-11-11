# 产物签名与更新验签（M190）

本文件是**发布侧手册**：私钥从哪来、怎么用、怎么轮换、哪一步失败必须让发布变红。
用户面的说明（README 里「为什么第一个包要手动下」那类话）不在本文件范围。

> **M255 补充：要照做的那几步在 [`signing-runbook.md`](signing-runbook.md)** —— 生成钥 → 私钥进 GitHub
> Secret（名字逐字是 `ULL_MANIFEST_SIGNING_KEY`）→ `PinnedKey` 贴进 `trust.rs` 标记之间 → 发一次带 `.sig`
> 的 Release → 用 `--verify-pinned` 回验 → 已知坑。本文件讲**为什么**，那份讲**怎么做**。
> 现状读数用 `python3 packaging/check_signing_ready.py`（三行 + `0/1/2`；未就绪时会打印一行修复命令）；
> CI 里同一条读数挂在 `.github/workflows/signing-status.yml` 的 job summary 上 —— **那是播报，不是门禁**。

## 0. 先说清这件事**不**解决什么

| 有 / 没有 | 状态 |
|---|---|
| 更新器验「清单是不是我们发的」 | **有**（Ed25519 清单签名 + 二进制里钉住的公钥） |
| Windows 上「未知发布者」提示消失 | **没有**。项目自有签名与 Authenticode 是两件事 |
| SmartScreen 信誉 | **没有**。只有买了 OV/EV 证书并真签名才可能逐步积累 |

> 一句话：**项目自有签名保护的是自更新这条链，不是 Windows 的信任判断。** 用户双击 exe 时看到的
> 「未知发布者」来自 SmartScreen，与本文件的方案无关；买证书是另一条线（见 M190 报告里的
> 「后续要用户拍板的两件事」）。

## 1. 威胁模型（为什么要签名）

清单里的 `sha256` 只保证「包体与清单对得上」。**能改 Release 的人可以同时改包和摘要**，所以摘要
本身不是信任根。签名的对象因此是**清单文件的原始字节**：改了任意一个字段（哪怕只改版本号里一个数字），
签名立刻对不上。

```
发布侧                                   用户机器
------                                   --------
manifest.json（字节）──签名──> manifest.json.sig
       │                                   │
       └── 挂到同一个 Release ─────────────┤
                                           ├─ 取清单 + .sig
                                           ├─ 用**编译进二进制**的公钥验签（trust.rs）
                                           ├─ 过了 → 再解析、再比 sha256、再装
                                           └─ 不过 → 五类签名错误，**什么都不会被安装**
```

## 2. 信任根在哪：`crates/core/src/update/trust.rs`

* 公钥是**编译期常量数组**（`PRODUCTION`），不是用户机器上的文件 —— 没有环境变量、没有设置键、
  没有「清单旁边的 keyring.json」这类运行期口子（`tests/signature_cases.rs::no_runtime_keyring_override`
  守着这条）。
* 是**数组**而不是一把：换签名方时新旧钥可以并存（见 §5）。
* `key_id` 只是查表标签，**判据永远是公钥字节**：把 `key_id` 伪造成钉住的那个也过不了
  （`key_id_spoofing_does_not_help`）。

### 空表 = fail-closed

正式表当前**是空的**：私钥只能来自仓库 Secret，任何代码仓库都不该自己生成并落盘私钥。
空表的效果是「任何正式构建都拒装任何清单」，同时 `--check-pinned-keyring` 让发布流水线当场红。
**这是有意的**：宁可不出 Release，也不出未签名的 Release。

## 3. 一次性录入（只做一次）

```bash
# ① 生成：打印一段 Secret JSON + 一行可以直接贴进 trust.rs 的 PinnedKey
python3 packaging/sign_manifest.py --generate-key --key-id ull-release-2026a
```

* 打印出来的 **第一行 JSON**（含 `seed_hex`）= 私钥，**只**贴进仓库 Secret：
  Settings → Secrets and variables → Actions → New repository secret，名字
  **`ULL_MANIFEST_SIGNING_KEY`**。
* 打印出来的 **`PinnedKey { … }` 那一行**贴进 `crates/core/src/update/trust.rs` 的
  `>>> PINNED-KEYS >>>` 与 `<<< PINNED-KEYS <<<` 标记之间（标记本身不要动，脚本按它解析）。
* 提交这次改动（公钥进 git 是对的：它是信任锚，本来就该公开可审计）。

### 私钥纪律（违反任何一条都算事故）

1. **只**存在于仓库 Secret `ULL_MANIFEST_SIGNING_KEY`。本地需要签测试产物时，走 `--key <文件>`
   并把那个文件放在**仓库外**（gitignore 不保险，别放仓库里）。
2. **不得**出现在日志、产物、提交、issue、聊天记录里。CI 里任何 `echo`/`cat`/`set -x` 都不许碰到它
   （工作流里的签名步骤显式 `set +x` 且只回显**公钥**）。
3. 一旦怀疑泄露：立即按 §5 轮换，并在下一个 Release 的说明里点名旧 `key_id` 作废。
4. 私钥文件/Secret 的备份不归仓库管；**丢了 = 旧钥作废**，用户机器上的旧二进制只能靠
   「先手动装一次新版本」恢复（`trust.rs` 里的旧条目那时才可以删）。

## 4. 发布时发生什么（`.github/workflows/release-publish.yml`）

在 `manifest.json` 生成之后、`gh release create/upload` 之前，多了一步：

```bash
python3 packaging/sign_manifest.py --self-test                     # ① RFC 8032 向量自检
python3 packaging/sign_manifest.py --check-pinned-keyring          # ② 密钥 ↔ trust.rs 必须对得上
python3 packaging/sign_manifest.py --sign dist/manifest.json       # ③ 产出 dist/manifest.json.sig
python3 packaging/sign_manifest.py --verify-pinned dist/manifest.json  # ④ 用钉住的公钥回验产出
```

第 ④ 步复现「客户端会怎么判」：它**不碰私钥**，只拿 `trust.rs` 里那把公钥去验刚落盘的 `.sig`。
因此「签名脚本自己说签好了」不构成证据，证据是「按客户端的判据能验过」。

四条命令任何一条非零退出 ⇒ **这一步红 ⇒ 整个 Release job 红 ⇒ 本次不出 Release**。常见红法：

| 现象 | 原因 | 怎么补 |
|---|---|---|
| `没有签名密钥` | Secret `ULL_MANIFEST_SIGNING_KEY` 没配 / 名字拼错 | 按 §3 录入 |
| `信任表是空的` | `trust.rs` 里还没钉任何公钥 | 把 §3 打印的 `PinnedKey` 那行贴进标记之间 |
| `签名钥（…）不在信任表里` | Secret 与 `trust.rs` 是**两把不同的钥** | 用同一把钥重跑 §3，或回滚 `trust.rs` 的改动 |
| `签出来的签名自己验不过` | 实现/环境异常 | 不要发布；把 `--self-test` 输出贴进 issue |

`dist/manifest.json.sig` 与 `dist/manifest.json` 一起作为 Release 资产上传（`assets=(...)` 那一行）。

## 5. 轮换（换钥 / 旧钥泄露 / 接托管签名）

**纪律：旧钥签新钥。** 不允许在没有旧钥签名的情况下替换信任表 —— 那等于让「能改仓库的人」变成
「能改信任根的人」。

1. 生成新钥（§3），得到 `key_id = ull-release-2027a`。
2. `trust.rs` 里**保留**旧条目、**追加**新条目（数组顺序无关；`key_id` 不能重名）。
3. 在一次发布里，让清单**同时带两把钥的签名**（两条 `signatures`，客户端「任意一条通过即放行」）：
   用旧钥签一次、新钥签一次，把两个信封的 `signatures` 数组拼起来放进 `manifest.json.sig`。
   这一步让「装着旧二进制的用户」和「装着新二进制的用户」都过得去。
4. 之后至少**一个完整发布周期**保持双签名，再观察下载/更新失败率。
5. 确认新钥已被广泛接受后，用新钥单独签发下一次清单，并从 `trust.rs` 删掉旧条目
   （删条目的那个版本必须**只**由新钥签）。
6. changelog 里点名：新增/作废的 `key_id` 与公钥指纹。

托管签名（例如 SignPath Foundation）作为**后续可替换路径**：它的产物签名由 CI 侧调用其服务完成，
`trust.rs` 的数组形状不用变（多一条 `PinnedKey` 即可）；清单签名这一层保持本项目自有钥不动。
**不允许**为了接托管签名把判据改成「只认证书」。

## 6. 三条守门（都能单独变红）

| 守门 | 脚本 / 用例 | 变红意味着 |
|---|---|---|
| 版本资源真的在产物里 | `packaging/check_pe_version.py <exe> --expect-version <版本>` | 读回产物字节找不到 `VS_VERSIONINFO` / 字段值不对（构建日志绿也没用） |
| 篡改清单 ⇒ 验签失败且不安装 | `cargo test -p unlucky-core --lib update::tests::signature_cases` | 改字段/只改签名/漏签名/陌生钥 任一仍能装 |
| 签名失败阻断发布 | `release-publish.yml` 的签名步 + `--check-pinned-keyring` | 没钥/钥没钉/钥不匹配时仍发出 Release |

另外 CI 的构建槽位会跑 `check_pe_version.py`（`release-build.yml`）：Windows 槽位上直接读
`target/release/unlucky-launcher-iced.exe` 与 `ull.exe` 的字节，断言 `CompanyName` 等字符串在。

## 7. 实现分工（M190 r1 复审后）

| 哪一半 | 在哪 | 用什么 |
|---|---|---|
| **验签**（进二进制） | `crates/core/src/update/ed25519.rs` | `ed25519-dalek::verify_strict`（在非 strict 的基础上额外拒**小阶公钥**与**小阶 `R`**；`S` 规范性是非 strict 的 `verify` 也查的） |
| **签名**（只在发布侧） | `packaging/ed25519_ref.py` + `packaging/sign_manifest.py` | 纯 Python 参考实现，零第三方依赖（CI 不装 pip 包） |

两套实现互相独立，`--self-test` 用 RFC 8032 §7.1 的同一组向量把两边各自钉住。**第一版曾把验签写成
仓内自写**（5x51 素域 + 扩展坐标）；r1 复审裁定不安全——信任锚的微妙错误后果是「验签通过但本不该通过」，
因此整段删除，改为官方实现。`crates/core/src/update/tests/ed25519_cases.rs` 现在钉的是
**dalek 的 strict 语义**：谁把 `verify_strict` 换成 `verify`，`small_order_public_key_is_rejected` 与
`small_order_r_is_rejected_only_by_strict` 会红（这是 r2 实测的结论——其余几条拒绝类用例在非 strict
下仍绿，别把它们当 strict 的守卫）。

## 8. 本地自测（不需要私钥）

```bash
python3 packaging/sign_manifest.py --self-test          # RFC 8032 向量 + 标记解析
python3 packaging/check_pe_version.py --self-test       # 口径表与 pe_version.rs 对拍
cargo test -p unlucky-core --lib update::tests::signature_cases
cargo test -p unlucky-core --lib update::tests::ed25519_cases
```

带 `update-test-source` 的验收构建里另有一把**公开的测试钥**（种子写在测试夹具里）：它只用于
「本地目录源 + 真二进制」的端到端验收，**不在**正式信任表里，签不出任何正式版本能装的清单。
