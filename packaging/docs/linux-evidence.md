# Linux 槽位的证据账（M184）

本文是「Linux 到底验过什么、没验什么」的**唯一账本**。M184 把 `linux-x64` 写进了 `PUBLISH_SLOTS`（发布），
把 `linux-arm64` 留在「构建但不发布」，两者都不是凭感觉：下面按「已有哪些证据 → 本轮补的实测 → CI 增量 →
仍然没验的部分 → 解锁条件」逐条记账，任何一条被推翻都要回来改这里与 `compliance-support-boundaries.md` §9。

## 1. M184 之前的证据盘点

| 证据 | 位置 | 覆盖了什么 | 没覆盖什么 |
|------|------|-----------|-----------|
| xvfb 运行时冒烟（每次 PR/main） | `.github/workflows/ci.yml` 的 `runtime-smoke` | `ubuntu-latest`（x64）上装 xvfb + 软渲染依赖 + 字体，`xvfb-run … --software-renderer --screenshot` 真渲一帧，`packaging/check_screenshot.py` 对**像素本身**断言（唯一色数 / 亮度 σ / 强边缘占比），截图作为 artifact 留证 | 只有 x64；用的是 **debug** 档（`cargo build --bin unlucky-launcher-iced --locked`），不是发出去的那个 release 产物；不跑 GPU 通路；不验 Wayland；不校验 `[theme]` / `[backend] final` 日志行；没有 arm64 |
| 本地复跑脚本（开发机） | `packaging/linux-smoke-docker.sh` | 在 `rust:1-bookworm`（**Debian 12 / glibc 2.36**）里逐步复刻上面那个 job；M131 真跑过一次（2026-11-05）：`exit=0`、截图 1180×760、`[ok] 截图非空白` | 基础镜像是 Debian 而不是 runner 的 Ubuntu；仍是 debug 档；同样没有 arm64 |
| 编译期门禁复跑 | `packaging/linux-check-docker.sh` | 在 Linux 上跑 `clippy --workspace --all-targets -- -D warnings` 与 `test --workspace --all-targets`（M144 那两条只能在 Linux 复现的红） | 与运行时无关，只证明「编得过、测得过」 |
| 阈值依据 | `packaging/ci-runtime-smoke.md` | 真实帧与纯色帧的实测数值（唯一色 342~2352、σ 19.45~27.46、强边缘 0.0076~0.0433）都在这里，阈值取在两者之间 | 标定样本来自 x64 软渲染 |
| `ull --version` 冒烟 | `packaging/make_artifacts.py::smoke_version` | 槽位与宿主一致时**机制上会真跑** CLI 并断言版本串（`slot != host_slot()` 才跳过）。`linux-x64` 侧本轮**真跑过**（见 §2 `smoke: ull --version -> 0.2.3 (ok)`） | **机制存在 ≠ 有过一次运行**：截至本分支，仓库里找不到任何一次 `linux-arm64` 成功出包的记录（`git grep -rn "all_slots" DevDocs/` 零命中），所以 **arm64 侧运行时证据为零** —— 连这条 CLI 冒烟都没在 arm64 上真跑过。GUI 一帧更没有 |

**glibc 地板怎么定的**：槽位表把 Linux 两档钉在 `ubuntu-22.04` / `ubuntu-22.04-arm`，**不跟 `latest` 走**
（`release.yml` 的槽位表注释：跟 latest 走地板就会随镜像漂移）。Ubuntu 22.04 ⇒ glibc **2.35**，这就是
`README` / §9 写的「glibc 2.35+」的来源，也是本轮真机验收为什么选 Ubuntu 22.04 的 WSL 发行版：**验证环境的地板
与发布产物的地板一致**，在更新的发行版上验等于放宽了条件。

**一处诚实记录**：M184 的任务描述里提到「曾用过 WSLg 开真窗口跑首帧」。本仓库里查不到那次留下的任何文档或
脚本痕迹（`grep -rn WSLg` 在本轮改动之前零命中），所以下面这一节是**重新做了一遍并留证**的，不是转述。

## 2. 本轮实测（WSLg 真机，linux-x64 release 产物）

**环境**（2026-11-08，`wsl.exe -v` / 发行版内自报，逐字取自实测输出）：

```
Linux 6.6.114.1-microsoft-standard-WSL2 x86_64      # WSL 2.7.3.0，WSLg 1.0.73
PRETTY_NAME="Ubuntu 22.04.5 LTS"                    # == 发布槽位 ubuntu-22.04 的同一代
ldd (Ubuntu GLIBC 2.35-0ubuntu3.15) 2.35            # == 文档写的 glibc 2.35 地板
DISPLAY=:0 WAYLAND_DISPLAY=wayland-0
rustc 1.99.1 / cargo 1.99.1（rust-toolchain.toml 的 stable）
```

**构建**（与 CI 同一条命令，12 核 WSL，crates 走 rsproxy 镜像；镜像只影响下载速度，不影响产物）：

```
cargo build --release --locked --bin unlucky-launcher-iced --bin ull
    Finished `release` profile [optimized] target(s) in 9m 38s      # BUILD_WALL=579.19s
-rwxr-xr-x 19571328 unlucky-launcher-iced
-rwxr-xr-x  7534848 ull
```

**打包**（CI 的同一份脚本 `packaging/make_artifacts.py`，`--slot linux-x64`）：

```
arch:     unlucky-launcher-iced -> x64 (ok for linux-x64)
smoke:    ull --version -> 0.2.3 (ok)              # 真跑，不是跳过（宿主 == 产物）
arch:     ull -> x64 (ok for linux-x64)
artifact: unlucky-launcher_0.2.3_linux_x64.tar.gz  8.05 MiB  sha256=c78e9f6cbab44e0fa76912f5d3608b911d89e2de2adc473339341d8ac3b60c13
artifact: ull_0.2.3_linux_x64  7.19 MiB  sha256=42ff569152f1af89899b0925b4a7495bfcaeb66988889a0348722aa2d6ae2369
sums:     SHA256SUMS.txt
```

归档解开就是用户拿到的东西（`unlucky-launcher` 0755；`ELF 64-bit LSB pie executable, x86-64, stripped`）：

```
-rw-r--r-- LICENSE 11345 / NOTICE 4721 / README.md 30542
-rwxr-xr-x unlucky-launcher 19571328
$ ./unlucky-launcher --version
unlucky-launcher 0.2.3（unlucky-cli）
os: Ubuntu 22.04.5 LTS · 6.6.114.1-microsoft-standard-WSL2（x86_64）· 满足最低要求
```

**首帧（`--screenshot` 通道，真窗口在 WSLg 的真显示服务里开出来再渲）**，`exit=0`，关键日志行逐字：

```
[probe] adapters=0 hardware=false :: request=cpu -> ICED_BACKEND=tiny-skia
[font] platform=linux text="Noto Sans CJK SC" display="Noto Sans CJK SC" mono="DejaVu Sans Mono" | … source=env cjk=not-needed
[shot] 未指定主题，按 Light 出图（D30 可复现；CI 冒烟依赖这条默认值）
[theme] source=default-light flavor=light follows_system=false
[window] requested size=1180x760 min=880x560 resizable=true decorations=false transparent=false scale_requested=system size_source=default title="Unlucky Launcher"
[boot] first frame in 485 ms（run() 开始 → 首次 view，含后端探测与字体扫描）
[shot] 1180x760 scale=1 -> …/wslg-first-frame.png
[shot] screen=shell theme=light renderer=tiny-skia route=Detail("survival")
[backend] final=tiny-skia result=started elapsed=1333ms evidence=probe-single-item
```

像素判定（`packaging/check_screenshot.py --json`，图 132637 字节）：

```
{"width": 1180, "height": 760, "pixels": 896800, "unique_colors": 1858,
 "top_color": "#f6faff", "top_color_share": 0.5564, "luma_mean": 241.18,
 "luma_stddev": 24.06, "edge_ratio": 0.0372, "problems": []}
[ok] 截图非空白
```

**真的开了一个窗口**（交互模式，不加 `--screenshot`）：进程 12 秒后仍存活，WSLg 侧出现
`msrdc` 窗口 **"Unlucky Launcher (cyrene-wsl)"**，`GetWindowRect` 读回 **1180×760**（正是它请求的尺寸），
从 Windows 侧对这块矩形截屏后**人眼可见**：自绘标题栏三键 + 左侧导航轨（Instances / Versions / Logs /
Settings / Assistant）+ "No instance selected" 空态 + 左下 "Not signed in"。这张图已入库：
[`docs/images/linux-x64-wslg-window.png`](../../docs/images/linux-x64-wslg-window.png)
（1180×760，43084 字节，`check_screenshot.py`: 唯一色 871、σ 14.4、强边缘 0.0137 ⇒ 非空白 —— 比进程内那张低，
因为桌面截屏是**已经合成到 Windows 桌面**的结果，抗锯齿被放大过一次；两张都远在阈值之上）。
交互模式的日志另有两行值得记：

```
[probe] adapters=1 [D3D12 (AMD Radeon(TM) Graphics) [Other]] hardware=false :: request=auto -> ICED_BACKEND=tiny-skia
[probe] 探测耗时 1527 ms（wgpu 实例 + 全后端适配器枚举）
[boot] first frame in 2273 ms（…）
[theme] source=system flavor=pending follows_system=true
[theme] 系统主题 = None
[appearance] hwnd=none（拿不到窗口句柄：整窗 α 无法下发）
[corner] boot corner=policy=system source=square os=unknown excluded_reason=unknown-os radius=system pref=none readback=n/a window=none frame=? layered=?
```

读法（别过度解读）：① WSLg 能枚举到一个 D3D12 适配器但 `hardware=false`，所以 auto 也落到软渲染
（`tiny-skia`）—— **GPU 通路在 Linux 上仍然没有证据**；② WSLg 没有"系统主题"，所以 `follows_system=true`
而 `系统主题 = None`（真桌面上的系统主题跟随仍未验）；③ **整窗透明与圆角在 Linux 上本来就是 no-op**：日志自己说
`hwnd=none`、`corner=os=unknown`（用户令：这两件事交给桌面合成器，我们不自绘）—— 所以它们不是"未验"，而是
**在 Linux 上没有这条代码路径**。

**一处 WSLg 的坑**（写在这里免得下次重复踩）：WSLg 的 X 侧只有 Weston 的代理窗口，
`import -window root` / `xwd -root` 一律 `X_GetImage … BadMatch`，**拿不到桌面级截屏**；
真窗口像素要么走进程内 `--screenshot`（CI 就是这么做的），要么从 Windows 侧对 `msrdc` 窗口矩形截屏（本节采用后者）。


## 3. 本轮实测（Xvfb，同一个 release 产物）

同一条命令在无头 X 里再跑一遍（Ubuntu 22.04 的 Xvfb，**release 产物**，判据与 CI 新增那步逐字一致）：

```
xvfb-run -a --server-args="-screen 0 1280x1024x24" ./unlucky-launcher --software-renderer \
  --screenshot …/xvfb-first-frame.png --data-dir …/data-xvfb
launcher exit=0
[theme] source=default-light flavor=light follows_system=false
[boot] first frame in 35 ms（run() 开始 → 首次 view，含后端探测与字体扫描）
[shot] 1180x760 scale=1 -> …/xvfb-first-frame.png
[backend] final=tiny-skia result=started elapsed=1147ms evidence=probe-single-item
{"width": 1180, "height": 760, "pixels": 896800, "unique_colors": 1858, "top_color": "#f6faff",
 "top_color_share": 0.5564, "luma_mean": 241.18, "luma_stddev": 24.06, "edge_ratio": 0.0372, "problems": []}
[ok] 截图非空白
```

软渲染是确定性的：WSLg 与 Xvfb 两张图的统计量**逐个相同**（1858 / 24.06 / 0.0372），也正是这一点让
`check_screenshot.py` 的阈值（唯一色 ≥64、亮度 σ ≥3.0、强边缘 ≥0.001）有 **8~37 倍**余量
（唯一色 1858/64≈29×、强边缘 0.0372/0.001≈37×、**亮度 σ 只有 24.06/3.0≈8×** —— 与 `check_screenshot.py` 自己
写的「阈值取在实测下限的 1/5~1/8」一致，三者里最紧的是亮度 σ）：CI 里这条门禁不会因为"某次抗锯齿抖了"而假红。

本机 Docker 复跑（`packaging/linux-smoke-docker.sh`，Debian 12 容器 + **debug** 档，缓存卷是热的）：

```
$ bash packaging/linux-smoke-docker.sh . C:/Temp/ull-m184/smoke-out
rustc 1.99.1 (58cf8f9c3 2026-10-13) / cargo 1.99.1 / 12.15      # 12.15 = Debian 版本号
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1m 26s
[font] platform=linux text="DejaVu Sans" display="DejaVu Sans" … source=default cjk=not-needed
[theme] source=default-light flavor=light follows_system=false
[boot] first frame in 69 ms（run() 开始 → 首次 view，含后端探测与字体扫描）
[backend] final=tiny-skia result=started elapsed=1736ms evidence=probe-single-item
exit=0
/out/runtime-smoke.png: 1180x760 唯一色 1756 亮度σ 23.71 强边缘 0.03366 主色 #f6faff(55.7%)
[ok] 截图非空白
```

两条差异值得写下来（说明"为什么容器绿了还不够"）：① 容器里没有中文字体，字体解析落到 `DejaVu Sans`，而 WSL 里是
`Noto Sans CJK SC` —— **字体选择随环境变**，观感类验收只能在真桌面上做；② 基础镜像是 Debian 12（glibc 2.36），
比发布地板 2.35 高一档，所以它能证明"这条运行时通路通"，**不能**代替 `ubuntu-22.04` 上的证据（后者见 §2/§3 前面那段）。

单次启动的墙钟（`/usr/bin/time`，含 `xvfb-run` 起 X 的开销）：**WALL=1.96s**，日志里 `launch elapsed=1188ms`。


## 4. CI 增量：2 个槽位 → 4 个槽位

默认矩阵从 **2 档变 4 档**（`windows-x64` / `windows-arm64` / `linux-x64` / `linux-arm64`），多出来的两档是**并行**作业
（`fail-fast: false`），所以：

| 项 | 变化 | 依据 |
|----|------|------|
| 并发作业数 | 2 → 4 | `release.yml` 的 `plan` 输出（本地跑那段 python：默认 4 档、`all_slots=true` 6 档） |
| 每档新增步骤 | `linux-x64` 多 **2 步**（xvfb 运行时冒烟 + 上传首帧图）；`linux-arm64` 多 **0 步** | `release-build.yml`：11 步 → 13 步（本地 YAML 解析计数） |
| `linux-x64` 那 2 步的耗时 | apt 装 xvfb ≈ 30~50 s，跑一帧 ≈ 2 s，日志/像素断言 ≈ 1 s，上传 ≈ 3 s ⇒ **≈ 1~2 分钟** | 本地 Xvfb 实测单次启动 WALL=1.96s（`/usr/bin/time`） |
| 单档 release 构建时长（**估算**） | 冷缓存 ≈ 20~30 min；`Swatinem/rust-cache` 命中后 ≈ 8~15 min | 本机 12 核实测 **9m38s**；官方 `ubuntu-22.04` runner 是 4 vCPU，编译墙钟按 2.5~3× 估，再乘缓存命中折扣 |
| 总机时（**估算**） | 每 tag **+约 40 分钟**机时（2 档 × ≈20 min） | 同上 |
| 墙钟（**估算**） | 增量 ≈ `max(0, linux 档时长 − 现有最慢档)`，量级 **0~10 分钟**；发布侧多两个资产，秒级 | 四档并行，墙钟由最慢那档决定 |
| 配额风险 | `ubuntu-22.04-arm` 的可用性**本机无法核实**（无 `gh` CLI、无 Actions API 凭据） | 见 §5 最后一条：它不在发布名单里，起不来也不会挡住那两档发布 |

诚实边界：单档构建时长是**估算**（本机不是 runner，核数、缓存命中率、网络都不同）。下一次 tag 的真实数字应该回来
把这张表换成实测——**别让估算一直冒充实测**。


## 5. 仍然没验的部分（如实划出）

- **真实桌面发行版的观感**：拖动 / 缩放 / 字体度量与中文整形，一个都没在真桌面（GNOME/KDE 的可交互窗口）上
  人眼验收过。本轮做的是「起窗 + 渲出一帧 + 断言像素与日志」，它证明「能跑出画面」，不证明「观感正确」。
  （**整窗透明度与圆角不在这张清单里**：Linux 上它们没有代码路径 —— 见 §2 的三条读法，别把「不适用」写成「未验」。）
- **GPU 通路**：所有 Linux 证据都是软件渲染（`--software-renderer` / tiny-skia）。wgpu→Vulkan/GL 那条路在 Linux
  上一次都没跑过。
- **Wayland 原生**：WSLg 同时提供 X11 与 Wayland，本轮只按 `DISPLAY` 走（见 §2 的实测环境），没单独验
  `WAYLAND_DISPLAY` 那条路。
- **linux-arm64 运行时完全未验**：GUI 进程在 ARM64 上一次都没启动过。连 `make_artifacts.py` 那条 `ull --version`
  冒烟也**只是机制上会真跑**（`mode: ci` + 宿主==产物），**截至本分支没有一次 arm64 成功出包的记录**，所以 arm64
  侧一份运行时证据都没有 —— 别把「这条冒烟以后会跑」读成「它跑过了」。
- **`ubuntu-22.04-arm` runner 的可用性未在本机核实**：本机没有 `gh` CLI，也没有到 Actions API 的可用凭据，
  所以「这档 runner 在这个仓库上真能起来」只能由下一次 CI 证明。风险是有界的：它**不在** `PUBLISH_SLOTS` 里，
  起不来/红了不会挡住 `windows-x64` + `linux-x64` 的发布（只会让那一条 job 红，外加一条 `::warning::`）。

## 6. `linux-arm64` 的解锁条件（写死在这里，别凭感觉放行）

1. 在**真 ARM64 机器**（或原生 arm64 runner）上把 release 产物跑出一个非空白帧，`packaging/check_screenshot.py`
   通过；
2. 同一份日志里有 `[theme] source=…` 与 `[backend] final=… result=started` 两行（证明主题来源与渲染后端都按预期）；
3. 把上面两条的原始输出（时间、命令、图尺寸、日志行）贴进本文 §2 的位置；
4. 然后把 `linux-arm64` 加进 `release-publish.yml` 的 `PUBLISH_SLOTS`（一行），并同步 §9 与 `release-notes.md`。

构建绿了**不算**解锁，`ull --version` 跑得过也**不算** —— 那两条只证明 CLI 与编译器，用户拿到的那个 `.tar.gz` 里
装的是 GUI。（何况截至本分支 arm64 上连那条 CLI 冒烟都还没真跑过一次。）
