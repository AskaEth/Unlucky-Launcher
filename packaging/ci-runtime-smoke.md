# CI 运行时冒烟（Linux 真渲染一帧）

`packaging/README.md` §11 是本节的索引；细节在这里。为什么拆：本节要写阈值依据、失效模式与复跑口径，
塞回 README 会撞 400 行硬门禁——**两文件都必须 ≤400 行**，任一边再增长就再拆一节。

「编得过」≠「跑得起来」。`ci.yml` 的 `runtime-smoke` job 在 `ubuntu-latest` 上补第一份**真实图形环境**
运行证据：装 `xvfb`（虚拟 X 服务器）+ 软渲染依赖 + 字体，用 `xvfb-run` 把 GUI 进程拉起来，走既有的
进程内截图通道（`--software-renderer --screenshot <png> --data-dir <tmp>`）渲出一帧 PNG，再对**像素本身**断言。

## 1. 三道断言

`packaging/check_screenshot.py` 退出码 0 才算过；artifact 名 `runtime-smoke-screenshot`。

1. 进程退出码 0（截图通道失败会以非 0 退出，不留半张图）；
2. PNG 存在、可解析、宽高 > 0（0 字节 / 非 PNG / 路径不存在都是硬失败，脚本 exit 1）；
3. **非空白**：唯一色数 / 亮度标准差 / 强边缘占比三项同时过阈值。

为什么不能只看退出码：`--screenshot` 的 PNG 是进程自己编码写出的，只要流程走到 `take_snapshot` 就会得到
「合法 PNG + 退出码 0」；像素断言是**纵深防御**，守的是「进程活着、写出合法 PNG，但场景几乎没画」这一档。
本机可复现的两种失效都是硬失败（运行步就红，不必等像素断言）：①完全没有字体 → fontique panic
（退出码 101，不产生 PNG）；②不是软渲染（默认 femtovg）→ `take_snapshot is not supported by this
FemtoVG backend`（退出码 1）。这两条由 M62 review 实测复现（本 mission 未自己跑这两种破坏性实验）。
其余「渲染器出图但内容没上去」的情形本机暂无样本，属防御性设计，不宣称已复现。

## 2. 阈值与依据

`python packaging/check_screenshot.py --selftest` 可复现后三行：

| 样本 | 唯一色数 | 亮度σ | 强边缘占比 | 判定 |
|------|---------|-------|-----------|------|
| 本机 Docker 真截图（xvfb + 软渲染，1180x760，空数据目录） | 342 | 19.45 | 0.00761 | pass |
| Windows 历史截图（M45/M52，同一界面深浅两套，1180x760） | 529~699 | 20.29~27.46 | 0.0186~0.0433 | pass |
| 人为纯白 / 纯黑（空白帧） | 1 | 0.0 | 0.0 | fail |
| 最弱内容（1 像素黑边框 + 一条中线） | 2 | 19.09 | 0.00169 | fail（卡唯一色数） |
| **阈值** | ≥64 | ≥3.0 | ≥0.001 | — |

阈值留 **5~8 倍**余量（342/64 = 5.3×、19.45/3.0 = 6.5×、0.00761/0.001 = 7.6×）：换主题/换字体不会误报，
空白帧一定红。

第三项用**强边缘占比**（水平相邻像素亮度差 ≥8 的比例）而不是平均亮度差：空数据目录的主界面 57% 是同一片
底色，平均差被稀释到 0.4，强边缘只数真正的边界，跨主题稳定（同一界面深浅两套差 2 倍，仍都过线）。

**这道闸门的下限 ≈「至少画出一条标题栏」**（review 实测切片）：只画 1180×40 标题栏的切片（174 / 35.10 /
0.02623）仍 pass，右下 400×300 的切片也 pass。所以它证明的是「场景被真实渲染过」，**不证明界面元素齐全、
更不证明视觉正确**；视觉正确只能靠人眼看 artifact。

## 3. 覆盖哪个平台

只有 Linux x64（X11 + Xvfb + 软渲染）。

- **macOS**：`runtime-smoke-macos` 只在 `workflow_dispatch` 触发且 `continue-on-error: true`。官方 macOS
  runner 处于 LaunchDaemon 上下文、无登录 WindowServer 会话，winit 大概率建不了窗口；本机没有 macOS
  且 worker 禁 push，这条路径**尚无实测证据**，故不拿它卡 PR 门禁。首跑定去留：绿 → 删 `if` 与
  `continue-on-error` 升常驻；红 → 错误原文进 progress，按 D26 归入 iced 重写再规划。
- **Windows**：不新增 job——`--software-renderer --screenshot` 已在本机验收里多次人眼验证（M45/M52）。

## 4. 本地复跑

```bash
bash packaging/linux-smoke-docker.sh . D:/tmp/ull-smoke
```

用 Docker（`rust:1-bookworm` + TUNA apt/rustup 源）复刻同一套步骤，截图落在 `<输出目录>/runtime-smoke.png`。
**输出目录别写 `/tmp/...`**：Git Bash 里 `/tmp` 是 MSYS 虚拟路径，`docker -v` 会把它当 Linux VM 内的路径，
结果是「脚本说成功、宿主机什么都没有」。脚本对 `out_dir`/`repo_root` 统一做 `cygpath -m` 归一，并回显真实
路径；没有 `cygpath` 的平台（Linux/macOS）按原样用。

诚实边界：本机容器是 Debian 12（glibc 2.36），官方 runner 是 Ubuntu 24.04（glibc 2.39），图形栈来源不同，
容器绿只证明「这条通道能跑」，官方 runner 首跑仍要单独看。

## 5. 已知跳过项与迁移风险

**跳过项**（D26 收窄，本条只做「框架无关的最小运行时冒烟」）：Wayland（`weston --headless`）、
`--screenshot-page` 子页覆盖、gsettings 主题探测回退都不在本 job 内，随 iced 重写另行规划。

**迁移风险**：本 job 绑死 Slint 的截图通道（`Window::take_snapshot`）。**UI 迁 iced 后必须重新验证**——
iced 没有等价快照 API，届时要提供「离屏渲染出一帧 PNG」的等价通道，否则本 job 会静默退化成「能起进程」
而不是「能出图」（它本来也只证明「能出图」，不证明视觉正确）。
