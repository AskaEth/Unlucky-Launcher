# Cargo 构建档位与实测数据（M145 拆自 `packaging/README.md`）

本文承载原 `packaging/README.md` 的 §6、§7，**节号沿用原编号**（全仓 `packaging/README.md §N` 这类引用照旧有效，
对照表在 [`packaging/README.md`](../README.md) 的索引里）。内容一字未改、只搬家：本行以上是 M145 新增的导航说明，
以下逐行取自原文件。

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

**本节以下所有输出里的 `0.1.0` 都是当时那次运行的真实输出（workspace 版本那时是 `0.1.0`），属实测记录，不随后续版本号改动而回填**；当前版本见 `Cargo.toml` 的 `[workspace.package] version`。§2 的校验机制不依赖任何写死的数字，改版本号不会让它失效。产物名同理且更容易被误读成现状：§7.3 / §7.5 里的 `unlucky-launcher-0.1.0-windows-*.exe`（**连字符**）是 2026-11-01 那几次实测的原文，D49（2026-11-05）才改成下划线形态——同一槽位现在叫 `unlucky-launcher_0.2.3_windows_x64.exe`（规则见 §1）；拿旧名去 Release 页或磁盘上找文件必然找不到，那不代表发布挂了。

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
