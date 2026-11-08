# 构建与 CI 流水线（M145 拆自 `packaging/README.md`）

本文承载原 `packaging/README.md` 的 §3，**节号沿用原编号**（全仓 `packaging/README.md §N` 这类引用照旧有效，
对照表在 [`packaging/README.md`](../README.md) 的索引里）。内容一字未改、只搬家：本行以上是 M145 新增的导航说明，
以下逐行取自原文件。

## 3. 怎么构建

### 本地（当前平台）

```bash
cargo build --release --locked                 # 产出 target/release/{unlucky-launcher-iced,ull}（M118：Slint 版不再进 workspace）
cargo build --profile fast --bins              # 只是想快点看到画面/跑测试：快档（§6），产物在 target/fast/
python packaging/make_artifacts.py --out dist  # 产物落在 dist/，含 SHA256SUMS.txt
python packaging/make_artifacts.py --version v0.2.2   # 顺带校验版本一致性
python packaging/make_artifacts.py --no-smoke         # 跳过 ull --version 冒烟
```

脚本按宿主自动判定槽位（Windows x64 / Windows arm64 / Linux x64 / Linux arm64 / macOS arm64 /
macOS x64），其它组合直接报错退出——不在矩阵里的平台不出"凑合能跑"的产物。

交叉构建（例如在 x64 宿主上产 Windows arm64 产物）靠两个参数把"产物根"和"槽位"显式喂进去，
脚本不去猜宿主与产物是不是同一架构（**CI 不用这条路径**，见下）：

```bash
rustup target add aarch64-pc-windows-msvc
cargo build --release --locked --target aarch64-pc-windows-msvc \
  --bin unlucky-launcher-iced --bin ull   # 不用 --bins：矩阵只发 GUI + ull；GUI 的构建产物名是 iced 的 bin（M118）
python packaging/make_artifacts.py --slot windows-arm64 \
  --build-dir target/aarch64-pc-windows-msvc/release --out dist
```

`--slot` 与宿主槽位不一致时，`ull --version` 冒烟会**自动跳过并打印原因**（arm64 产物在 x64
宿主机上根本执行不了；拿"跳过"当通过是假证据，所以跳过永远带原因）。**CI 不走交叉**（M140）：
`windows-arm64` 在原生 `windows-11-arm` 上编，宿主=产物架构 → 冒烟**真跑**；`linux-arm64` 的同类
冒烟只在 `all_slots=true` 的取证跑里出现（`ubuntu-22.04-arm` 原生 runner 上真跑 `ull --version`）。

### CI（默认只跑两个 Windows 槽位；`all_slots=true` 才整表六槽位）

`.github/workflows/release.yml`：

- 触发：`push` tag `v*`；`workflow_dispatch` 只构建不发布，且多一个 `all_slots` 开关（**默认 false =
  只跑两个 Windows 槽位**，true = 整表六槽位都编一遍取证）。`inputs.prerelease` **只在 tag ref 上 dispatch
  才可达**——Actions 页面的 "Run workflow" 只能选分支，要跑 tag 得 `gh workflow run release.yml --ref v0.2.3`。
  分支 ref 的 dispatch 与 push 一样被 `release` job 的 `if: github.ref_type == 'tag'` 挡在发布之外，所以
  「分支 ref 的 dispatch 只构建不发布」与「tag ref 上 dispatch 能选标记」是两条互不冲突的规则。
- 矩阵**由 `plan` job 按 `all_slots` 生成**（`matrix: ${{ fromJSON(needs.plan.outputs.matrix) }}`：矩阵本身
  不能带 `if`，而 `strategy` 可读 `needs`），槽位表只在该 job 里有一份：`windows-latest` x64 与
  **原生** `windows-11-arm` arm64（M140；后者是公开仓免费的标准 arm64 runner，4C/16G）——两者都是 MSVC +
  `-C target-feature=+crt-static`、产物都在 `target/release`，标 `mode: ci`（默认编）；
  `ubuntu-22.04`（glibc 2.35 地板）/ `ubuntu-22.04-arm`（原生 ARM64，同地板）/ `macos-latest` /
  `macos-15-intel` 标 `mode: probe`——**未验证，默认一个都不编**，只有 `all_slots=true` 才进矩阵。
- 每个槽位 `cargo build --release --locked`（**不传 `--target`**：每槽原生编译，交叉槽位与 `rustup
  target add` 一并没了）。arm64 槽位多一步编译前体检：查 VC 的 `lib/arm64/libcmt.lib`（`+crt-static`
  靠它）与 SDK 的 `um/arm64/kernel32.lib`，缺了就 fail；**注解里固定打 4 行实况**（探测根 / SDK Lib 版本→um
  架构 / MSVC lib 架构 / 两个关键库）。M142 修的正是「诊断自己先死」：`set -uo pipefail` 关不掉 runner 注入的
  `-e`，`find` 扫不存在的根被 pipefail 判非零就当场退出 → 现在 `set +e` + 处处 `|| true`，清单先打完再定红绿。
- **发布白名单（`PUBLISH_SLOTS`，唯一开关）**：只有白名单里的槽位进 Release 与 `SHA256SUMS.txt`。**M142 起只有
  `windows-x64`**：arm64 连试四轮未走通，而齐全性检查是「名单内少一档就 fail」，挂着它就等于 x64 编成功也发不出去；
  它仍留在矩阵里照跑取证、红了不挡发布。**可逆**：arm64 修好后把键加回 `release.yml` 那一行，再同步 §1/§9 与
  `release-notes.md`。四档未验证的（默认连构建都不跑）依旧不推给用户。
- `release` job：下载全部 artifact → **按白名单裁剪** → 重算 `SHA256SUMS.txt` → 用 `packaging/release-notes.md`
  模板渲染支持矩阵 → 生成更新清单 `manifest.json` → `gh release create`（**push tag 默认建稳定版**：不带任何标记）。
- **同一 tag 重跑幂等**：`gh release view` 命中已存在的 Release 时改走 `gh release upload --clobber`
  + `gh release edit`，覆盖资产与说明而不是再 `create`（`create` 遇已存在会失败，把流水线最后一步染红）。
- **预发布标记的语义表（M163，唯一真相源 `packaging/release_marker.py`）**：

  | 触发 | create（新建 Release） | edit（Release 已存在） |
  | --- | --- | --- |
  | `push` tag `v*`（含其 Re-run） | **稳定版**（不带标记） | 不碰标记 |
  | `workflow_dispatch` + `prerelease=true` | 预发布 | 显式改成预发布 |
  | `workflow_dispatch` + `prerelease=false`（默认） | 稳定版 | 显式改成稳定版 |

  空串必须是稳定版：`releases/latest/download/*` 只指向**非 prerelease** 的最新 Release，而自更新清单固定取
  `releases/latest/download/manifest.json`（D50）——push tag 打成预发布，稳定通道就一直是 404（v0.2.2 至今
  如此，M163 修的就是这条；**v0.2.2 那个 Release 保留为历史预发布不动**，稳定通道以 v0.2.3 为起点）。
  edit 的空串是「不碰」而不是「改成稳定版」：同 tag 重跑**不会**把用户手工提成稳定版的 Release 翻回预发布。
  要改已发版本的标记仍是两条等价路：① `gh workflow run release.yml --ref <tag> -f prerelease=false`
  （落到 edit 分支显式断言）② 直接 `gh release edit <tag> --prerelease=false`（不经流水线，后续默认重跑不碰它）。
- **这条语义有守门（M163）**：`packaging/check_release_semantics.py`（CI 在 `.github/workflows/release-guard.yml`）
  断言四件事：标记表逐条对（push tag 的 create 不含 `--prerelease`）、把 `release_marker.py --emit-bash` 的产物
  **真灌进 bash** 读回两个数组、`release-publish.yml` 的可执行内容里没有 `--prerelease` 字面量、dispatch 默认值
  `== false`（读 YAML 结构，不是 grep）。`--self-mutation` 再把「默认值改回 true」「塞回字面量」「表里 push 分支
  改成预发布」注入临时副本，断言守门**每一项都红**、基线绿——守门自己抓不住回归时它先红。
- **M145 拆分（本节新增的唯一一段）**：上面所有口径都还成立，只是"在哪个文件里"变了 —— `release.yml` 只留
  触发、`concurrency`、槽位表（`plan`）与两个 `uses:` 编排；`build` job 的 11 步逐字搬进
  `.github/workflows/release-build.yml`（`workflow_call`，四个矩阵键换成同名 `inputs.*`），`release` job 的 6 步
  逐字搬进 `.github/workflows/release-publish.yml`（同 `workflow_call`，`secrets.GITHUB_TOKEN` 改为调用方显式透传
  的 secret）。触发（`on`）、`env`、顶层 `permissions`、`concurrency` 与 `plan` 的槽位表在 M145 搬家时**一字未动**；
  后来只有 M163 动过 `on` 里 `prerelease` 的默认值（`true` → `false`，见上面那张语义表）。改槽位仍只改
  `release.yml` 那一处（`PUBLISH_SLOTS` 白名单在 `release-publish.yml`，两个 arm64 修复时要改的仍是那两行）。
- **M149 微软登录 id 的烘焙**：`release-build.yml` 的 `cargo build` 步带 `env.ULL_MSA_CLIENT_ID`（值
  `${{ secrets.ULL_MSA_CLIENT_ID }}`），`option_env!` 把它烘进二进制，于是发布产物装完即可登录。id **不进
  git 文件**：真值放仓库 Secret，由 `release.yml` 的 `build` job 按名显式透传（可复用工作流拿不到未显式传入的
  secret；这里刻意不用 `secrets: inherit`——那会把仓库全部 secret 一起透进被调方）。Secret 未录入、或在整个
  Actions 之外求值时该表达式为空串，`client_id.rs` 把空白当「没配」⇒ 非官方构建只是不烘 id，不会拿空 id 去
  联网；本地自烘仍走 `ULL_MSA_CLIENT_ID=<id> cargo build`。**别把这个 env 导出到跑 `cargo test` 的 shell**：
  presenter 的 M132 入口锚会在「已烘焙 id」的构建里**故意红**（它要求一个两处都没有 id 的构建）。
- 发布说明模板（`release-notes.md`）的 "Known limitations" 里，微软登录一条已改为「开箱即用、env 可覆盖」。

Windows 为什么用 MSVC 静态 CRT 而不是本机这套 GNU 工具链：两者都属"标准工具链"（D21），
但发布产物要**不依赖 VC++ 可再发行包**，`+crt-static` 把 CRT 静态链进 exe，Win10+ 的 UCRT 本身是
系统组件，于是 exe 真正单文件可拷走。GNU 交叉产物在 CI 上还要拉 MinGW 链接器，收益为零。
