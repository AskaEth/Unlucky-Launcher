# 签名就绪度操作手册（M255）

本文件是**人类照做**的手册：从「信任表是空的」走到「发出去的 Release 用客户端判据能验过」。
背景/威胁模型/轮换纪律在 [`signing.md`](signing.md)；本文件只讲**步骤、命令、坑**。

> ⚠ 本文件**不含**任何真实私钥、种子或公钥。所有示例都是 `<尖括号占位符>`。
> `--generate-key` 的输出是**敏感物**：只在你要贴进 Secret 的那一次运行里存在，别存文件、别贴 issue。

## 0. 现状读一遍（2026-11 的 M254 审计结论）

`crates/core/src/update/trust.rs` 的 `PRODUCTION` 表在 `PINNED-KEYS` 标记之间是**空的**。空表 =
`KeyringEmpty` 一律拒装 ⇒ **今天任何正式二进制都验不过任何清单**（fail-closed 成立，但用户端表现为
「更新永远失败」）。已存在的部分（都能用）：

| 环节 | 在哪 | 今天的状态 |
|---|---|---|
| 生成钥 | `sign_manifest.py --generate-key` | 有；打印 Secret JSON + `PinnedKey` 那一段（多行，fmt 保持的形态） |
| 钥 ↔ 信任表对账 | `sign_manifest.py --check-pinned-keyring` | 有；**今天必然红**（表空 ⇒ 「信任表是空的」） |
| 签名 | `sign_manifest.py --sign dist/manifest.json` → `.sig` | 有；**今天必然红**（`ULL_MANIFEST_SIGNING_KEY` 没有，前面先红） |
| 回验 | `sign_manifest.py --verify-pinned dist/manifest.json` | 有；**今天必然红**（表空 ⇒ 「无从回验」） |
| 发布接线 | `release-publish.yml` 的「签更新清单」步 | 有；上面四条任一非零 ⇒ 不出 Release（**这是对的**） |
| 资产上传 | `release-publish.yml` 的 `assets=(… manifest.json manifest.json.sig …)` | 有 |
| **操作步骤与可见性** | 本文件 + `packaging/check_signing_ready.py` + `signing-status.yml` | **本次补上的就是这两样** |

一句话：**工具链不缺，缺的是有人把钥钉进表里。** 那件事只有人类能做（塔不会自己生成生产私钥）。

## 1. 先看读数（不用碰任何钥）

```bash
python3 packaging/check_signing_ready.py          # 三行读数 + 退出码
python3 packaging/check_signing_ready.py --dist dist   # 顺手看一眼本地留档
```

三行分别是 ① `PRODUCTION` 表空不空 ② 环境里有没有签名钥（并顺手对账「这把钥在不在表里」）
③ 最近一次发布的 `.sig`/清单痕迹（取不到就写「未知」——本地没留档 ≠ 上次没签）。

退出码：`0` 就绪 / `1` 未就绪 / `2` 用法错（参数写错、`trust.rs` 读不到、标记丢了）。
**未就绪时会打印一行修复命令**，直接照抄即可。CI 里同一条读数在 `Signing status（播报）` workflow 的
job summary 里（见 §6 —— 它是播报，不是门禁）。

## 2. 人类要亲手做的 5 步（下单清单）

下面每一行都只有命令与占位符，**没有任何密钥值**。做完一步就把该步的验证跑一遍。

> 🔴 **顺序硬要求（M255 r1 P2-1）：先合并「播报 workflow 的 ref 门」那笔改动，再录 Secret。**
> 播报 workflow（`.github/workflows/signing-status.yml`）会把 `ULL_MANIFEST_SIGNING_KEY` 注入到
> 就绪度那一步的环境里；那个注入**只在 `refs/heads/main` 上发生**，而且该 workflow **没有**
> `pull_request` 触发。这两条是安全边界：仓库内分支的 PR 与任意分支的 `workflow_dispatch` 都拿得到
> 仓库 Secret，而 CI 会在**那个 PR 自己的代码**上执行 checker —— 种子一旦进过那种运行，等于交给
> 任何能推分支的人（这把种子能给**全量已安装客户端**签可信清单，即用户端 RCE）。
> 所以：**ref 门那笔没合进去之前不要录 Secret**；录之前先确认线上版本里有
> `github.ref == 'refs/heads/main'` 这个条件式（§6 也写了这条）。

```bash
# ① 生成钥（⚠ 输出是敏感物；不要跑在 CI 里，不要重定向到仓库里的文件）
python3 packaging/sign_manifest.py --generate-key --key-id <key-id，例 ull-release-2026a>

# ② 把打印出来的那一行 JSON（{"key_id": …, "seed_hex": …}）贴进仓库 Secret（⚠ 先读上面的顺序硬要求）：
#    Settings → Secrets and variables → Actions → New repository secret
#    名字必须**逐字**是：ULL_MANIFEST_SIGNING_KEY
#    （等价 CLI：gh secret set ULL_MANIFEST_SIGNING_KEY，然后按提示粘贴，别用 -b 把值留在 shell 历史里）

# ③ 把打印出来的那一段 PinnedKey（4 行，含结尾逗号）贴进 trust.rs 的 PINNED-KEYS 标记之间（标记本身别动）
#    那个形状就是 cargo fmt 会保持的形态 —— 贴完**不用**再跑 fmt（压成一行反而会被 fmt 拆开）
grep -n 'PINNED-KEYS' crates/core/src/update/trust.rs
#    贴完提交这次改动 —— 公钥进 git 是对的：它是信任锚，本来就该公开可审计

# ④ 对账（需要一个有钥的环境：本机就先 export，别写进任何文件）
python3 packaging/sign_manifest.py --check-pinned-keyring     # 期望：ok: 签名钥 <key-id> 已被钉住

# ⑤ 发一次带 .sig 的 Release，然后用**客户端的判据**回验（这一步不需要私钥）
python3 packaging/sign_manifest.py --verify-pinned dist/manifest.json   # 期望：ok: 用钉住的公钥能验过
```

`--verify-pinned` 才是证据：`--sign` 说「签好了」不算 —— 客户端拿的是 `trust.rs` 里那把公钥。
发布后建议再用**下载下来的资产**验一次（§5），因为「CI 上能验过」不等于「上传的那份就是被签的那份」。

### 每一步的判据（怎么知道这步真的成了）

| 步 | 成功长什么样 | 失败长什么样 |
|---|---|---|
| ① | 打印 ① 私钥 JSON、② 一段 4 行的 `PinnedKey`、③ 公钥十六进制；脚本自带「自己签自己验」自检 | `error: 自己签自己验不过` ⇒ 不要用这把钥，环境有问题 |
| ② | `gh secret list` 里能看到 `ULL_MANIFEST_SIGNING_KEY` | 列表里没有 / 名字拼错（大小写、下划线） |
| ③ | `check_signing_ready.py` 的 ① 从「未就绪」翻成「就绪」 | ① 仍说 0 条 ⇒ 贴到了标记**外面**（读数里会点名「另有 N 条在标记之外（key_id：…）」） |
| ④ | `ok: 签名钥 … 已被钉住` | `签名钥（…）不在信任表里` ⇒ ②③ 不是同一把钥 |
| ⑤ | `ok: manifest.json.sig 用钉住的公钥 … 能验过（客户端会接受）` | `拿 trust.rs 里钉住的公钥回验不过 —— 这份 Release 的用户会全部拒装。别发版` |

## 3. 一次发布里签名是怎么被执行的（不用额外操作）

`release-publish.yml` 的「签更新清单」步按顺序跑四条，任一非零 ⇒ 本步红 ⇒ 不出 Release：

```bash
sign_manifest.py --self-test                    # RFC 8032 向量：脚本自己没坏
sign_manifest.py --check-pinned-keyring         # Secret 里的钥必须已在 trust.rs 钉住
sign_manifest.py --sign dist/manifest.json      # 产出 dist/manifest.json.sig
sign_manifest.py --verify-pinned dist/manifest.json  # 用钉住的公钥回验产出
```

这一步已经把「未配置 Secret」「钥没钉」「钥不匹配」都变成**发版前的红**，所以「发了但没签名」这条
不会发生 —— 它只会在发布 job 上红，不会静默发出去。

## 4. 已知坑（按踩到的概率排序）

1. **Secret 名字拼错**（`ULL_MANIFEST_...` vs `ULL_MANIFEST_KEY`）：可复用工作流只拿得到自己**声明过**的
   secret，名字不对就是空串 ⇒ 签名步红，而错误文案会反过来指控「Secret 没配」。`release-publish.yml`
   已经声明了这一个（`required: false`，缺省走 fail-closed），照抄名字即可。
2. **只配 Secret 没钉表**：`--check-pinned-keyring` 报「信任表是空的（fail-closed）」，并**直接打印**
   该贴的那一段 4 行 `PinnedKey`（只含公钥，安全）。照贴即可，注意连 `error:` 前缀一起删掉再贴。
3. **只钉表没配 Secret**：`--check-pinned-keyring` 报「没有签名密钥」。这不影响已有用户（表非空 ⇒
   他们能验签），但发不了新版。
4. **Secret 与 trust.rs 是两把不同的钥**：最常见于「换过钥但只更新了一边」。判据是**公钥字节**，
   `key_id` 只是标签；`--check-pinned-keyring` 会把两边的公钥都打出来对账。
5. **把条目贴到标记外面**（`];` 之后、或贴着别的 `const`）：等于没钉。`check_signing_ready.py` 会**点名**
   标记之外的 `key_id`；注意本仓 `TEST_ONLY` 那条**本来就在标记之外**（那是它的正确位置），所以读数是
   「另有 N 条在标记之外（key_id：…）」而不是「贴错了」—— 对着 `key_id` 自己认。
6. **`set -x` / `echo $ULL_MANIFEST_SIGNING_KEY`**：一次就够把私钥泄进日志。工作流里那一步显式 `set +x`
   且只回显**公钥**；本机调试同理，别开 trace。
7. **`--generate-key` 的输出落进文件/issue/聊天**：当成泄露处理，走 `signing.md` §5 轮换（旧钥签新钥）。
8. **以为「非 main 分支的读数」就是仓库状态**：播报 workflow **只在 `refs/heads/main` 上注入** Secret
   （也没有 `pull_request` 触发 —— 见 §2 的顺序硬要求与 §6）。非 main ref 的运行里 ② 显示
   「**未判定** —— 非 main ref（按设计不注入 Secret）」，这是**策略**，不是「没配」。以 `main` 上的读数为准。
9. **`.sig` 没上传到 Release**（手改过 `assets=(…)` 那一行）：客户端取 `<产物根>/manifest.json.sig`
   会 404 ⇒ 所有人验签失败。上传后请在资产列表里**亲眼看到** `manifest.json` 与 `manifest.json.sig`
   两个名字（§5 的 `gh release view` 那条命令就是干这个的）。
10. **删掉旧钥条目太早**：用户机器上装着旧二进制，它只认旧钥；删条目的那个版本必须**只**由新钥签，
    并且至少一个发布周期的双签名之后（`signing.md` §5）。
11. **「昨天验过」不等于「今天验得过」**：`PRODUCTION` 是**编译期**常量，改 `trust.rs` 只影响**下一次构建**。
    已在用户盘上的旧二进制永远用旧表 —— 这是设计，不是 bug。
12. **给播报 workflow 加回 `pull_request` 触发、或去掉注入行的 ref 门**：等于把生产签名种子交给任意推
    分支的人的代码（自更新链 = 用户端 RCE）。这两处各有接线用例钉着（去掉任一处都会红），
    别为了「PR 上也能看读数」去动它 —— 看读数请对 `main` 跑，或读 §1 在本地跑。

## 5. 发布后回验（发布侧，不需要私钥）

```bash
# 资产里必须两个名字都在（漏一个等于全员验签失败）
gh release view <tag> --json assets --jq '.assets[].name' | grep -E 'manifest\.json(\.sig)?$'

# 拿下载回来的真实资产复现客户端判据：manifest.json 与 .sig 必须成对、且是同一份字节
mkdir -p /tmp/verify && gh release download <tag> --pattern 'manifest.json*' --dir /tmp/verify
python3 packaging/sign_manifest.py --verify-pinned /tmp/verify/manifest.json
```

退出码 0 = 「客户端会接受这份清单」。非零时**不要**手动 `edit` Release 绕过 —— 先看脚本 stderr
（它会说清楚是表空、无 `.sig`、还是签名对不上）。

## 6. CI 里那条读数：`Signing status（播报）`

`.github/workflows/signing-status.yml` 在 `push: branches: [main]` 与**手动 dispatch** 上跑，把 §1 的三行
搬进 job summary。**它不是门禁**：

* 它**不应被设为 required check**（分支保护里不要勾它），也不阻断任何其它 job；
* 它就绪度那一步**恒 `exit 0`**：未就绪用 `::warning::` 播报，不用 error 级注解（那会让人以为构建挂了）；
* **生产签名种子只在 `refs/heads/main` 上注入**（`github.ref == 'refs/heads/main' && secrets.… || ''`），
  而且**没有 `pull_request` 触发**。理由是安全：仓库内分支的 PR / 任意分支的 dispatch 都拿得到仓库 Secret，
  而 CI 跑的是那个 ref 自己的代码 —— 种子进过那种运行就等于交给任何能推分支的人。
  非 main ref 的运行里 ② 显示「**未判定** —— 非 main ref（按设计不注入）」，**不写「没配」**；
* 它**刻意不声明 `workflow_call:`**，所以别的 workflow 没法 `uses:` 它；接线自检还会扫全部 workflow，
  任何 `uses: …/signing-status.yml` 出现即红（防的是「顺手接进 dev 线把播报变成门禁」）；
* 它**绿**只说明脚本跑完了；结论只看 summary 里的三行字。
* 同一 workflow 里另有两步跑自测（`packaging/tests/signing_ready_selftest.py` +
  `packaging/tests/signing_ready_wiring_selftest.py`）：那两步**会红**，红 = **尺子坏了**（三行读数/退出码/
  变异重放/接线与 ref 门之一没过），与「签名未就绪」是两件不同的事，措辞在 summary 里分开写。

为什么不做成门禁：今天已经有四次「守卫假绿」的账（见 `packaging/README.md` §12 里那几条先例）。
把一条**按定义今天必然失败**的检查做成 required，只会有两种结局 —— 要么所有人学会忽略红叉，
要么有人把它改成永远绿。真正的红留在 `release-publish.yml` 的签名步：那里今天红是**对的**，
因为签不出来的产物本来就不能发。

## 7. 本手册**没**覆盖 / 没验的部分（明账）

* **Authenticode / SmartScreen**：项目自有签名解决的是「这条自更新链是不是我们发的」，用户双击 exe 看到
  的「未知发布者」与此无关（`signing.md` §0）。
* **托管签名**（如 SignPath）：只是后续可替换路径，本手册与判据都不依赖它。
* **本文档里的命令没有被真跑过一遍**：`--generate-key` 会产出生产私钥，塔不生成、也不跑它；
  ②③ 两步要动 GitHub Secret 与 Rust 常量，同理。所以本手册是「按现有脚本的接口写的步骤」，
  首次执行时**以脚本的实际输出为准**，跑完请回来修正本文档。
* **「用户机器上真的更新成功」**没有在本条里验：需要一次真 Release + 一台装着旧版本的机器。
* **粘贴块「过一遍 fmt 不变形」只在有 `rustfmt` 的机器上是硬证据**：`sign_manifest.py --self-test` 会在
  临时副本上真跑一遍 `rustfmt` 比对；PATH 里没有 `rustfmt` 时它会打一行 `note: ... 跳过`，只剩
  「形状 == `trust.rs` 现存条目的形状」这条结构断言（抓得住回退成单行/缩进走样，抓不住 rustfmt 版本或
  `.rustfmt.toml` 改配置带来的排版变化）。
* **ref 门只在本地验证过**（抽 `run:` 块用 Git Bash 跑 + 接线用例的文本变异），没有在真 Actions 上看过
  `secrets` 的求值结果 —— 第一次真跑请确认非 main ref 的 ② 显示「未判定」。

## 8. 相关文件

| 文件 | 作用 |
|---|---|
| `packaging/check_signing_ready.py` | 三行就绪度 + `0/1/2` 退出码（只读、零第三方依赖） |
| `packaging/tests/signing_ready_selftest.py` | 判据自测：三行/退出码/两向变异重放/`--key` 用法错/指针章节 |
| `packaging/tests/signing_ready_wiring_selftest.py` | 接线自测：不是门禁 / 不被 `uses:` / **ref 门** / 文档路径存在性 + 三处变异 |
| `.github/workflows/signing-status.yml` | 把三行搬进 job summary 的**播报** workflow（不是门禁；种子只在 main 注入） |
| `packaging/docs/signing.md` | 为什么签名、威胁模型、轮换纪律、四条发布侧命令的红法对照表 |
| `packaging/sign_manifest.py` | 生成钥 / 签名 / 对账 / 回验的唯一实现（`--self-test` 跑 RFC 8032 向量 + 粘贴块形态） |
