# 打包与发布工程（packaging/）

单一真相源：**单文件产物矩阵 + 发布契约**。`.github/workflows/release.yml` 按本文实现，
`packaging/make_artifacts.py` 是本地与 CI **共用**的打包脚本（同一份代码 → 同一套目录布局）。
本目录只放发布工程资产（脚本 / 说明 / 发布说明模板），不参与任何 crate 的编译。

## 1. 产物矩阵

| 槽位 | 本版发布 | 产物文件名 | 形态 | 归档内容 |
|------|---------|-----------|------|---------|
| `windows-x64` | ✅ 发布 | `unlucky-launcher_<ver>_windows_x64.exe` | **裸 exe**（双击即用） | exe 单文件；三份合规文本作为并列 Release 资产 |
| `windows-arm64` | ❌ 不发布（构建照跑取证） | `unlucky-launcher_<ver>_windows_arm64.exe` | **裸 exe**（同 x64 策略） | 同上；CI 仍在**原生** `windows-11-arm` runner 上构建（M140 起不交叉），**红了不挡 x64 发布**（M142，见 §3） |
| `linux-x64` | ❌ 不构建（未验证） | `unlucky-launcher_<ver>_linux_x64.tar.gz` | 单二进制归档 | `unlucky-launcher` + `LICENSE` + `NOTICE` + `README.md` |
| `linux-arm64` | ❌ 不构建（未验证） | `unlucky-launcher_<ver>_linux_arm64.tar.gz` | 单二进制归档（原生 ARM64 runner 构建） | 同上 |
| `macos-arm64` | ❌ 不构建（未验证） | `unlucky-launcher_<ver>_macos_arm64.tar.gz` | 单二进制归档 | 同上 |
| `macos-x64` | ❌ 不构建（未验证） | `unlucky-launcher_<ver>_macos_x64.tar.gz` | 单二进制归档 | 同上 |
| 每槽位 | 随所属槽位 | `ull_<ver>_<平台>_<架构>[.exe]` | **裸 CLI 文件**（独立资产，不进 tar.gz） | 零 UI 依赖的 `ull` 本体，进同一份 `SHA256SUMS.txt` |
| 全平台 | 仅已发布资产 | `SHA256SUMS.txt` | 校验清单 | 每行 `<sha256>␠␠<文件名>`（`sha256sum -c` 可直吃） |

约定细节：

- **「不构建」= CI 默认根本不跑**（M139）：那四档未验证，产物不给用户、构建配额也不花；要整表六槽位取证走
  `gh workflow run release.yml -f all_slots=true`（§3）。发布白名单**只有 `windows-x64`**（M142：arm64 照构建、不发布）。
- **产物命名**：`<名字>_<版本号>_<平台>_<架构>`，下划线分段、**不带实现细节字样**（不出现 `iced`）；真相源是 `make_artifacts.py::asset_stem()`。
- **"单文件"指可执行文件本身零运行时依赖**（无 WebView、无 JRE、无 sidecar、无 DLL 旁挂）；归档里的三份合规文本是许可义务、不是运行时依赖，Windows 因为"一个 exe 装不下文本"，
  把 `LICENSE` / `NOTICE` / `README.md` 作为 Release 的并列资产分发，条款同样随下载走。
- 归档解压后是**一个顶层目录** `unlucky-launcher_<ver>_<平台>_<架构>/`，用户不会解压出一堆散文件。
- **`ull` 是并列的裸资产，不塞进 tar.gz**：headless 用户要的就是「下一个小文件直接跑」；归档里放
  LICENSE/NOTICE/README.md 是许可义务，而这三份本来就作为并列 Release 资产分发，裸 `ull` 不缺条款。
- `unlucky-launcher` 本体也认全部 CLI 子命令（M82），`ull` 不是第二个界面，而是**不链接图形栈**的同一套命令（差异见 §4 与 §7.4），两者不是重复品。
  M118：GUI 的**构建产物**名是 `unlucky-launcher-iced`（`crates/app` 已被 workspace `exclude`，不再产出 `unlucky-launcher`）；对外文件名仍是 `unlucky-launcher`，改名在 `make_artifacts.py` 落盘时（`LAUNCHER_BIN` 找产物 / `LAUNCHER_ASSET` 定对外名），所以下载链接 / `SHA256SUMS` / 本文矩阵不变。
- `<ver>` 不带前导 `v`（tag 是 `v0.2.1`，文件名是 `0.2.1`）——由脚本统一裁剪。
- UNIX 归档内 `mtime`/`uid`/`gid` 归零、权限固定（二进制 0755、文本 0644），
  同源构建的 sha256 不受打包时刻影响。
- **架构不靠猜**：脚本打包前回读二进制的可执行文件头（PE 机器码 / ELF `e_machine` / Mach-O
  `cputype`），与槽位声明的架构不符即失败——不会出现"名字是 arm64、内容还是 x64"的假产物。

## 2. 版本来源（唯一真相源）

版本号只有一个来源：**`Cargo.toml` 的 `[workspace.package] version`**。

- `<ver>` 由 `make_artifacts.py` 通过 `cargo metadata --no-deps` 读出（不是正则扒文件），
  任何一个 workspace member 版本不一致时直接失败。
- 带 tag 构建时（CI）额外传 `--version "$TAG"`：脚本裁剪 `v` 后与 `Cargo.toml` 交叉校验，
  **不一致即失败**——避免出现 tag 说 0.2.1、产物里写 0.2.0 的发布事故。
- `ull --version` 必报同一版本（脚本打包前跑一次冒烟 `ull --version` 断言版本串在其中），
  这是 `env!("CARGO_PKG_VERSION")` 的实际输出，不是估算。
- 发布流程：先改 `Cargo.toml` 版本 + 提交 → `git tag v<ver>` → 推送 tag 触发流水线。

## 3. 怎么构建

### 本地（当前平台）

```bash
cargo build --release --locked                 # 产出 target/release/{unlucky-launcher-iced,ull}（M118：Slint 版不再进 workspace）
cargo build --profile fast --bins              # 只是想快点看到画面/跑测试：快档（§6），产物在 target/fast/
python packaging/make_artifacts.py --out dist  # 产物落在 dist/，含 SHA256SUMS.txt
python packaging/make_artifacts.py --version v0.2.1   # 顺带校验版本一致性
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
  才可达**——Actions 页面的 "Run workflow" 只能选分支，要跑 tag 得 `gh workflow run release.yml --ref v0.2.1`。
  分支 ref 的 dispatch 与 push 一样被 `release` job 的 `if: github.ref_type == 'tag'` 挡在发布之外，所以
  「dispatch 只构建不发布」与「dispatch 能关 pre-release」是两条互不冲突的规则。
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
  模板渲染支持矩阵 → `gh release create`（**pre-release 默认开**：首建时 `push` 触发按 pre-release，`workflow_dispatch` 可关）。
- **同一 tag 重跑幂等**：`gh release view` 命中已存在的 Release 时改走 `gh release upload --clobber`
  + `gh release edit`，覆盖资产与说明而不是再 `create`（`create` 遇已存在会失败，把流水线最后一步染红）。
- **pre-release 标记只按显式输入断言**：`gh release edit` 分支在默认重跑（`push` 触发的 Re-run，
  `inputs.prerelease` 为空）时**不传** `--prerelease`，所以重跑**不会**翻转标记；只有 tag ref 上的
  `workflow_dispatch` 显式给了输入才断言（`prerelease=true` → `--prerelease`，`false` → `--prerelease=false`）。
  要改标记两条路等价且互不打架：① 走流水线 `gh workflow run release.yml --ref v0.2.1 -f prerelease=false`
  （会落到 edit 分支显式断言）② 直接 `gh release edit <tag> --prerelease=false`（不经流水线；后续默认重跑
  不碰该标记，不会被翻回去）。首建（`create`）仍保持「pre-release 默认开」的老契约，空输入按 `true` 处理。

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

### Windows ARM64（Win11 on ARM）

**本版不发布**（CI 照构建取证，红了不挡 x64 发布；等它绿了把键加回 `PUBLISH_SLOTS` 即可）。产物形态与门禁跟 x64
一样（裸 exe + 静态 CRT + dumpbin 导入表硬门禁），差别只在**怎么编**：**M140 起在原生 `windows-11-arm` 上编**
（GitHub 文档 2026 的 Standard runners 表明列该标签：4C/16G、公开仓库免费），产物落在 `target/release`。原路线
是在 `windows-latest` 上交叉到 `aarch64-pc-windows-msvc`，但**CI 上从未成功过一次**（v0.2.1 首次真跑即红在
「ARM64 工具链体检」）。M142 已确认那几轮「只剩一行 exit code 1」是**诊断脚本自己先死**（`set -uo pipefail` 关不掉
runner 注入的 `-e`，`find` 扫不存在的根被 pipefail 判非零就退出），已修；ARM64 的静态库在这台 runner 上到底缺不缺
要看下一次运行的新注解（固定 4 行实况清单，见 §3）。原生的附赠好处：宿主=产物架构，`ull --version` 冒烟**真跑**。

运行时依赖与 x64 同表（UCRT 是 Win11 ARM 的系统组件，`+crt-static` 静态链 CRT，不需要可再发行包）。
游戏侧如实说：**启动器原生运行**；Temurin 没有 windows-aarch64 产物，Microsoft OpenJDK 有该
发行版（core 的 `microsoft` 源按 `<os>-<arch>` 拼名，命名已覆盖），但 Minecraft 本体没有
Windows ARM64 原生库（Mojang/LWJGL 只发 x64/x86），实际游玩仍走系统 x64 模拟层 + x64 JDK。
所以本槽位只承诺「启动器产物」支持，不替 Java/游戏管理做架构级承诺。

### Linux x64

| 依赖 | 说明 |
|------|------|
| glibc ≥ 2.35 | 由 `ubuntu-22.04` 构建决定（该槽位默认不编，`all_slots=true` 取证跑才有产物）；更老的发行版请自行重建或等 musl 版 |
| `libxkbcommon.so.0` | winit 键盘处理 |
| `libfontconfig.so.1` + `libfreetype.so.6` | 文本渲染 |
| `libGL.so.1` / `libEGL.so.1` | Slint 版：femtovg 的 GL 渲染；iced 版：wgpu 的 GL/EGL 后端（**推断，未实测**）。两者缺失都退到 CPU 软渲染（tiny-skia） |
| `libxcb.so.1` / `libX11.so.6` | X11 后端（Wayland 会话下也需 XWayland 相关库） |
| `libwayland-client.so.0` | Wayland 后端 |

Debian/Ubuntu 一行：`sudo apt install libxkbcommon0 fontconfig libgl1 libegl1 libxcb1 libx11-6 libwayland-client0`。
运行期**不需要** `-dev` 包、不需要 Java（Java 是启动游戏时才由启动器自己管理的东西）。

**M118（D26）口径**：除上表里标注「推断，未实测」的那一行外，这张表与下面这条结论都是 **Slint 版 GUI** 的实测记录（M82：`readelf -d` + `ldd` + `nm -D`）；iced 版（`crates/app-iced`）的 `DT_NEEDED` / `dlopen` 集合**尚未重测**，请按「同一族库、细节待验」读，别当成 iced 版的实测承诺。
仍然成立的部分：上表是**开窗**要用的库，不是进程启动门槛——实测（§7.4，Slint 版）除 `libfontconfig.so.1` 外它们**都不在** ELF 的 `DT_NEEDED` 里，由 `dlopen` 在开窗那一刻才加载，所以没有 X11 / Wayland / GL 的机器上 `unlucky-launcher instances list` 照样跑得起来，`ull` 连 fontconfig 都不需要。
arm64 侧同理（同一份依赖树；历史口径里的「winit + femtovg」中 femtovg 只属于 Slint 版）。

### Linux arm64（aarch64）

依赖与 x64 完全同一套（`libxkbcommon` / `fontconfig` / `libGL` / `libxcb` / `libwayland-client`），
**glibc 地板也相同**：arm64 槽位跑在原生 `ubuntu-22.04-arm` runner 上（默认矩阵不编它，只有 `all_slots=true` 取证跑才出产物），地板 = **2.35**，与 x64
用的 `ubuntu-22.04` 一致。选 22.04 而不是 24.04 就是为了这个地板——24.04 的 glibc 更高，会把
Debian 12（2.36）、Raspberry Pi OS Bookworm（2.36）这些实际在跑的 ARM 发行版挡在门外。
要抬地板（例如将来 ARM 生态整体上移）换 `release.yml` 里那个槽位的 `os` 即可，脚本与产物名都不用动；
`ubuntu-24.04-arm` / `ubuntu-26.04-arm` 都是同一家族的标准公开仓库标签（4C/16GB、免费不限量）。

### macOS 12+

| 依赖 | 说明 |
|------|------|
| 系统框架（Cocoa / Metal / IOKit / Security …） | 由操作系统提供，无第三方动态库 |
| `libSystem` | 系统 |

macOS 产物是**裸二进制**而非 `.app` bundle：符合单文件承诺，但没有 Dock 图标、Finder 双击不生效，
从终端启动（README 里给了 `xattr -d com.apple.quarantine` 的 Gatekeeper 解法）。
签名/公证（需 Apple Developer 账号）与 `.app` 打包是后续工作，不做假承诺。

## 5. 软渲染兜底（无 GPU / 老驱动 / 远程桌面）

**现役入口（iced 版）**：`unlucky-launcher-iced --software-renderer`（等价 `--renderer=cpu`，即 `ICED_BACKEND=tiny-skia`）
走 tiny-skia 光栅化，这是现役构建唯一的软渲染入口——CI 的 runtime-smoke 与 `packaging/linux-smoke-docker.sh` 都这么跑（§11）。

**历史资产（`v0.1.0` 的 Slint 版）**：那批已发布二进制仍认 `unlucky-launcher --software-renderer`
（解析在 `crates/app/src/main.rs` 的 `apply_renderer_flag()`，只认完全相等的那个串）。
这条 Slint 路径（默认 `renderer-femtovg`/GL）已不在默认构建与发布矩阵内——`crates/app` 被 workspace `exclude`，
源码仅作参考、不编译——所以它**对现役 `unlucky-launcher-iced` 无效**，别照抄到 iced 版上。

- 软渲染是 CPU 光栅化，无 GPU 也能出图；代价是动画与大窗口重绘更慢（Metro 纯色界面影响有限）。
- 截图通道 `--screenshot <png>`：历史实现 `crates/app/src/screenshot.rs`，现役 `crates/app-iced/src/shot/`；
  软渲染下照样出图，是 CI 与远程机唯一可靠的"真出图"证据（发布冒烟见 §7，本机实跑 `--software-renderer --screenshot`）。

## 6. Cargo 构建档位（release 体积档 / fast 速度档）

| 档位 | 用在哪 | opt-level | lto | codegen-units | panic |
|------|--------|-----------|-----|---------------|-------|
| `release` | 发布 / 验收产物 | `"z"`（workspace 自身 crate） | `fat` | 1 | `abort` |
| `release.package."*"` | 同上，**只作用于依赖** | `"z"` | 不可覆盖 | 16 | 继承 |
| `fast` | 日常迭代、本机跑测试 | 2 | `thin` | 16 | `unwind` |

- **`release` 是体积档**：`fat` + 单 CGU 换来实测 GUI −10.8% / CLI −18.7%（§7.1）。变慢的根源在 `codegen-units = 1`
  （单 CGU → LTO 只一个线程可跑；`fat` 只管"跨 crate 全量参与"，不等于单线程），375 个 crate 排队链接就是十几分钟的量。
- **`release.package."*"` 放宽的只有并行度，不是 opt-level**：实测把依赖 opt-level 抬到 3 反而**又慢 29.6%
  又大 19~24%**（§7.1），所以保持 `"z"`、只把 `codegen-units` 放到 16。package 覆盖**只允许调优化强度**：
  `panic` / `lto` / `rpath` 写进去 cargo 直接报错，`strip` 会改变整个二进制的运行时行为——都不许出现。
- **`fast` 是速度档，不是发布物**：`thin` LTO + 16 CGU 可并行，`opt-level = 2` 介于 `z` 与 3 之间。
  `panic = "unwind"` 是给**编出来的二进制**留的（`release` 是 `abort`，`inherits` 会照抄）；`cargo test`
  **不**依赖它——cargo 对 test 目标自己强制 unwind（实测 release/abort 档下 `cargo test --profile release`
  照跑、23 个测试全过，test 单元的 rustc 参数里没有 `-Cpanic=`）。要看这行是否生效，比 `cargo build -v`
  的 rustc 参数：release 有 `panic=abort`、fast 没有。它继承 `strip="symbols"`，所以能 unwind 但**没有符号
  回溯**——要栈回溯就给 fast 档加 `strip = false`，代价是体积再涨。
- **`fast` 在机械盘机器上冷编译并无收益**（比 release 还慢、体积 2.3 倍，§7.1），价值在 SSD 机器与增量重编。
- **`[profile.fast.package."*"]` 是承重墙**：`inherits` 会把 release 的 package 覆盖一起带过来（实测：删掉它，fast 档外部依赖静默退回 `"z"`），必须存在。
- 本地开发迭代走 `cargo build`（dev profile）不受影响，但 **`ci.yml` 每次 push/PR 都跑
  `cargo build --workspace --release`**，会实打实吃这份 fat LTO 的链接时间——CI 慢的一部分是 release 档构建。

## 7. 实测（本机，不是估算）

构建环境：rustc 1.99.1 stable / `x86_64-pc-windows-gnu`（MinGW 链接器）。§7.2/§7.3 的早数字是另有 worker 并发做全量 debug 构建时测的（偏悲观）；§7.1 的三行 M96 数据是空载冷编译。

**本节以下所有输出里的 `0.1.0` 都是当时那次运行的真实输出（workspace 版本那时是 `0.1.0`），属实测记录，不随后续版本号改动而回填**；当前版本见 `Cargo.toml` 的 `[workspace.package] version`。§2 的校验机制不依赖任何写死的数字，改版本号不会让它失效。产物名同理且更容易被误读成现状：§7.3 / §7.5 里的 `unlucky-launcher-0.1.0-windows-*.exe`（**连字符**）是 2026-11-01 那几次实测的原文，D49（2026-11-05）才改成下划线形态——同一槽位现在叫 `unlucky-launcher_0.2.1_windows_x64.exe`（规则见 §1）；拿旧名去 Release 页或磁盘上找文件必然找不到，那不代表发布挂了。

### 7.1 体积与冷构建耗时（实测字节数，不是估算）

| 档位 | `unlucky-launcher.exe` | `unlucky-launcher-iced.exe` | `ull.exe` | 冷构建耗时 |
|------|-----------------------|----------------------------|-----------|-----------|
| `release`，`thin`（M46 前） | 16,366,080 B = 15.61 MiB | 未测（当时无 iced 版） | 7,572,992 B = 7.22 MiB | 11m52s |
| `release`，`fat`，依赖全 `"z"`（M46，改前） | **14,595,584 B = 13.92 MiB** | 未测 | **6,157,824 B = 5.87 MiB** | 9m27s（增量） |
| `release`，`fat` + 依赖 opt-3 / cu16（M96 试过的那版，**已否**） | 20,407,296 B = 19.46 MiB | 18,371,584 B = 17.52 MiB | 7,743,488 B = 7.38 MiB | 22m45s（冷） |
| `release`，`fat` + 依赖 `"z"` / cu16（**M96 起，发布档**） | **16,479,232 B = 15.72 MiB** | **13,910,528 B = 13.27 MiB** | **6,256,128 B = 5.97 MiB** | **16m01s（冷）** |
| `fast`，`thin` + cu16（M96 起，日常档） | 38,182,400 B = 36.41 MiB | 33,570,304 B = 32.02 MiB | 15,967,744 B = 15.23 MiB | 24m27s（冷） |

测量条件（M96 三行）：12 核 / `x86_64-pc-windows-gnu` / `-j 4`，源码机械盘、`CARGO_TARGET_DIR` 在 SSD，均为**删空 target 后的
冷编译**、同机同晚无并发负载；M46 两行是更早提交上的数字（依赖已热 / 另有并发），**跨行比耗时没意义**。
① **依赖 opt-level 从 3 退回 `"z"` 又快 29.6% 又小 19~24%**（两臂同为 cu16，唯一自变量是 opt-level）——抬 opt-level 是拿编译时间和体积换运行期；
`cu1 → cu16` 的收益本轮**没有同条件对照**，未归因。② `fast` **冷编译比 `release` 还慢**（24m27s vs 16m01s）、体积 2.3 倍，瓶颈在两个大 UI
crate + 链接 + 机械盘 IO——定位是 **SSD 机器与增量重编**，不是冷编译加速。体积归因：GUI 相对旧基线 +12.9%（13.92 → 15.72 MiB）里
「代码增长」与「依赖 cu16」**未分离**；`ull` 只 +1.7%，在 +1~3% 口径内。体积构成（**M118 前的 Slint 版，历史数**）：Slint 运行时（双渲染器 + winit/accesskit/托盘）+ rustls/reqwest + rig/rmcp/axum 为主，`ull` 更小是 D18（CLI 零 UI 依赖）的结果。

### 7.2 冒烟（真跑，2026-10-31 夜间 + 11-01 早，本机 Win11 26100）

| 命令 | 实测结果 |
|------|---------|
| `ull.exe --version` | 输出 `ull 0.1.0（unlucky-cli）` + `os: Windows 11 · 10.0.26100（x86_64）· 满足最低要求`，exit 0 —— 版本与 workspace `0.1.0` 一致 |
| `unlucky-launcher.exe --software-renderer --screenshot … --data-dir …`（Slint 版，历史实测） | exit 0，产出 1180×760 PNG（3,588,298 B）；看图确认 Metro 实例页（侧栏/搜索/空态/AI 助手面板）正常渲染 → **软渲染路径真的能出图**，不依赖 GL |
| `unlucky-launcher.exe --version` | **无输出**，8 秒后被 timeout 杀掉（exit 124）——GUI 当时不解析 `--version`，会直接开窗口。M82 后已修（走 CLI 分发，见 §10-1）|

### 7.3 打包验证（本机 Windows 槽位真跑；产物名是 D49 改名前的形态）

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

### 7.4 运行时依赖证据（PE 导入表 / ELF `DT_NEEDED`）

`objdump -p target/release/unlucky-launcher.exe` 共 42 条 DLL 导入记录、33 个不同名字（大小写各记一次），全部是 Windows 自带：
`KERNEL32/USER32/GDI32/ADVAPI32/SHELL32/SHLWAPI/ole32/oleaut32/combase/comctl32/dwmapi/dwrite/imm32/
uxtheme/uiautomationcore/opengl32/ws2_32/bcrypt/bcryptprimitives/ntdll` + 一组 `api-ms-win-crt-*`（UCRT API set）。

**没有** `VCRUNTIME140.dll` / `MSVCP140.dll` 这类 MSVC 可再发行包依赖，也没有 `libwinpthread-1.dll` /
`libgcc_s_*.dll` 之类 MinGW 运行时 —— 即"拷走一个 exe 就能跑"（Win10+ 自带 UCRT）。
CI 的 MSVC `+crt-static` 产物同理（静态 CRT）；`release.yml` 里用 `dumpbin -dependents` 把这条变成硬门禁，
两侧都 fail、没有降级分支：**出现 VCRUNTIME/MSVCP/ucrtbase 就 fail**，**找不到 dumpbin 同样 fail**
（`windows-latest` 与 `windows-11-arm` 两个镜像都自带 VS 工具链，基本找得到；跳过等于让门禁在最需要它时失效）。
总之不允许"悄悄要用户装可再发行包"。
注意：本机只装了 GNU 工具链，MSVC 那个槽位的产物本身**没有在本机验证过**，由 CI 首跑验证。

Linux 侧同一件事的对照（M82 实测：`readelf -d` + `ldd` + `nm -D`，容器内 Debian 12 glibc 2.36；被测的是 **Slint 版 GUI，历史实测**）：Slint
GUI 的 ELF `DT_NEEDED` 只有 `libfontconfig.so.1` / `libgcc_s.so.1` / `libm.so.6` / `libc.so.6` /
`ld-linux-x86-64.so.2`；`libX11` / `libxcb` / `libxkbcommon` / `libwayland-client` / `libGL` / `libEGL`
**都不在**导入表里，而是以字符串形式由 `dlopen` 在开窗那一刻加载（`nm -D` 可见 `U dlopen@GLIBC_2.34` /
`dlsym` / `dlclose`）。所以 §4 那张「开窗要的库」不是启动前提，缺了也不影响进程起来跑 CLI；
`ull` 的 `DT_NEEDED` 连 fontconfig 都没有，只剩 glibc 那一组。

**补一次真跑（M82 复审在 `debian:bookworm-slim` 上做的）**：镜像里 X11 / wayland / GL / EGL / xkbcommon /
libxcb 条目数 = 0，只把 fontconfig 那 7 个 `.so` 放进 `LD_LIBRARY_PATH`，跑**改动后**的二进制 → `--version`
与 `--help` exit 0、`instances list --json` 合法 JSON exit 0、`lst` exit 1 且带提示；`readelf -d` 复核仍是
恰好 5 条 `DT_NEEDED`（无 `libstdc++`、无任何图形库），`ull` 4 条 —— 即「能跑完子命令」是实测，不是推论。

### 7.5 arm64 槽位的本机验证（脚本层取证，不假装跑得动 arm64 二进制）

本机没有 ARM 机器，也不具备 MSVC ARM64 链接库组合，所以只做**能真实证明的东西**（`python` 直接加载
`packaging/make_artifacts.py` 调函数 + 走 CLI 全流程，证据见下）：

| 取证项 | 结果 |
|--------|------|
| 宿主判定注入 | `win32/AMD64`→`windows-x64`、`win32/ARM64`→`windows-arm64`、`linux/x86_64`→`linux-x64`、`linux/aarch64`→`linux-arm64`、`linux/arm64`→`linux-arm64`、`darwin/arm64`→`macos-arm64`、`darwin/x86_64`→`macos-x64`、`freebsd/riscv64`→exit 1 |
| 文件头架构识别 | 真 `unlucky-launcher.exe`→`x64`；合成 PE `0xAA64`→`arm64` / `0x8664`→`x64`；ELF `0xB7`→`arm64` / `0x3E`→`x64`；Mach-O `0x0100000C`→`arm64` / `0x01000007`→`x64`；垃圾字节→`None`（拒绝出包） |
| `windows-arm64` stub 全流程 | 把真 exe 的 PE 机器码改成 ARM64 当 stub：`arch: … -> arm64 (ok for windows-arm64)`、`smoke: skipped (host is windows-x64, artifact is windows-arm64)`、产物 `unlucky-launcher-0.1.0-windows-arm64.exe`（当次实测的连字符名）14.01 MiB，`SHA256SUMS.txt` 行名一致 |
| 架构守卫真拦 | x64 二进制冒充 `--slot windows-arm64` → exit 1：`is a x64 binary but slot windows-arm64 says arm64` |
| `linux-arm64` stub 归档 | 成员 `unlucky-launcher-0.1.0-linux-arm64/{LICENSE,NOTICE,README.md,unlucky-launcher}`（旧连字符名，当次实测），权限 0644/0755、`uid=gid=0`、mtime 0、gzip 头 mtime 字段 `00 00 00 00`；连调两次 sha256 完全一致（`f949d0f5…`）→ 可复现规则与 x64 同源 |
| `windows-x64` 原生回归 | `arch: … -> x64`、`smoke: ull.exe --version -> 0.1.0 (ok)`、产物 14.01 MiB、`sha256sum -c SHA256SUMS.txt` OK（这一次用的是当前工作树的二进制，与 §7.3 那次不是同一次构建，sha256 自然不同） |

**没在本地验证、只能由 CI 补的**：`windows-arm64` 在 `windows-11-arm` 上的真实链接与 ARM64 导入表、
`linux-arm64` 在原生 arm64 runner 上的真跑（本机既没有 ARM 机器，也连 `raw.githubusercontent.com` 都解析不了）。
M141/M142 保留了编译前体检（查 VC 的 `lib/arm64/libcmt.lib` 与 SDK 的 `um/arm64/kernel32.lib`，无论齐不齐都先打 4 行
实况注解）。闭环标准不是「两槽位都进 SHA256SUMS」（M142 起只有 x64 进），而是 arm64 注解转绿 + 把键加回 `PUBLISH_SLOTS`。

## 8. 发行合规

- `LICENSE`（Apache-2.0 全文）与 `NOTICE`（含 Slint Royalty-Free 2.0 义务说明）随包分发，
  三份文本在 UNIX 归档内、在 Windows 侧作为并列 Release 资产。
- Slint 的 RF 2.0 义务只跟 `v0.1.0` 那批资产：当时关于页有 `AboutSlint` 部件、选的是不触发 GPL 分支的 RF 分支
  （见 `deny.toml` 的许可证白名单注释）。现役 iced UI 里没有、也不需要任何 Slint 部件。
- **M118 后这条义务只跟着已发布的 `v0.1.0` 资产走**：`crates/app` 已被 workspace `exclude`，发布矩阵里再没有链接 Slint 的二进制，新的 iced 版产物不链接 Slint；`deny.toml` 那条 RF-2.0 allow 因此暂时是死条目（不删，留给将来真有别的路径引入 Slint 时兜底）。
- `README.md`（英文主版）随包分发；中文版 `README.zh-CN.md` 留在仓库不进包（避免包体噪音）。
- 依赖许可门禁不变：`deny.toml` 白名单（Apache-2.0 / MIT / BSD / Zlib / BSL-1.0 / ISC / CC0 / Unlicense /
  Unicode-3.0 / CDLA-Permissive-2.0 / Slint-RF-2.0）。发布任务不新增任何依赖。
- 发布产物不含游戏本体、不含 JRE、不含任何账号数据；`ull`/GUI 的数据目录在用户侧生成。

## 9. 支持矩阵（对外口径，与 release-notes 模板逐行一致）

| 平台 | 本版发布 | 级别 | 运行时要求 |
|------|---------|------|-----------|
| Windows 10/11 x64 | ✅ | 一等 | 无（UCRT 系统自带） |
| Windows 11 ARM64 | ❌ 不发布（构建取证中，待修） | 一等（启动器产物） | 无额外库；启动器原生运行，游戏本体走系统 x64 模拟层 + x64 JDK |
| macOS 12+（arm64 / x64） | ❌ 不构建 | 一等（未真机验证） | 无第三方库；未签名需去 quarantine |
| Linux x64 X11 | ❌ 不构建 | 一等（未真机验证） | glibc 2.35+、xkbcommon、fontconfig、libGL、xcb |
| Linux x64 Wayland | ❌ 不构建 | 支持 | 上述 + wayland-client |
| Linux arm64 X11 | ❌ 不构建 | 一等（未真机验证） | glibc 2.35+（`ubuntu-22.04-arm` 原生构建，**与 x64 同地板**）、xkbcommon、fontconfig、libGL、xcb |
| Linux arm64 Wayland | ❌ 不构建 | 支持 | 上述 + wayland-client |
| Windows 8.1/8/7 | — | 不支持 | 基线已上调（D21） |
| Android Termux + X11 | — | 试验级（不承诺） | 仅 CLI（源码自建），GUI 未验证，不在本发布矩阵内 |

`packaging/release-notes.md` 的 "Support matrix" 表与上表逐行对应（英文版同样带 published / not
published 一列，Termux 行标注 experimental）——改一边必须同步另一边，否则标题里的"一致"就是假话。

两个 arm64 槽位都是**正式矩阵槽位**（不是 Termux 那种源码自建旁路），但本版两个都**不发布**：`windows-arm64` 照构建取证
（红了不挡 x64 发布），`linux-arm64` 连构建都不跑；发布开关是 `release.yml` 的 `PUBLISH_SLOTS`（当前只有 `windows-x64`）。

## 10. 已知边界 / 后续

1. **GUI 的 `--version` 已解决**（M82）：现在命中子命令分发，输出与 `ull --version` 同一条版本 + 系统摘要后退出，
   不再开窗；§7.2 里「无输出、8 秒后被 timeout 杀掉」那行是**改之前**的记录，留作对照。
2. **签名/公证**：Windows 无 Authenticode、macOS 无 codesign/notarize（都要证书/开发者账号），
   当前以 SHA256SUMS + Release 说明作为完整性参考。
3. **macOS `.app` bundle / `.dmg`**：与单文件承诺冲突，暂不做；若用户要 Dock 图标再单开 mission。
4. **Linux musl 静态版**：可彻底摆脱 glibc 地板，但现役 iced 版（winit + wgpu）在 musl 上的可行性未验证，未纳入本矩阵。
5. **`ull` 已随包**（M82 起）：每个槽位与 GUI 并列发一份裸 `ull`，进同一份 `SHA256SUMS.txt`，矩阵
   完整性检查按「GUI 与 CLI 各自都要有」分开查（`release.yml`）。等 CI 首跑确认这条在真流水线上有效。
6. **本机 GNU 链接器的已知无害告警**：`cargo build --release`（GNU 工具链）会打一行 `warning: linker
   stderr: corrupt .drectve at end of def file`（MinGW `ld` 的老式抱怨，不影响产物；CI 的 MSVC 槽位没有）。
7. **两个 arm64 槽位本机跑不了**：本机是 x64 Windows，既无 ARM 机器也无 MSVC 的 ARM64 链接库组合，
   `windows-arm64` / `linux-arm64` 的**真实产物只能由 CI 出**（前者靠 `dumpbin -dependents` 的静态 CRT
   门禁 + 架构校验，后者在 `ubuntu-22.04-arm` 上真跑 `ull --version` 冒烟）；本机能做且已做的是脚本层
   取证（§7.5：宿主判定注入、可执行文件头识别、把真 exe 的 PE 机器码改成 ARM64 当 stub 走完全流程）。

## 11. CI 运行时冒烟（Linux 真渲染一帧）

**细节与依据在 [`packaging/ci-runtime-smoke.md`](ci-runtime-smoke.md)**（本节再增长也往那里写，两个文件都必须 ≤400 行）。

`ci.yml` 的 `runtime-smoke` job 在 `ubuntu-latest` 上补第一份**真实图形环境**证据：装 `xvfb` + 软渲染依赖 +
字体，用 `xvfb-run` 起 GUI 进程渲出一帧 PNG，再对**像素本身**断言（退出码 0 / 尺寸 / 非空白，artifact
`runtime-smoke-screenshot`）。

要点三条：

- 只看退出码不够：「合法 PNG + 退出码 0」不等于画面有内容；「完全没字体」与「非软渲染」都是**硬失败**（panic /
  报错，不产生 PNG；review 实测）。
- 闸门下限 ≈「至少画出一条标题栏」（review 实测切片仍 pass）：证明「场景被真实渲染过」，**不证明界面元素齐全，
  更不证明视觉正确**——视觉靠人眼看 artifact。
- 只覆盖 Linux x64；macOS 是 dispatch-only 探针（无实测证据），Windows 已有本机人眼验收。

**本地复跑**：`bash packaging/linux-smoke-docker.sh . D:/tmp/ull-smoke`——脚本逐步复刻这个 job（同样的 apt 依赖、
`--bin unlucky-launcher-iced`、同样的 `xvfb-run … --software-renderer --screenshot --data-dir`）；输出目录别写 `/tmp`，
Git Bash 下会静默落进 Docker VM（脚本内部用 `cygpath -m` 归一）。
**M131 真跑过一次**（本机 Docker / Debian 12 容器 / 2026-11-05）：`exit=0`，截图 1180×760、`[ok] 截图非空白`。

## 12. 文件清单

| 文件 | 作用 | 行数上限 |
|------|------|---------|
| `packaging/README.md` | 本文：矩阵/依赖/版本/合规契约 | ≤400 |
| `packaging/make_artifacts.py` | 打包与校验（本地 + CI 共用） | ≤400 |
| `packaging/release-notes.md` | GitHub Release 说明模板（`{{VERSION}}` / `{{SUMS}}` 占位） | ≤400 |
| `packaging/check_screenshot.py` | 截图非空白判定（CI 门禁 + 本地手动跑，零第三方依赖） | ≤400 |
| `packaging/ci-runtime-smoke.md` | §11 的细节：断言口径/阈值依据/失效模式/复跑 | ≤400 |
| `packaging/linux-smoke-docker.sh` | 本地复跑 `runtime-smoke`（Docker + xvfb） | ≤400 |
| `.github/workflows/release.yml` | tag 触发 → 矩阵构建 → Release | ≤400 |
