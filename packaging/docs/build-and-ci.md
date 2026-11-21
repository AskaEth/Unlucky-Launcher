# 构建与 CI 流水线（M145 拆自 `packaging/README.md`）

本文承载原 `packaging/README.md` 的 §3，**节号沿用原编号**（全仓 `packaging/README.md §N` 这类引用照旧有效，
对照表在 [`packaging/README.md`](../README.md) 的索引里）。内容一字未改、只搬家：本行以上是 M145 新增的导航说明，
以下逐行取自原文件。

## 3. 怎么构建

### 本地（当前平台）

工具链由 `rust-toolchain.toml` 钉在 **1.99.1**（M391 / GAPS E6）：rustup 按该文件自动选工具链，`fmt` 与
`clippy` 的判据因此不再随上游 `stable` 漂。为什么是 1.99.1 而不是更新的 1.100.0、以及升级步骤，见该文件注释。

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
`windows-arm64` 在原生 `windows-11-arm` 上编，宿主=产物架构 → 冒烟**真跑**；`linux-arm64` 也是原生
`ubuntu-22.04-arm` 构建 → 冒烟在 runner 上真跑（M184 起它默认就编，不再只在 `all_slots=true` 的取证跑里出现）。

### CI（默认跑四个槽位：Windows 两档 + Linux 两档；`all_slots=true` 才整表六槽位）

`.github/workflows/release.yml`：

- 触发：`push` tag `v*`；`workflow_dispatch` 只构建不发布，且多一个 `all_slots` 开关（**默认 false =
  只跑四个槽位**，true = 整表六槽位都编一遍取证）。`inputs.prerelease` **只在 tag ref 上 dispatch
  才可达**——Actions 页面的 "Run workflow" 只能选分支，要跑 tag 得 `gh workflow run release.yml --ref v0.2.3`。
  分支 ref 的 dispatch 与 push 一样被 `release` job 的 `if: github.ref_type == 'tag'` 挡在发布之外，所以
  「分支 ref 的 dispatch 只构建不发布」与「tag ref 上 dispatch 能选标记」是两条互不冲突的规则。
- 矩阵**由 `plan` job 按 `all_slots` 生成**（`matrix: ${{ fromJSON(needs.plan.outputs.matrix) }}`：矩阵本身
  不能带 `if`，而 `strategy` 可读 `needs`），槽位表只在该 job 里有一份：`windows-latest` x64 与
  **原生** `windows-11-arm` arm64（M140；后者是公开仓免费的标准 arm64 runner，4C/16G）——两者都是 MSVC +
  `-C target-feature=+crt-static`、产物都在 `target/release`，标 `mode: ci`（默认编）；
  （M184）`ubuntu-22.04`（glibc 2.35 地板）与 `ubuntu-22.04-arm`（原生 ARM64，同地板）**也标 `mode: ci`** ——
  tag 推送默认就编这两档；只剩 `macos-latest` / `macos-15-intel` 是 `mode: probe`，**未验证、默认一个都不编**，
  只有 `all_slots=true` 才进矩阵。
- **运行时证据决定发布，构建绿了不够**：`linux-x64` 进 `PUBLISH_SLOTS` 的依据是「release 产物真开窗渲出一帧」
  （真机 WSLg + 截图 + `[theme]` / `[backend] final` 日志，实测输出见 `packaging/docs/linux-evidence.md`），
  所以 `release-build.yml` 给**这个槽位**配了一条 xvfb 运行时冒烟步（`if: inputs.slot == 'linux-x64'`）：
  装 xvfb → `xvfb-run … --software-renderer --screenshot` 起 **release** 产物 → 断言 `[theme] source=` 与
  `[backend] final=… result=started` 两行 → `check_screenshot.py` 判像素 → 上传首帧图。
  `linux-arm64` **刻意没有**这条判据：arm64 上这条路一次都没验过，给它加一条没验证过的门禁就是造红。
- 每个槽位 `cargo build --release --locked`（**不传 `--target`**：每槽原生编译，交叉槽位没了）。arm64 槽位在编译前
  **把工具链显式备齐**（M164）：`rustup target add aarch64-pc-windows-msvc`、用微软自己的 `vcvarsall.bat arm64` 取
  `LIB`/`INCLUDE`（写进本步环境与 `$GITHUB_ENV`）、用 vswhere 定位 arm64 `link.exe` 后显式导出
  `CARGO_TARGET_AARCH64_PC_WINDOWS_MSVC_LINKER`（Hostarm64 原生优先、Hostx64 次之；`-find` 的斜杠/反斜杠两种形态都试）。
  顺序不能反：只覆盖链接器而不给 `LIB`，撞上的就是 `LNK1181: cannot open input file 'kernel32.lib'`。
  **`cmd //c` 不能直接收「带引号的 vcvarsall 路径」**：MSYS 会把 `"` 改写成 `\"`，cmd 的 `/c` 剥离规则接不住，
  结果 `LIB`/`INCLUDE` 全空（M164 复审实测；`cmd //c` 只调**不含引号**的路径是好的）。当时的报错原文是
  `'\"C:\...\vcvarsall.bat\"' 不是内部或外部命令`（将来搜到这句、想知道来龙去脉，就看这一段）。故本步往 `$RUNNER_TEMP`
  写一个 CRLF 的 wrapper `.bat`（`call "<vcvarsall>" arm64` + `set`），`cmd //c` 只收相对文件名——引号留在
  `.bat` 内部由 cmd 自己解析，路径带空格也不受影响。
- **编译前体检已降级为「只取证」**（M164）：M141/M142 的版本用 `find` 猜 `um/arm64/kernel32.lib` 在不在、猜缺就
  `exit 1` —— v0.2.1~v0.2.3 五轮全红在**那一步**，`cargo build` 一次都没跑过（永远 skipped），而它跟自己打的快照
  自相矛盾（`um/arm64` 明明在）。现在那一步只打印 vswhere 组件查询、各目录 `.lib` **计数**与 `link.exe` 候选，
  永不 `exit 1`；**红绿只由编译器 / 链接器 / 静态 CRT 导入表门禁决定**。runner 实况：`windows-11-arm` 自 2026-09
  是 Windows 11 Arm64 + **Visual Studio 2026** 镜像，`VC.Tools.ARM64` 与 `Windows11SDK.22621/26100` 都已装好。
- **发布白名单（`PUBLISH_SLOTS`，唯一开关）**：只有白名单里的槽位进 Release 与 `SHA256SUMS.txt`。**M184 起是
  `windows-x64 linux-x64`**：齐全性检查是「名单内少一档就 fail」，所以 `linux-x64` 进来之后它的构建红了就挡住整次
  发布 —— 这正是「声明为 ci 的槽位失败必须阻断发布」那条要求，判据落在 `check_publish_slots.py` 的 `--dist` 模式里
  （`release` job 是 `always()`，构建矩阵里红一档不会让它跳过，挡住它的只能是那道守卫）。
  `linux-arm64` 本轮**只构建不发布**：它没有任何运行时证据（没有 ARM64 真机跑过一帧，也没有 arm64 的 xvfb 冒烟），
  **解锁条件 = 真机上真跑出一帧 + 主题日志**，之后才把键加进 `release-publish.yml` 那一行，再同步 §1/§9 与
  `release-notes.md`。`windows-arm64` 同理仍未进名单（M164：它的五轮红全红在误判的体检步，等下一 tag 的 CI 证）。
  **可逆且只是一行**：翻牌只改 `PUBLISH_SLOTS` 那一行，齐全性与 SHA256SUMS 自动跟着扩。
- **耗时（M184 估算，2 槽位 → 4 槽位）**：多出的两档是**并行**跑的（矩阵 `fail-fast: false`），所以墙钟增量 ≈
  「`linux-x64` 那一档的时长（含新增 xvfb 冒烟步约 1~2 分钟）」与「`linux-arm64` 那一档的时长」取大者，而不是两者相加；
  真正增加的是**并发作业数与总机时**（约 +2× 单槽位机时，公开仓 arm64 Linux runner 不额外计费）。
  发布侧只多两个资产的裁剪/摘要/上传（秒级）。细节与逐项拆解见 `packaging/docs/linux-evidence.md`。
- **名单 ⇄ 产物两个方向都有守卫（M164，M184 扩）**：`packaging/check_publish_slots.py`，同一份代码两处跑 —— PR 阶段
  （`.github/workflows/release-guard.yml`）断言「名单非空 / 每项都是槽位表里 `mode: ci` 的档」；发布侧
  （`release-publish.yml` 的 `--dist` 模式，跑在**裁剪前**的全量集上）断言「声明了就得有 GUI 与 CLI 两个产物，
  缺一个就红」，并把「有产物但名单没声明」打成 `::warning::` 明说未发布 —— M142 之后那个方向是静的（裁剪步只
  `echo drop:`），一个真编出来的 arm64 会被就地删掉而没人知道。M184 补第三条：`mode: ci` 的槽位**本次连产物都没有**
  （那一档构建红了）也打 `::warning::`，别只在表里少一行。`--self-test` 注入 11 条回归（含「声明的 `linux-x64` 构建红了
  ⇒ 必须红」「`linux-arm64` 有产物但没声明 ⇒ 只警告」「提前把 `linux-arm64` 写进名单 ⇒ 缺产物必红」），本地一条命令可跑。
- `release` job：下载全部 artifact → **发布名单守卫**（断言 + 未发布提示）→ 按白名单裁剪 → 重算 `SHA256SUMS.txt` →
  用 `packaging/release-notes.md` 模板渲染支持矩阵 → 生成更新清单 `manifest.json` → `gh release create`
  （**push tag 默认建稳定版**：不带任何标记）。
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
  后来只有 M163 动过 `on` 里 `prerelease` 的默认值（`true` → `false`，见上面那张语义表），M164 改过
  `release-build.yml` 的 arm64 体检步与 `cargo build` 步、并在 `release-publish.yml` 裁剪前插了「发布名单守卫」
  （于是本分支 8 步：checkout / download / **守卫** / 裁剪 / 重算 SHA256SUMS / manifest / 说明 / `gh release`；
  搬家时 6 步、主线 7 步）。M184 又在 `release-build.yml` 里给 `linux-x64` 插了「运行时冒烟 + 上传首帧图」两步
  （该文件 11 → 13 步，发布侧仍是 8 步）。改槽位仍只改 `release.yml` 那一处（`PUBLISH_SLOTS` 白名单在
  `release-publish.yml`，翻牌时要改的仍是那两处）。
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

### 发版前：手动 dispatch 一次 update 回归（M368）

更新链路的端到端验收（`update-e2e`）**在 dev 线上一次都不会自动跑**：`ci.yml` 的 `push` 只挂 `main`，
`pull_request` 不过滤分支但本仓日常合并不走 PR ⇒ dev 线的 update 回归「第一次能被看见的时机 = 推快照那一刻」
（M365 结论原文）。M368 把这条作业整段抽成 `.github/workflows/update-e2e.yml`：作业体（命令、`env`、
`timeout-minutes: 45`）**逐字未改**，只多了触发方式 —— `workflow_call`（`ci.yml` 的 `update-e2e` job 用
`uses:` 调它，于是 push main / pull_request / schedule 的触发与抽取前**完全相同**）+ `workflow_dispatch`
（**新通道：dev 线唯一能主动看见它的方式**）。抽出这个文件的直接收益是行数：`ci.yml` 398 → 371 行
（400 行红线在**抽前**只剩 2 行余量，作业体每改一次都要先还一笔行数债）。

操作项（写进发版清单，**在打 tag 之前**）：

```bash
gh workflow run update-e2e.yml --ref <待发分支>   # Actions 页面 "Run workflow" 等价
```

- **什么时候跑**：准备打 `v*` tag 之前，在**待发分支的末梢**上跑一次；红就先别打 tag —— tag 之后再发现
  update 回归挂掉，代价是「已经发出去的产物装不上更新」。
- **怎么判红**：先看 check-run 的**注解**里有没有 cargo 正文。验收条件（M365 原文）：**下一次它红必须带
  cargo 正文；只有一句 `Process completed with exit code 1` 说明 pwsh 包装层的问题还在** —— 那种情况要修的是
  pwsh 包装层，不是 cargo（`ci.yml` 的 test 作业、`update-e2e.yml` 都带了「失败行升 `::error::`」那一段，
  就是为了在拿不到 job log 时注解里还有正文）。
- **入口前提**：`workflow_dispatch` 的入口只在工作流文件已存在于**默认分支**时才出现在 Actions 页面
  （GitHub 文档语义）⇒ 本文件合入 `main` 之前这条通道还不存在，别在 dev 上找它。
- **绿 ≠ 跨卷绿**：runner 上只有一块可写卷，这条作业恒走豁免分支（`ULL_XDEV_TEST_WAIVE: "1"`）⇒ 跨卷替换 /
  回滚那条路径在 CI 上一次都没真跑过，唯一真验过的是本机 C:/D: 两块卷那一次（M171）。CI 绿只能读成
  「单卷上的 E2E 绿」。

### 构建缓存 key：必须带镜像维度（`Swatinem/rust-cache`，M270）

**事故形态（M269 端到端复现）**：`linux-x64` 发布构建红在链接期，`libaws_lc_sys-*.rlib` 里的 `bcm.o` 引用
`__isoc23_sscanf` / `__isoc23_strtol` —— 那是 glibc ≥ 2.38 的头把 `sscanf` / `strtol` 改写成 `__isoc23_*` 之后才有的符号，
而链接发生在 `ubuntu-22.04`（glibc 2.35）。干净 22.04 容器里同一条命令 **exit 0**，干净 24.04 也 exit 0 ⇒ 不是「镜像版本
差异」本身，是**缓存把 24.04 编出来的 `.o` 搬到了 22.04**。

**key 是怎么算出来的**（`rust-cache@v2` 的 `src/config.ts`；`add-job-id-key` 与 `add-rust-environment-hash-key` 两项默认都是 true）：

```
key = prefix-key「v0-rust」
    + 「-<key 输入>」            ← 本仓 M270 才第一次用（下文「修法」）
    + 「-<GITHUB_JOB>」          ← 是 job **id**，不是 job 的 `name:`
    + 「-<os.type()>-<os.arch()>」 ← Linux-x64 / Windows_NT-x64 / Darwin-arm64，**没有镜像标签**
    + 「-<env hash 前 8 位>」     ← rustc -vV（release + host + commit-hash）+ CARGO*/CC/CFLAGS/CXX/CMAKE/RUST* 的 sha1
restoreKey = 到这里为止的一整串                      ← 兜底恢复 key（**不含**锁哈希）
key       += 「-<manifest/lock hash 前 8 位>」       ← 各 Cargo.toml（版本号被抹平）+ Cargo.lock + .cargo/config.toml
cacheKey   = key
```

两条恢复路径（`src/restore.ts` 就是一句 `restoreCache(paths, cacheKey, [restoreKey])`）：

- ① 主 key **逐字节相同** → `full match = true`，**不清理** `target/`，直接吃别人的产物；
- ② 主 key 未命中、`restoreKey` **前缀匹配**到某条已有缓存 → `full match = false`，先 `cleanTargetDir`（只清 profile 层），
  依赖的 `.o` / `.rlib` 留着算 fresh —— M269 那条「依赖 fresh + 旧系统链接」正是走这条。

**为什么 CI 与发布两侧会撞**：`ci.yml` 的 job id 是 `build`（跑 `ubuntu-latest` = 24.04），`release-build.yml` 的 job id
也是 `build`（跑 `ubuntu-22.04`）；`rust-toolchain.toml` 是 `channel = "stable"`（同一天两边解析到同一版 rustc），
`Cargo.lock` 也是同一份 ⇒ env hash 与锁哈希全相同 ⇒ 两个 key **逐字节相同**。这串表达式里**没有任何一项**能反映出镜像。
`RUSTFLAGS` 也救不了：它只写在 `cargo build` 那一步的 `env:` 里，cache 步根本看不到它。

**为什么 cargo 自己不发现**：freshness 判据是 fingerprint，记的是编译器版本、features、profile、依赖 metadata ——
而 `-C metadata` 里**没有 glibc 版本**。于是恢复来的 `.rlib` 被判 fresh、不重编，问题一路拖到最后的链接。

**修法**（CI 侧 6 处 = `ci.yml` 5 处 + `update-e2e.yml` 1 处，`release-build.yml` 1 处；全仓共 8 处，第 8 处在 `dev-guards.yml`，见本节末尾。
M368 把 `update-e2e` 那个 job 抽到 `update-e2e.yml` ⇒ 是 5 + 1 + 1 处分在三个文件里，**站点总数与 key 算法都没变**）：

- **发布侧**：`key: ${{ inputs.os }}`（只用镜像标签，**不带 slot**）+ `save-if: ${{ github.ref_type != 'tag' }}`
  （tag run 只读不写）。两条各有单独小节。
- **CI 侧**：三个矩阵 job 用 `key: ${{ matrix.os }}`（自动跟 `runs-on`）；三个单 OS job 用它自己的镜像标签字面量。
  后者的 `key:` 与 `runs-on:` 是**两份手写文本**：改 `runs-on` 时必须一起改 `key:`，否则镜像维度会静默失准（不报错，
  只是继续跨镜像共用）。M368 之后这三个单 OS job 里有一个（`update-e2e`）在 `update-e2e.yml` 里，两份文本仍成对
  写在同一个文件里，只是**文件换了**：改它的时候别在 `ci.yml` 里找 `runs-on`。它的 `GITHUB_JOB` 仍是 job id
  `update-e2e` ⇒ key 与抽取前逐字节相同（文档语义，未在 runner 上观测），不会因搬家冷编一次。
- `key` 输入落在**前缀段**（`v0-rust-<key>-<job>-…`，在 `restoreKey` 定格之前），所以主 key 与兜底 `restoreKey`
  同时带上镜像维度，**两条恢复路径一起分开**。

#### 发布侧 key 为什么只取 `inputs.os`（不带 slot）

两个方向同时成立，去掉 slot 才对：

- **必须断的**：`linux-x64` @ `ubuntu-22.04` vs `ci/build` @ `ubuntu-latest` ⇒ 不同 key。这就是 M269 那条路径。
- **故意保留的**：`windows-x64` @ `windows-latest` vs `ci/build` @ `windows-latest` ⇒ **同 key**。同镜像、同 rustc、
  同一份锁 ⇒ 借 main 上的热缓存是安全的：M270 之前 Windows 走的正是这条路，而 `windows-x64` 一直在 `PUBLISH_SLOTS`
  里、产物是绿的。slot 本来就是冗余维度（每个槽位的 `inputs.os` 互不相同）。
- 代价与取舍：加了 slot 的那一版会把上面这一对也分开，两者的实质差异只是 `RUSTFLAGS=-C target-feature=+crt-static`。
  合并回同一把 key 后 cargo 按 fingerprint 区分（`RUSTFLAGS` 是否被计入本仓没有实测；但这是 M270 前线上一直成立的
  状态），最坏情况是多编一遍，不会错编。

#### 发布侧为什么「只读不写」（tag run 的缓存作用域）

- 官方文档（`docs.github.com` → dependency-caching → "Restrictions for accessing a cache"）：一次 run 只能恢复
  **当前分支**或**默认分支（main）**创建的缓存；不同 tag 名之间互不可读（`release-a` 写的缓存，`release-b` 读不到）。
- 推论一：发布侧的 key 是 `ubuntu-22.04` / `ubuntu-22.04-arm` / `windows-11-arm` / `macos-*`，而 **main 上一个 job 都
  不跑这些标签**（`ci.yml` 只有 `ubuntu-latest` / `windows-latest` / `macos-latest`）⇒ 这四个槽位**每次发布都是冷编**，
  连 registry / git 源码也拿不到（rust-cache 是「一把 key 一个 entry」，所有路径捆在一起，没有分路径缓存）。
  唯一还能借到热缓存的是 `windows-x64`（见上一小节）。
- 推论二：发布侧写出去的缓存只能被**同名 tag 的下一次 run** 读到，实际就是一次性死缓存；它还占 per-repo 的 10 GB
  配额（超出按最后访问时间 LRU 淘汰、7 天未访问过期），会把 main 上真正在用的 CI 热缓存挤掉。**所以 tag run 不写。**
- 为什么不是写死 `save-if: false`：把「写」留给唯一有效的场景 —— **在 main 上预热**。`gh workflow run release.yml`
  （在 main 的末梢上跑）那次 run 的缓存作用域是 **main**，之后从 main 末梢切出来的 tag run 才读得到它（锁哈希与
  rustc 版本一致才命中）。真要彻底禁写就把这一行改成 `false`，那时预热也要临时改文件才做得到。

#### 演算（核心判据；r1 换了 key，r0 那张表作废）

把上面的拼接写成脚本，对 15 个站点（ci 9：clippy/test/build 各 2 个 OS + 3 个单 OS job —— M368 之后其中
`update-e2e` 那站在 `update-e2e.yml`，站点数不变；发布 6）× 2 个
`Cargo.lock` 状态两两判定：

| 关系 | 改前（7 处都没有 `key:`） | 改后（`key: inputs.os` + `save-if`） |
| --- | --- | --- |
| ① `rel/windows-x64` ↔ `ci/build`（**同镜像**） | 主 key 相同 | 主 key **仍相同**（故意保留 / 安全） |
| ① `rel/linux-x64` ↔ `ci/build`（**跨镜像**） | 主 key 相同 ⇒ **M269 的 bug** | **不相同**（必须断） |
| ② **跨镜像** `restoreKey` 前缀命中 | **2 组**（`ci/build@ubuntu-latest` ↔ `rel/linux-x64` 两个方向） | **0 组** |

拼出来（env / 锁哈希用占位值，取值不影响结论）：

```
改后 ci/build@ubuntu-latest   cacheKey = v0-rust-ubuntu-latest-build-Linux-x64-3f9a1c07-b2d6e815
改后 rel/linux-x64@22.04      cacheKey = v0-rust-ubuntu-22.04-build-Linux-x64-3f9a1c07-b2d6e815
        ↑ 前缀 v0-rust-ubuntu- 之后立刻分叉（`l` vs `2`）；两边 restoreKey 都更长 ⇒ 前缀命中不可能
改后 ci/build@windows-latest  cacheKey = v0-rust-windows-latest-build-Windows_NT-x64-3f9a1c07-b2d6e815
改后 rel/windows-x64          cacheKey = v0-rust-windows-latest-build-Windows_NT-x64-3f9a1c07-b2d6e815
        ↑ 逐字节相同（同镜像，这是要保留的复用）；锁哈希不同时退化成路径②，仍是同镜像，安全
```

脚本按「**跨镜像**」这一条断言（15 个站点两两 + 两个锁状态）：改后任意两个 **runner 标签不同**的站点之间，主 key
相同 0 对、`restoreKey` 前缀命中 0 组。仍存在的相同/前缀关系全部落在**同一个镜像标签内部** —— 也就是
`rel/windows-x64` ↔ `ci/build` 那一对（同 `windows-latest`，正是要保留的复用）以及同站点换锁哈希那种（本来就该
复用依赖）。判据的输出是一次性的脚本，跑完即删；上面这几行就是它的结论。

#### `cache-targets` 到底关不关：**不关**

- **CI 侧**：关掉就只缓存 registry / git 源码，每个 job 都要从头编整棵依赖树（M269 干净容器实测同一条命令 18m43s
  量级），日更矩阵每次提交都付这笔钱，收益为 0 —— 加镜像维度之后 `target/` 的恢复永远来自**同一镜像标签**，
  M269 那条路径已经断了。
- **发布侧**：关掉确实能连 target 缓存一起省，但它**把 Windows 借 main 热缓存这条路一起掐掉**（target 不进缓存就
  没得借），而 `windows-x64` 是发布侧唯一还能借到的一档。发布侧的死缓存问题由 `save-if` 解决，比关 targets 精准：
  **该写的一次都不落，该借的一次都不少。**
- 被同一把 key 一起管住的还有 `cache-bin`：`~/.cargo/bin` 里是 `dtolnay/rust-toolchain` 从 `static.rust-lang.org`
  装下来的**官方预编译** rustup 代理（不是在这台 runner 上编的），与 `target/` 同域、同一个 key，不引入额外风险。

#### 抓得住什么 / 抓不住什么

- 抓得住**跨镜像复用**：不同 runner 标签之间（`ubuntu-22.04` ↔ `ubuntu-latest`）不再共享 `target/`，两条恢复路径
  都断，判据就是上面那张表。
- **抓不住「同一个镜像标签、但底层镜像悄悄升级」**：`ubuntu-latest` 从 24.04 升到 26.04、或 22.04 打一次 glibc 补丁时，
  标签不变 ⇒ key 不变 ⇒ 旧缓存照样能恢复。这一档**没有自动化机制**，靠手动 bump：改 `prefix-key`（`v0-rust` →
  `v1-rust`）或改对应 `key:` 的字面量，一次性作废旧缓存。**别把这修法当成「以后不会再有这类问题」。**
- 顺带：`rust-toolchain.toml` 是 `stable`，rustc 一升版 env hash 就变 ⇒ **编译器升级自动冷启动**，这一档本来就不在
  风险里。

#### 代价与一次性影响

- CI 侧：缓存条数从「每 job 一条」变成「每 job × 每镜像一条」；三个单 OS job 各多一次冷编。
- 发布侧：只有 `windows-x64` 还能借 main 的热缓存；`linux-x64` / `linux-arm64` / `windows-arm64` / `macos-*`
  **每次发布都冷编** —— 这是断开 M269 那条路径的必然代价（22.04 在 main 侧没有任何产出者）。想省就按上面
  「在 main 上预热」那条做。
- 合入后所有旧 key 的缓存一次性失配，靠 LRU 自然淘汰；第一次 CI run 是冷缓存，别把它当成回归。
- 全仓第 8 处 `rust-cache` 在 `.github/workflows/dev-guards.yml:205`（job id `clippy`，只跑 `ubuntu-latest`；
  行号会随注释增减漂移，**钉的锚点是 job id `clippy` 里那一条 `Swatinem/rust-cache@v2`**），
  **本次刻意不动**：它唯一的 runner 就是 `ubuntu-latest`，job id 也不与发布侧 `build` 重名，今天不可能跨镜像相撞。
  副作用是它的 key 与 `ci/clippy` 从此不再逐字节相同（改前是相同的）⇒ 合入后 dev-guards 那次 clippy 会冷编一次，
  一次性、无害。

**没验的部分**：真实 Actions 上的缓存命中 / 未命中本地验不了 —— 本机只能证明 key 字符串（上面的演算）、YAML 仍被真
解析器接受（`python packaging/check_workflows_yaml.py`）、以及结构正确。「tag run 只能读 main / 同名 tag」这条来自
官方文档而不是观测；下一次 tag 正好顺手验三件事：`Cache restored from key:` 是否变成 `No cache found.`（linux-x64
应当是）、`Compiling aws-lc-sys` 是否重编、日志里还有没有 `__isoc23_*`。

## 4. 红的时候先看注解（M380）

job log 在匿名侧读不到（`GET /check-runs/<id>/log` → 403，M144 实证），**check-run 的注解是唯一可见正文**
（`GET /check-runs/<id>/annotations` → 200）。所以每条 `::error::` 都是给「下一个人」留下的现场，不是装饰。

### 4.1 `clippy` 作业：lint 正文升注解（M380）

改前那一步只有一行 `cargo clippy --workspace --all-targets -- -D warnings`，红时注解里只有
`Process completed with exit code 101` —— **知道红了，不知道红在哪**。改后：

- 与 `update-e2e.yml` / `test` 作业同一手法：`tee` 一份日志、取 `PIPESTATUS[0]`（cargo 的真码，不是 tee 的）、
  `%` 转 `%25`、失败时把 lint 头部行进 `::error::`；
- **先剥 ANSI，再 grep**（这是 r1 复审抓到的唯一 P1，别把它改回去）：本文件顶部 `env:` 有
  `CARGO_TERM_COLOR: always`，cargo 因此给 rustc/clippy 传 `--color=always`，**每一行诊断都以 SGR 转义开头**
  （`^[[1m^[[91merror^[[0m…`、` ^[[1m^[[96m--> …`）。用 `^` 锚定的正则对彩色行**一条都匹配不上** ⇒ 注解 0 条
  （r1 复审用真 cargo 彩色输出实测：不剥 = 0 条，剥 = 5 条）。所以链条最前面是
  `sed -E 's/\x1b\[[0-9;]*m//g' "$log"`，**必须在 grep 之前**：链条末尾那层 `sed` 在 grep 下游，改它没用。
  同一处修改也补到了 **`test` 作业的 Linux bash 步骤**（`:232` 附近，同一套锚定正则、同一个坑）；
- **顺序即优先级**：先 `error[...]` / `error:` 与它们的 `-->` 定位行（`head -30`），再 `warning:` 头部（`head -10`）。
  上一轮 `test (windows)` 的注解里全是 `--> Cargo.toml:93/95`（`unused workspace dependency` 这类**清单告警**的位置行；
  M391 已摘掉那两条依赖，这组位置行随之消失）——
  这类行在 clippy 这一步被 `grep -v Cargo.toml` 摘掉，否则它们会先吃光配额；
- **判据一个字没动**：命令仍是 `cargo clippy --workspace --all-targets -- -D warnings`，`--workspace` 仍不含
  `unlucky-app`（M118 的 exclude），`-D warnings` 仍是零 warning 门禁。改的只是「失败之后怎么报」；
- 两个平台共用**一个** `shell: bash` 步骤（两台 runner 都自带 Git Bash；本文件 build 作业的 PE 门禁、test 作业的
  截图步骤已是 bash-on-Windows 先例）。分平台写两份，只会得到两份各自漂移的判据。

行数账：`ci.yml` 改前 **371** 行、改后 **400** 行（红线 400，`guards.yml` 的 file-size 判据是 `-gt 400` 才红）。
`test (windows)` 那一步同步加了「失败用例名」注解（§4.2），两件事一起吃掉了全部余量 ⇒ **下一处想往 `ci.yml` 加东西
的人，先把一段纯函数/步骤搬出去**，别再往里挤。没有删任何空行、没有合行去凑数。

### 4.2 `test (windows)`：先注解「谁红了」（M380）

`--no-fail-fast` 下每个失败的 test binary 都会打一遍 `failures:` 段落，用例名是**裸行**（4 空格 + 名字），
旧的 `Select-String` 模式只认 `failures:` 这个**标题行**，段落里的用例名一条都匹配不上；
再加上 `Select-Object -First 40`，40 条配额在到达那个段落之前就被告警定位行吃掉了。改后先按段落把名字捞出来：

- **抓得住**：每个失败 binary 的 `test result: FAILED. …` 汇总行，以及 `failures:` 段落里**逐条用例名**
  （`--no-fail-fast` 下有第二条失败也照收）；名字排在正文之前 ⇒ 配额再紧也先有「谁红了」。
- **抓不住**：① 进程**被杀**（超时 / OOM / runner 掉线）时日志里根本没有 `test result:` 行，此时只剩
  `cargo exit=<码>`——那正是「退出码给了、诊断没给」的那一档，注解救不了；② 装配/编译失败（test binary 没跑起来）
  时没有 `failures:` 段落，靠的是原有的 `error:` 那支；③ 用例自己打出来的 `assert_eq!` 左右值**不在**注解里
  （只注解名字与 `panicked at` 行），要细节仍得读日志；④ `Select-Object -First 40` 与注解条目的显示上限仍然存在——
  顺序即优先级，别指望「全量」。
- 截至 M380，**没有任何失败用例名**可用于解释上一轮 `test (windows)` 为什么红：本轮只补取证，
  **不猜**是哪个用例、也不写成「某个 flake」。

### 4.3 `update-e2e`：`sign_manifest.py` 的 cp1252 事故（M380）

**根因（塔已定位，本机复核过）**：`packaging/sign_manifest.py` 的 `print(f"signed: {manifest} → {out}")` 把 U+2192
写到 Windows runner 的 **cp1252** 控制台/管道 ⇒ `UnicodeEncodeError: 'charmap' codec can't encode character '\u2192'`
⇒ 脚本非零退出 ⇒ `crates/cli/tests/support/update_e2e_sign.rs` 的「签名脚本失败」断言炸。本机是 cp936，`→` 编得出来，
所以**本机跑一万次都是绿的**——本机绿从来不等于修好了。

修法（源头，不改调用方）：`main()` 开头把 `sys.stdout` / `sys.stderr` 的**错误策略**定死成 `backslashreplace`
（编不出来的字符转义、绝不抛），与仓里另外几个 packaging 脚本同手法；**不靠调用方设 `PYTHONIOENCODING` 兜底**。
`→` 这个字符本身**故意留着**：它是「这类 bug」的活样本，改成 ASCII 只能掩盖这一行，掩盖不了同一批路径上其余
几十处中文输出（`--check-pinned-keyring` 的失败文案全是中文）。

复现与验证（`PYTHONIOENCODING=cp1252` 是本机唯一能造出 cp1252 的手段）：

```
# 改前：EXIT=1，UnicodeEncodeError: 'charmap' codec can't encode character '\u2192'
PYTHONIOENCODING=cp1252 python packaging/sign_manifest.py --sign <manifest>
# 改后：EXIT=0，同一行输出 `signed: … \u2192 …`（cp1252 编不出来的字符转义成 `\uXXXX`，编得出来的原样出字节）
```

`crates/cli/tests/support/update_e2e_sign.rs` 里**刻意不设** `PYTHONIOENCODING` / `PYTHONUTF8`：这条 e2e 用例的
子进程 stdout 本来就是**管道**，在 en-US 的 Windows runner 上就是 cp1252 ⇒ 它顺带是「脚本不许在 cp1252 下崩」的
回归探针。给调用方加环境变量会把探针拆掉。

`packaging/**.py` 里其余非 ASCII 的 stdout 输出（M380 按 AST 取 `print(...)` 与 `help=` 逐个文件扫过一遍）：
今天**只在 `ubuntu-latest` 上跑**的几个守门脚本（`check_screenshot` / `check_signing_ready` / `check_test_key_off` /
`check_test_source_off` / `check_workflows_yaml` / `make_manifest`）**没有**这层加固 —— Ubuntu runner 是 UTF-8 locale，
今天不会炸；一旦谁把它们挪到 Windows runner，`--help` 都过不去（本机 `PYTHONIOENCODING=cp1252 … --help` 实测崩 5 个）。
这是**已知欠账**，不在 M380 的 scope 里。

### 4.4 本地验「注解逻辑」的正确姿势（M380 r1 的教训）

假日志**必须带颜色**。做法：用 `CARGO_TERM_COLOR=always` 跑一次真 `cargo clippy`（玩具 crate 里放一条
`clippy::needless_bool` 就够，退出码 101），把 stderr 存成日志；再把 `ci.yml` 那一步的 `run:` **原样抽出来**
（PyYAML 取 `jobs.clippy.steps[*].run`），用一个只 `cat` 这份日志并 `exit 101` 的 stub `cargo` 跑一遍，数
`::error::` 的行数。对照组 = 同一份日志 `sed -E 's/\x1b\[[0-9;]*m//g'` 之后（必须先剥再 grep）。
判据是「彩色输入与纯文本输入的注解条数相同，且退出码都是 101」。假日志不带 ANSI 时，「0 条注解」在本地
**看不见** —— M380 第一轮就是这么漏掉这个 P1 的。

**本次没有观察到 CI 变绿**：上面三条全部是「下一次在真实 runner 上跑才知道」的东西（clippy 的 lint 到底是什么、
`test (windows)` 到底红在哪个用例、`update-e2e` 是否真从红转绿）。
