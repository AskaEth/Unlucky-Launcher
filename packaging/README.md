# 打包与发布工程（packaging/）

单一真相源：**单文件产物矩阵 + 发布契约**。`.github/workflows/release.yml` 按本文实现，
`packaging/make_artifacts.py` 是本地与 CI **共用**的打包脚本（同一份代码 → 同一套目录布局）。
本目录只放发布工程资产（脚本 / 说明 / 发布说明模板），不参与任何 crate 的编译。

## 1. 产物矩阵

| 槽位 | 产物文件名 | 形态 | 归档内容 |
|------|-----------|------|---------|
| `windows-x64` | `unlucky-launcher-<ver>-windows-x64.exe` | **裸 exe**（双击即用） | exe 单文件；三份合规文本作为并列 Release 资产 |
| `linux-x64` | `unlucky-launcher-<ver>-linux-x64.tar.gz` | 单二进制归档 | `unlucky-launcher` + `LICENSE` + `NOTICE` + `README.md` |
| `macos-arm64` | `unlucky-launcher-<ver>-macos-arm64.tar.gz` | 单二进制归档 | 同上 |
| `macos-x64` | `unlucky-launcher-<ver>-macos-x64.tar.gz` | 单二进制归档 | 同上 |
| 全平台 | `SHA256SUMS.txt` | 校验清单 | 每行 `<sha256>␠␠<文件名>`（`sha256sum -c` 可直吃） |

约定细节：

- **"单文件"指可执行文件本身零运行时依赖**（无 WebView、无 JRE、无 sidecar 资源目录、无 DLL 旁挂）。
  归档里额外放三份合规文本是许可义务要求，不是运行时依赖；Windows 因为"一个 exe 装不下文本"，
  把 `LICENSE` / `NOTICE` / `README.md` 作为 Release 的并列资产分发，条款同样随下载走。
- 归档解压后是**一个顶层目录** `unlucky-launcher-<ver>-<槽位>/`，用户不会解压出一堆散文件。
- `<ver>` 不带前导 `v`（tag 是 `v0.1.0`，文件名是 `0.1.0`）——由脚本统一裁剪。
- UNIX 归档内 `mtime`/`uid`/`gid` 归零、权限固定（二进制 0755、文本 0644），
  同源构建的 sha256 不受打包时刻影响。

## 2. 版本来源（唯一真相源）

版本号只有一个来源：**`Cargo.toml` 的 `[workspace.package] version`**。

- `<ver>` 由 `make_artifacts.py` 通过 `cargo metadata --no-deps` 读出（不是正则扒文件），
  四个 workspace member 版本不一致时直接失败。
- 带 tag 构建时（CI）额外传 `--version "$TAG"`：脚本裁剪 `v` 后与 `Cargo.toml` 交叉校验，
  **不一致即失败**——避免出现 tag 说 0.2.0、产物里写 0.1.0 的发布事故。
- `ull --version` 必报同一版本（脚本打包前跑一次冒烟 `ull --version` 断言版本串在其中），
  这是 `env!("CARGO_PKG_VERSION")` 的实际输出，不是估算。
- 发布流程：先改 `Cargo.toml` 版本 + 提交 → `git tag v<ver>` → 推送 tag 触发流水线。

## 3. 怎么构建

### 本地（当前平台）

```bash
cargo build --release --locked                 # 产出 target/release/{unlucky-launcher,ull}
python packaging/make_artifacts.py --out dist  # 产物落在 dist/，含 SHA256SUMS.txt
python packaging/make_artifacts.py --version v0.1.0   # 顺带校验版本一致性
python packaging/make_artifacts.py --no-smoke         # 跳过 ull --version 冒烟
```

脚本按宿主自动判定槽位（Windows x64 / Linux x64 / macOS arm64 / macOS x64），
其它组合直接报错退出——不在矩阵里的平台不出"凑合能跑"的产物。

### CI（三平台矩阵）

`.github/workflows/release.yml`：

- 触发：`push` tag `v*`；`workflow_dispatch` 只构建不发布（验证矩阵仍编得过）。
  `inputs.prerelease` **只在 tag ref 上 dispatch 才可达**——Actions 页面的 "Run workflow" 只能选分支，
  要跑 tag 得 `gh workflow run release.yml --ref v0.1.0`。分支 ref 的 dispatch 与 push 一样被
  `release` job 的 `if: github.ref_type == 'tag'` 挡在发布之外，所以「dispatch 只构建不发布」
  与「dispatch 能关 pre-release」是两条互不冲突的规则。
- 矩阵：`windows-latest`（MSVC + `-C target-feature=+crt-static`）、`ubuntu-22.04`（glibc 2.35 地板）、
  `macos-latest`（arm64）、`macos-15-intel`（Intel x64；`macos-13` 已下线，`macos-26-intel` 为更新替位）。
- 每个槽位 `cargo build --release --locked` → `make_artifacts.py` → 归档内容自检 → 上传 artifact。
- `release` job：下载全部 artifact → 重算一份全量 `SHA256SUMS.txt` → 用 `packaging/release-notes.md`
  模板渲染支持矩阵 → `gh release create`（**pre-release 默认开**：首建时 `push` 触发按 pre-release，`workflow_dispatch` 可关）。
- **同一 tag 重跑幂等**：`gh release view` 命中已存在的 Release 时改走 `gh release upload --clobber`
  + `gh release edit`，覆盖资产与说明而不是再 `create`（`create` 遇已存在会失败，把流水线最后一步染红）。
- **pre-release 标记只按显式输入断言**：`gh release edit` 分支在默认重跑（`push` 触发的 Re-run，
  `inputs.prerelease` 为空）时**不传** `--prerelease`，所以重跑**不会**翻转标记；只有 tag ref 上的
  `workflow_dispatch` 显式给了输入才断言——`prerelease=true` → `--prerelease`，`prerelease=false` →
  `--prerelease=false`。要改标记两条路等价且互不打架：① 走流水线
  `gh workflow run release.yml --ref v0.1.0 -f prerelease=false`（会落到 edit 分支显式断言）
  ② 直接 `gh release edit <tag> --prerelease=false`（不经流水线；后续默认重跑不碰该标记，
  不会被翻回去）。首建（`create`）仍保持「pre-release 默认开」的老契约，空输入按 `true` 处理。

Windows 为什么用 MSVC 静态 CRT 而不是本机这套 GNU 工具链：两者都属"标准工具链"（D21），
但发布产物要**不依赖 VC++ 可再发行包**，`+crt-static` 把 CRT 静态链进 exe，Win10+ 的 UCRT 本身是
系统组件，于是 exe 真正单文件可拷走。GNU 交叉产物在 CI 上还要拉 MinGW 链接器，收益为零。

## 4. 每平台运行时依赖

### Windows x64（Win10/11）

| 依赖 | 来源 | 说明 |
|------|------|------|
| `KERNEL32` / `USER32` / `GDI32` / `ADVAPI32` / `SHELL32` / `ole32` / `WS2_32` … | 系统 | Win10+ 一律自带 |
| `ucrtbase` / `api-ms-win-crt-*`（UCRT） | 系统 | Win10+ 内置，**不需要** VC++ 可再发行包 |
| MSVC 运行时（`VCRUNTIME140` / `MSVCP140`） | **无** | `+crt-static` 静态链接，故不需要可再发行包 |
| OpenGL（`opengl32`） | 系统 | 只在 GL 渲染路径用；缺失走软渲染 |

本机（GNU 工具链）实测导入表证据见 §7。

### Linux x64

| 依赖 | 说明 |
|------|------|
| glibc ≥ 2.35 | 由 `ubuntu-22.04` 构建决定；更老的发行版请自行重建或等 musl 版 |
| `libxkbcommon.so.0` | winit 键盘处理 |
| `libfontconfig.so.1` + `libfreetype.so.6` | 文本渲染 |
| `libGL.so.1` / `libEGL.so.1` | femtovg GL 渲染；缺失走软渲染 |
| `libxcb.so.1` / `libX11.so.6` | X11 后端（Wayland 会话下也需 XWayland 相关库） |
| `libwayland-client.so.0` | Wayland 后端 |

Debian/Ubuntu 一行：`sudo apt install libxkbcommon0 fontconfig libgl1 libegl1 libxcb1 libx11-6 libwayland-client0`。
运行期**不需要** `-dev` 包、不需要 Java（Java 是启动游戏时才由启动器自己管理的东西）。

### macOS 12+

| 依赖 | 说明 |
|------|------|
| 系统框架（Cocoa / Metal / IOKit / Security …） | 由操作系统提供，无第三方动态库 |
| `libSystem` | 系统 |

macOS 产物是**裸二进制**而非 `.app` bundle：符合单文件承诺，但没有 Dock 图标、Finder 双击不生效，
从终端启动（README 里给了 `xattr -d com.apple.quarantine` 的 Gatekeeper 解法）。
签名/公证（需 Apple Developer 账号）与 `.app` 打包是后续工作，不做假承诺。

## 5. 软渲染兜底（无 GPU / 老驱动 / 远程桌面）

Slint 默认 `renderer-femtovg`（GL）。两条兜底路径，效果等价，任选其一：

```bash
unlucky-launcher --software-renderer                     # 启动参数（本仓库解析后设 SLINT_BACKEND）
SLINT_BACKEND=winit-software unlucky-launcher            # 环境变量（Slint 原生开关）
```

- 参数解析在 `crates/app/src/main.rs` 的 `apply_renderer_flag()`，只认完全相等的 `--software-renderer`。
- 软渲染是 CPU 光栅化，无 GPU 也能出图；代价是动画与大窗口重绘更慢（Metro 纯色界面影响有限）。
- 验收/无交互桌面场景另有进程内截图通道：`--screenshot <png>`（见 `crates/app/src/screenshot.rs`），
  它同样能在软渲染下工作，是 CI 与远程机唯一可靠的"真出图"证据。
- 发布冒烟（本机实跑，见 §7）就是用 `--software-renderer --screenshot` 组合证明"无 GPU 也能起来"。

## 6. Cargo 体积优化（release profile）

`Cargo.toml`：

```toml
[profile.release]
opt-level = "z"      # 体积优先（启动器不是算力密集；UI 走 Slint 渲染，热路径不在 Rust 侧）
lto = "fat"          # 全量 LTO：跨 crate 内联+死代码消除，比 thin 再小一截（实测见 §7）
codegen-units = 1    # 单 CGU，配合 LTO 拿最大优化面
panic = "abort"      # 去掉 unwind 表
strip = "symbols"    # 剥符号表（GNU/MSVC 路径都生效）
```

取舍：`lto = "fat"` + `codegen-units = 1` 让 release 链接明显变慢（本机全量 release 构建 10 分钟量级，
见 §7.1），换来的是体积与启动时间。变慢的根源在 `codegen-units = 1`：整个 crate 只切出一个 CGU，
LTO 也就只有一个线程可跑（`fat` 本身只管"跨 crate 全量参与"，不等于单线程）。
本地开发迭代走 `cargo build`（dev profile）不受影响，但 **`ci.yml` 每次 push/PR 都跑
`cargo build --workspace --release`**，会实打实吃这份 fat LTO 的链接时间——CI 慢的一部分是 release
档构建，不是测试本身。
`panic = "abort"` 意味着 panic 不带 backtrace 栈展开——UI 层已用 `Result` 承接错误，
`unwrap` 只出现在测试里，这是可接受的取舍。

## 7. 实测（本机，不是估算）

构建环境：rustc 1.99.1 stable（host `x86_64-pc-windows-gnu`，MinGW 链接器），
`cargo build --release --locked --bin unlucky-launcher --bin ull`，12 核机器（同一时刻另有别的 worker
在做全量 debug 构建，属于偏悲观的真实负载）。产物：`target/release/unlucky-launcher.exe`（GUI，交付物）
与 `target/release/ull.exe`（CLI）。

### 7.1 体积（实测字节数，不是估算）

| release profile | `unlucky-launcher.exe` | `ull.exe` | 冷构建耗时（本机 12 核） |
|-----------------|-----------------------|-----------|-----------|
| `lto = "thin"`（改前） | 16,366,080 B = 15.61 MiB | 7,572,992 B = 7.22 MiB | 11m52s |
| `lto = "fat"`（改后，本仓库采用） | **14,595,584 B = 13.92 MiB** | **6,157,824 B = 5.87 MiB** | 9m27s（增量，依赖已编好） |

`fat` 相对 `thin` 的实测差：GUI **−1,770,496 B（−10.8%）**，CLI **−1,415,168 B（−18.7%）**。
一次全量 LTO 把跨 crate 的死代码与重复实例化一起消掉，收益远超常见印象里的"1~2%"；
代价是 release 链接更慢——慢在 `codegen-units = 1`（单 CGU → LTO 单线程），不是 `fat` 本身，
且只影响发布/验收构建，日常 `cargo build` 走 dev profile 不受影响。

体积构成（按依赖树判断）：Slint 运行时（femtovg + software 双渲染器 + winit/accesskit/托盘）+ rustls/reqwest
+ rig/rmcp/axum 是主要来源；`ull` 明显更小是 D18 纪律的直接结果（CLI 零 UI 框架依赖）。

### 7.2 冒烟（真跑，2026-10-31 夜间 + 11-01 早，本机 Win11 26100）

| 命令 | 实测结果 |
|------|---------|
| `ull.exe --version` | 输出 `ull 0.1.0（unlucky-cli）` + `os: Windows 11 · 10.0.26100（x86_64）· 满足最低要求`，exit 0 —— 版本与 workspace `0.1.0` 一致 |
| `unlucky-launcher.exe --software-renderer --screenshot … --data-dir …` | exit 0，产出 1180×760 PNG（3,588,298 B）；看图确认 Metro 实例页（侧栏/搜索/空态/AI 助手面板）正常渲染 → **软渲染路径真的能出图**，不依赖 GL |
| `unlucky-launcher.exe --version` | **无输出**，8 秒后被 timeout 杀掉（exit 124）——GUI 目前不解析 `--version`，会直接开窗口。已作为 finding 上报，见 §10-1 |

### 7.3 打包验证（本机 Windows 槽位真跑）

```
smoke: ull.exe --version -> 0.1.0 (ok)
artifact: unlucky-launcher-0.1.0-windows-x64.exe  13.92 MiB  sha256=d4954c62…c3570
sums:     SHA256SUMS.txt
$ sha256sum -c SHA256SUMS.txt
unlucky-launcher-0.1.0-windows-x64.exe: OK
```

版本守卫也实测过：`--version v9.9.9` 时脚本以 `tag/version mismatch: requested 9.9.9, Cargo.toml says 0.1.0` 失败退出（exit 1）。

UNIX 归档的可复现性单独取证（本机 Windows 槽位只走裸 exe 分支，tar 路径要另测）：
reviewer-packaging 在 M46 review 里连续调 `stage_archive()` 两次 → **两次 sha256 完全一致**
`a99d6e9e727cd7e390434c566a15978451f340858fa89836e2f63790bbc392f8`；成员 `mtime=0`、
`uid=gid=0`、`uname/gname` 为空、`mode` 0755（二进制）/ 0644（文本），gzip header 的 mtime 字段是
`00000000`，归档内单顶层目录且含 `LICENSE`/`NOTICE`/`README.md`。
即"同源构建 → 同 sha256"在 tar 路径上成立，不随打包时刻漂移（见 §1 的约定细节）。

### 7.4 运行时依赖证据（PE 导入表）

`objdump -p target/release/unlucky-launcher.exe` 共 42 条 DLL 导入记录、33 个不同名字（大小写各记一次），全部是 Windows 自带：
`KERNEL32/USER32/GDI32/ADVAPI32/SHELL32/SHLWAPI/ole32/oleaut32/combase/comctl32/dwmapi/dwrite/imm32/
uxtheme/uiautomationcore/opengl32/ws2_32/bcrypt/bcryptprimitives/ntdll` + 一组 `api-ms-win-crt-*`（UCRT API set）。

**没有** `VCRUNTIME140.dll` / `MSVCP140.dll` 这类 MSVC 可再发行包依赖，也没有 `libwinpthread-1.dll` /
`libgcc_s_*.dll` 之类 MinGW 运行时 —— 即"拷走一个 exe 就能跑"（Win10+ 自带 UCRT）。
CI 的 MSVC `+crt-static` 产物同理（静态 CRT）；`release.yml` 里用 `dumpbin -dependents` 把这条变成硬门禁，
两侧都 fail、没有降级分支：**出现 VCRUNTIME/MSVCP/ucrtbase 就 fail**，**找不到 dumpbin 同样 fail**
（`windows-latest` 自带 VS 工具链，基本找得到；"找不到就跳过"等于让门禁在最需要它的时候失效）。
总之不允许"悄悄要用户装可再发行包"。
注意：本机只装了 GNU 工具链，MSVC 那个槽位的产物本身**没有在本机验证过**，由 CI 首跑验证。

## 8. 发行合规

- `LICENSE`（Apache-2.0 全文）与 `NOTICE`（含 Slint Royalty-Free 2.0 义务说明）随包分发，
  三份文本在 UNIX 归档内、在 Windows 侧作为并列 Release 资产。
- Slint 的 RF 2.0 义务：关于页保留 `AboutSlint` 部件（已在 UI 内），不得移除；
  选 RF 分支即不触发 GPL 分支义务（见 `deny.toml` 的许可证白名单注释）。
- `README.md`（英文主版）随包分发；中文版 `README.zh-CN.md` 留在仓库不进包（避免包体噪音）。
- 依赖许可门禁不变：`deny.toml` 白名单（Apache-2.0 / MIT / BSD / Zlib / BSL-1.0 / ISC / CC0 / Unlicense /
  Unicode-3.0 / CDLA-Permissive-2.0 / Slint-RF-2.0）。发布任务不新增任何依赖。
- 发布产物不含游戏本体、不含 JRE、不含任何账号数据；`ull`/GUI 的数据目录在用户侧生成。

## 9. 支持矩阵（对外口径，与 release-notes 模板逐行一致）

| 平台 | 级别 | 运行时要求 |
|------|------|-----------|
| Windows 10/11 x64 | 一等 | 无（UCRT 系统自带） |
| macOS 12+（arm64 / x64） | 一等 | 无第三方库；未签名需去 quarantine |
| Linux x64 X11 | 一等 | glibc 2.35+、xkbcommon、fontconfig、libGL、xcb |
| Linux x64 Wayland | 支持 | 上述 + wayland-client |
| Windows 8.1/8/7 | 不支持 | 基线已上调（D21） |
| Android Termux + X11 | 试验级（不承诺） | 仅 CLI（源码自建），GUI 未验证，不在本发布矩阵内 |

`packaging/release-notes.md` 的 "Support matrix" 表与上表逐行对应（英文版，Termux 行同样标注
experimental / not part of this release matrix）——改一边必须同步另一边，否则标题里的"一致"就是假话。

## 10. 已知边界 / 后续

1. **GUI 无 `--version`**：`unlucky-launcher.exe` 目前只解析 `--software-renderer` / `--screenshot*` /
   `--data-dir` / `--mock`，`--version` 会当成普通参数忽略并直接开窗口。版本自证目前靠：
   ①产物名里的 `<ver>` ②`ull --version`（同 workspace 版本）③tag 交叉校验。
   加一个 `--version` 分支属于 `crates/app` 范围，已作为 finding 上报，不在本 mission 内改。
2. **签名/公证**：Windows 无 Authenticode、macOS 无 codesign/notarize（都要证书/开发者账号），
   当前以 SHA256SUMS + Release 说明作为完整性参考。
3. **macOS `.app` bundle / `.dmg`**：与单文件承诺冲突，暂不做；若用户要 Dock 图标再单开 mission。
4. **Linux musl 静态版**：可彻底摆脱 glibc 地板，但 Slint 在 musl 上需额外验证，未纳入本矩阵。
5. **`ull` CLI 未随包**：本次矩阵只发 GUI 单文件（用户核心卖点是启动器本体）；headless 用户可从
   源码 `cargo build --release --bin ull`，或在后续把 `ull` 加进同一矩阵（脚本已按槽位参数化，改动很小）。
6. **本机 GNU 链接器的已知无害告警**：`cargo build --release`（GNU 工具链）会打一行
   `warning: linker stderr: corrupt .drectve at end of def file`，来自 MinGW `ld` 对 `.drectve` 段的
   老式抱怨，不影响产物（体积/导入表/冒烟均已实测正常）；CI 的 MSVC 槽位不会出现这条。

## 11. 文件清单

| 文件 | 作用 | 行数上限 |
|------|------|---------|
| `packaging/README.md` | 本文：矩阵/依赖/版本/合规契约 | ≤400 |
| `packaging/make_artifacts.py` | 打包与校验（本地 + CI 共用） | ≤400 |
| `packaging/release-notes.md` | GitHub Release 说明模板（`{{VERSION}}` / `{{SUMS}}` 占位） | ≤400 |
| `.github/workflows/release.yml` | tag 触发 → 矩阵构建 → Release | ≤400 |
