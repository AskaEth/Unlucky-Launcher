# 打包与发布工程（packaging/）

单一真相源：**单文件产物矩阵 + 发布契约**。`.github/workflows/release.yml` 按本文实现，
`packaging/make_artifacts.py` 是本地与 CI **共用**的打包脚本（同一份代码 → 同一套目录布局）。
本目录只放发布工程资产（脚本 / 说明 / 发布说明模板），不参与任何 crate 的编译。

## 索引：§1–§12 各在哪份文件（M145 拆分）

拆分原因：本文件与 `.github/workflows/release.yml` 都已是 400 行整，下一处改动必撞全仓行数红线（`guards.yml` 的
`file-size`，单文件 ≤400 行）。按主题分开后每份都留出余量，**内容一字未改、只搬家**；**节号沿用原编号**，
所以全仓那些「`packaging/README.md` §4 / §7.4 / §9」式引用不必改动 —— 按下表找承载文件即可。

| 节 | 主题 | 承载文件 |
|----|------|---------|
| §1 | 产物矩阵 | `packaging/docs/artifacts-and-version.md` |
| §2 | 版本来源（唯一真相源） | `packaging/docs/artifacts-and-version.md` |
| §3 | 怎么构建（本地 / CI） | `packaging/docs/build-and-ci.md` |
| §4 | 每平台运行时依赖 | `packaging/docs/runtime-deps.md` |
| §5 | 软渲染兜底 | `packaging/docs/runtime-deps.md` |
| §6 | Cargo 构建档位 | `packaging/docs/profiles-and-measurements.md` |
| §7 | 实测（本机，不是估算） | `packaging/docs/profiles-and-measurements.md` |
| §8 | 发行合规 | `packaging/docs/compliance-support-boundaries.md` |
| §9 | 支持矩阵（对外口径） | `packaging/docs/compliance-support-boundaries.md` |
| §10 | 已知边界 / 后续 | `packaging/docs/compliance-support-boundaries.md` |
| §11 | CI 运行时冒烟（Linux 真渲染一帧） | 本文下方（细节在 `packaging/ci-runtime-smoke.md`） |
| §12 | 文件清单 | 本文下方 |

本文件仍是**契约的唯一真相源入口**：`.github/workflows/release.yml`（及拆出的 `release-build.yml` /
`release-publish.yml`）的注释、`make_artifacts.py` 的说明、根 `README.md` / `README.zh-CN.md` 与
`packaging/release-notes.md` 都按「`packaging/README.md` §N」引用。改契约时先改上表对应的承载文件，
再看一眼 §12 的行数预算 —— 任何一份超过 400 行都要继续按主题拆。

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
| `packaging/README.md` | 本文：索引 + §11 / §12 / §13（契约入口；§1–§10 见 `docs/`） | ≤400 |
| `packaging/docs/artifacts-and-version.md` | §1 产物矩阵 + §2 版本来源 | ≤400 |
| `packaging/docs/build-and-ci.md` | §3 怎么构建（本地 / CI） | ≤400 |
| `packaging/docs/runtime-deps.md` | §4 每平台运行时依赖 + §5 软渲染兜底 | ≤400 |
| `packaging/docs/profiles-and-measurements.md` | §6 Cargo 构建档位 + §7 实测 | ≤400 |
| `packaging/docs/compliance-support-boundaries.md` | §8 发行合规 + §9 支持矩阵 + §10 已知边界 | ≤400 |
| `packaging/make_artifacts.py` | 打包与校验（本地 + CI 共用） | ≤400 |
| `packaging/release-notes.md` | GitHub Release 说明模板（`{{VERSION}}` / `{{SUMS}}` 占位） | ≤400 |
| `packaging/check_screenshot.py` | 截图非空白判定（CI 门禁 + 本地手动跑，零第三方依赖） | ≤400 |
| `packaging/ci-runtime-smoke.md` | §11 的细节：断言口径/阈值依据/失效模式/复跑 | ≤400 |
| `packaging/linux-smoke-docker.sh` | 本地复跑 `runtime-smoke`（Docker + xvfb） | ≤400 |
| `packaging/linux-check-docker.sh` | 本地复跑 `ci.yml` 的两条 ubuntu 门禁（clippy / test，Docker） | ≤400 |
| `.github/workflows/release.yml` | tag 触发 → 选槽位（`plan`）→ 调下面两个可复用工作流 | ≤400 |
| `.github/workflows/release-build.yml` | `workflow_call`：单槽位构建（体检 / cargo / 打包 / 上传 artifact） | ≤400 |
| `packaging/make_manifest.py` | 更新清单 `manifest.json` 的生成与自验（D50，见 §13） | ≤400 |
| `packaging/tests/manifest.golden.json` | 清单 golden 样例：`make_manifest.py --self-test` 逐字节对拍 | ≤400 |
| `.github/workflows/release-publish.yml` | `workflow_call`：发布（`PUBLISH_SLOTS` 裁剪 / SHA256SUMS / 清单 / Release） | ≤400 |

## 13. 更新清单 `manifest.json`（D50）

清单作为**固定文件名**的 Release 资产挂着；客户端按 `releases/latest/download/manifest.json` 取（D50）。
字段形状的唯一权威是 `crates/core/src/update/manifest.rs` 的 `ReleaseManifest`（`deny_unknown_fields` +
逐字段校验：semver、`size > 0`、摘要长度/字符集、`target`/`format` 取值表）——生成器只负责把那套形状填对。

- **生成器**：`packaging/make_manifest.py`（零第三方依赖）。CI 在 `release-publish.yml` 里**重算
  `SHA256SUMS.txt` 之后**调它，所以清单描述的是「真正要发的资产」，不是「构建出来的全部」。
- **同源，不双写**：每个 `sha256` 直接读 `SHA256SUMS.txt`，`size` 取自同一批文件的 `stat()`；产物名里的版本段
  必须等于 `--version`（tag 与 `Cargo.toml` 漂了就没有产物可对，当场失败）。`--self-test` 用
  `packaging/tests/manifest.golden.json` 逐字节对拍，并跑 14 条负例证明严格解析不是摆设。
- **只放启动器产物**：`ull` 不进清单。DTO 的 `Artifact` 没有角色字段，`Release::artifact_for()` 取「第一个
  target 命中的产物」，把 CLI 并列写进去会让「该覆盖哪个文件」取决于数组顺序（`ull` 照旧进 `SHA256SUMS.txt`）。
- **`url` 两种形态**：默认裸文件名（相对源根，`DirSource` 口径，本地/内网用）；CI 传 `--asset-base-url`
  拼绝对地址，且**钉 tag 不钉 latest**——清单里每条都指向与它自报版本相符的字节。
- `min_supported` / `notes` 只是**可填而不默认填**：前者是「收回老版本支持」的发布策略（未拍板，写错会强制升级），
  后者在 Release 正文里已有，两处各写一份迟早漂。
- 本地复跑：`python3 packaging/make_artifacts.py --manifest` → 与 CI 同一段代码，产出 `dist/manifest.json`。

**D51 定案（2026-11-06，方案甲）+ M163 语义修正（2026-11-07）**：GitHub 的 `releases/latest/download/*`
只指向**非 prerelease** 的最新 Release。而自更新清单固定取 `releases/latest/download/manifest.json`，所以
「新版本必须被标成稳定版，latest 才会指过去」——M163 之前 push tag 默认打 `--prerelease`，v0.2.2 因此至今
`releases/latest` 404、更新通道取不到清单。现在的语义（唯一真相源 `packaging/release_marker.py`）：

| 触发 | create（新建 Release） | edit（Release 已存在） |
| --- | --- | --- |
| `push` tag `v*`（含其 Re-run） | **稳定版**（不带标记） | 不碰标记 |
| `workflow_dispatch` + `prerelease=true` | 预发布 | 显式改成预发布 |
| `workflow_dispatch` + `prerelease=false`（默认） | 稳定版 | 显式改成稳定版 |

`v0.2.2` 那个 Release **保留为历史预发布不动**，稳定通道以 `v0.2.3` 为起点。发版 checklist 第六件 =
确认 prerelease 标记与通道一致（`gh release view <tag> --json isPrerelease`），守门是
`packaging/check_release_semantics.py`（CI：`.github/workflows/release-guard.yml`）。
