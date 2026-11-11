# 发行合规、支持矩阵与已知边界（M145 拆自 `packaging/README.md`）

本文承载原 `packaging/README.md` 的 §8、§9、§10，**节号沿用原编号**（全仓 `packaging/README.md §N` 这类引用照旧有效，
对照表在 [`packaging/README.md`](../README.md) 的索引里）。内容一字未改、只搬家：本行以上是 M145 新增的导航说明，
以下逐行取自原文件。

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
| Linux x64 X11 | ✅ | 一等 | glibc 2.35+（`ubuntu-22.04` 原生构建即地板）、xkbcommon、fontconfig、libGL（或软渲染）、xcb |
| Linux x64 Wayland | ✅（同一归档；**Wayland 路径未验**） | 支持 | 上述 + wayland-client（XWayland 应用仍需 X11 那套） |
| Windows 11 ARM64 | ❌ 不发布（构建取证中） | 一等（启动器产物） | 无额外库；启动器原生运行，游戏本体走系统 x64 模拟层 + x64 JDK |
| Linux arm64 | ❌ 已构建、不发布（**运行时零证据**） | 一等（未真机验证） | glibc 2.35+（`ubuntu-22.04-arm` 原生构建，**与 x64 同地板**）、xkbcommon、fontconfig、libGL、xcb / wayland-client |
| macOS 12+（arm64 / x64） | ❌ 不构建 | 一等（未真机验证） | 无第三方库；未签名需去 quarantine |
| Windows 8.1/8/7 | — | 不支持 | 基线已上调（D21） |
| Android Termux + X11 | — | 试验级（不承诺） | CLI 与 GUI 均需源码自建；GUI 已在单机真机验证两条路径各一次（CPU 软渲染 + 自编 Mesa/turnip GPU 加速，配方见 `packaging/docs/termux-glibc-build.md`、证据见 `README.termux.md`），不在本发布矩阵内 |

`packaging/release-notes.md` 的 "Support matrix" 表与上表逐行对应（英文版同样带 published / not
published 一列，Termux 行标注 experimental）——改一边必须同步另一边，否则标题里的"一致"就是假话。

**M184 的翻牌（Linux x64 进发布名单，arm64 只构建）**：发布开关仍是 `release-publish.yml` 的 `PUBLISH_SLOTS`，
现在是 `windows-x64 linux-x64`；`release.yml` 槽位表里 `linux-x64` 与 `linux-arm64` 都是 `mode: ci`（tag 推送默认就编）。
两档的唯一差别是**运行时证据**：

- `linux-x64` 有：release 产物在 Linux 真机（Ubuntu 22.04 + WSLg 真窗口）渲出一帧、`[theme]` 与
  `[backend] final=… result=started` 两行齐全、像素判定通过；`release-build.yml` 里还有一条**同槽位**的 xvfb
  运行时门禁，每轮发布都重跑（证据与实测输出：`packaging/docs/linux-evidence.md`）。
- `linux-arm64` 没有：没有任何 ARM64 机器跑过它一帧，也没有 arm64 的 xvfb 冒烟。**解锁条件 = 真机（或原生
  ARM64 runner）上真跑出一帧 + 主题日志**，之后才把键加进 `PUBLISH_SLOTS`。构建绿了不算解锁，别拿没跑过的包发人。

`windows-arm64` 为什么还没进名单（M164 遗留）：它 v0.2.1~v0.2.3 的红**全部**红在一个编译前体检步（`find` 猜 SDK 的
`um/arm64/kernel32.lib`），`cargo build` 一次都没跑过；M164 把那个门禁降级为只取证、判定权交回编译器，但「能不能
出 PE」只能由**下一 tag 的 CI** 证明。名单 ⇄ 产物的两个方向都有守卫（`packaging/check_publish_slots.py`）：
声明了缺产物 ⇒ 红（这正是「声明为 ci 的槽位构建红了就挡住发布」那条闸），有产物没声明 ⇒ `::warning::` 明说未发布，
`mode: ci` 却本次没产物 ⇒ 也打 `::warning::`（红了要看得见）—— 所以那次 CI 真出包之后，翻一行名单即可。

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
8. **Linux 上自更新只"能查不能装"**（M184 发布 Linux x64 之后才有意义）：清单现在会带 `linux-x64` 的
   `portable-zip` 产物，而 `ull update apply` 只做便携单文件 `.exe` 的自替换
   （`crates/core/src/update/job.rs::ensure_self_replaceable`）——Linux 上它会带具名原因**拒绝**
   （"发布里的 linux-x64 产物是 portable-zip 形态，不能原地替换启动器 … 压缩包请解压后再用"），
   不会半装坏。Linux 的更新方式仍是「下载新归档覆盖旧副本」。真要做 Linux 自更新（换目录里的文件再重启）
   是另一件事，未规划。
