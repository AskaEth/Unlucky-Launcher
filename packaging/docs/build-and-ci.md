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
  结果 `LIB`/`INCLUDE` 全空（M164 复审实测；`cmd //c` 只调**不含引号**的路径是好的）。故本步往 `$RUNNER_TEMP`
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
