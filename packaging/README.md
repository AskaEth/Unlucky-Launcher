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
- （M184）`linux-x64` 已进发布名单，所以这条**同类**判据也常驻在发布流水线里：`release-build.yml` 的
  `Linux x64 运行时冒烟` 步用同一个 `check_screenshot.py` 判 release 产物的首帧，外加 `[theme]` /
  `[backend] final=… result=started` 两行日志断言。证据账（含真机 WSLg 实测输出）在
  [`packaging/docs/linux-evidence.md`](docs/linux-evidence.md)。

**本地复跑**：`bash packaging/linux-smoke-docker.sh . D:/tmp/ull-smoke`——脚本逐步复刻这个 job（同样的 apt 依赖、
`--bin unlucky-launcher-iced`、同样的 `xvfb-run … --software-renderer --screenshot --data-dir`）；输出目录别写 `/tmp`，
Git Bash 下会静默落进 Docker VM（脚本内部用 `cygpath -m` 归一）。
**M131 真跑过一次**（本机 Docker / Debian 12 容器 / 2026-11-05）：`exit=0`，截图 1180×760、`[ok] 截图非空白`。

## 12. 文件清单

| 文件 | 作用 | 行数上限 |
|------|------|---------|
| `packaging/README.md` | 本文：索引 + §11–§16（契约入口；§1–§10 见 `docs/`） | ≤400 |
| `packaging/docs/artifacts-and-version.md` | §1 产物矩阵 + §2 版本来源 | ≤400 |
| `packaging/docs/build-and-ci.md` | §3 怎么构建（本地 / CI） | ≤400 |
| `packaging/docs/runtime-deps.md` | §4 每平台运行时依赖 + §5 软渲染兜底 | ≤400 |
| `packaging/docs/profiles-and-measurements.md` | §6 Cargo 构建档位 + §7 实测 | ≤400 |
| `packaging/docs/compliance-support-boundaries.md` | §8 发行合规 + §9 支持矩阵 + §10 已知边界 | ≤400 |
| `packaging/make_artifacts.py` | 打包与校验（本地 + CI 共用） | ≤400 |
| `packaging/release-notes.md` | GitHub Release 说明模板（`{{VERSION}}` / `{{SUMS}}` 占位） | ≤400 |
| `packaging/check_screenshot.py` | 截图非空白判定（CI 门禁 + 本地手动跑，零第三方依赖） | ≤400 |
| `packaging/check_gpu_shot.py` | GPU 截图证据判定（M249）：`[shot] … renderer=wgpu` + `Selected: AdapterInfo` 的 `device_type ∉ {Cpu, Other}` + PNG 非空/尺寸对账/非空白，逐条点名；`--expect-refuse` 判拒绝降级出图；`--self-test` 跑 `tests/gpu_shot_vectors.json` 的 19 条向量（脚手架 `tests/gpu_shot_selftest.py`）。CI 接线见下一行（M253），口径与能证/不能证见 `docs/gpu-evidence.md` | ≤400 |
| `.github/workflows/gpu-evidence.yml` | GPU 证据门禁接线（M253）：`selftest` **无条件**跑 `--self-test` + 三个退出码 0/1/2；`probe` 探测可用产物与硬件适配器；`evidence` 只在 `needs.probe.outputs.available == 'true'` 时跑真判据 —— **无 GPU / 无产物时它整块显示 skipped（不是绿色 ✓）**，原因写进 job summary 并打 `::warning::`（跳过 ≠ 通过）。新开文件而不是并进 `guards.yml`：那份已 399/400 行，`release-build.yml` 正好 400。**dev 线复用**（r1 P2-1）：`dev-guards.yml` 里 `uses:` 同一份，dev 上只落 `selftest` 那一半 | ≤400 |
| `packaging/tests/gpu_shot_ci_selftest.py` | 上一条的**接线自检**（M253，r1+r2 返工）：10 条判据 —— ① 五步按 `id:` 必须在场且无条件（`vectors`/`exitcodes`/`wiring`/`detect`/`judge`，`exitcodes` 还得真断言 0/1/2）；② 自测步必须**指向本脚本且是单条命令**（`|| echo 先忽略` 无从下手）；③ **解析 `if:` 表达式求值**判「判定挂门」（`available` 取 `'true'`/`'false'`/`''` 必须真/假/假，认不出即红）；④ 两种调用形态都在场且产物来自 `needs.probe.outputs.*`；⑤ **判据步真跑两次**（正例必须 0、CPU 档必须非 0，`bash -e` 与裸 bash 两种模式）；⑥ 跳过路径**实跑 7 种输入**；⑦ `continue-on-error` 按 YAML 布尔语义判；⑧ 吞码标记表（兜底）；⑨ `probe.outputs.*` 必须逐个绑到 `steps.detect.outputs.*`；⑩ `on:` 必须同时有 `push`（含 `main`）/`pull_request`/`workflow_call`。外加 **33 条变异向量**（`tests/gpu_shot_ci_vectors.json`：r1 复审表 20 条 + 1 条补充 + r2 复审 12 条）逐条必须当场红。**不覆盖**：不自守、载体依赖：整个 `gpu-evidence.yml` 被删掉/替换后，**没有任何东西会跑这个自检** —— 守门人住在被守对象自己的文件里（`guards.yml` 已经 399/400 行，塞不进去）。求值器与 GitHub 语义的差异、判据 4 的「合法调用替换」。**自 2026-11-11 起停止加固**，扩项须由人类新开 mission | ≤400 |
| `packaging/tests/gpu_shot_ci_expr.py` | 上面判据 ③ 的**小求值器**：给 `needs.probe.outputs.available` 取 `'true'`/`'false'`/`''`，门必须分别判真/假/假 —— 门还挂不挂由**语义**决定，不由子串决定。只认 `==`/`!=`/`&&`/`||`/括号/字符串/`needs.<job>.outputs.<名>`/`true|false|null`/四个无参函数；`!` 与其余写法一律抛错（调用方按红处理）。不是 GitHub 语义的替身，是它的保守子集 | ≤400 |
| `packaging/tests/gpu_shot_ci_probe.py` | 上面判据 ⑤⑥ 的**执行脚手架**：把 `detect` / `judge` 两步的 `run:` 原文落盘、用桩 env 起 bash。`detect` 跑 7 种输入，断言 `$GITHUB_OUTPUT` 的 `available` / summary 文案 / `::warning::` / 三条「点了名却找不到 ⇒ 必须红」；`judge` 跑两次（正例必须 0、CPU 档必须非 0，两种 bash 模式），一次封掉 `\|\| echo` / `; :` / 管道 / `if !` / `true # 原行` 整类「绿而没判」。找不到能跑的 bash 即判红（不降级） | ≤400 |
| `packaging/tests/gpu_shot_ci_vectors.json` | 接线自检的变异向量：**33 条**，按**解析后的结构**注入（`step_delete`/`job_delete`/`step_set`/`job_set`/`path_set`/`trigger_set`/`run_append`/`run_replace`，`also` 可叠加），每条写明应红的判据名；来源见文件里的 `_note` | ≤400 |
| `packaging/tests/gpu_shot_vectors.json` | `check_gpu_shot.py --self-test` 的向量：19 条（红 16 / 绿 3），每条给出**期望的失败判据清单**，数据与代码分开 | ≤400 |
| `packaging/tests/gpu_shot_selftest.py` | 上面那 19 条向量的执行器（按真实行格式手拼日志 + 用 `check_screenshot.encode_png` 现造 PNG）：判据脚本自己守 ≤400 行，脚手架放这里 | ≤400 |
| `packaging/check_publish_slots.py` | 发布名单守卫（M164/M184）：名单 ⇄ 产物两个方向 + `mode: ci` 槽位无产物的警告；`--self-test` 注入 11 条回归 | ≤400 |
| `packaging/check_pe_subsystem.py` | PE 子系统守卫（M196）：**读产物 PE 头的 `Subsystem`**（GUI 必须 2 / CLI 必须 3），非 2 即红；`--self-test` 注入 8 条回归（红 6 / 绿 2）（见 §15） | ≤400 |
| `packaging/check_test_source_off.py` | 测试通道守门（M169）：workflow / 打包脚本 / `Cargo.toml default` 都不许打开 `update-test-source`；归属按 `jobs.*.steps[]` 列表项判、默认拒绝；`--self-test` 按 `tests/guard_vectors.json` 注入 | ≤400 |
| `packaging/check_test_key_off.py` | 测试公钥守门（M190 r3）：把公开的测试公钥并进信任表的那条 feature 只许出现在 `crates/core/Cargo.toml` 自己的声明与 `crates/presenter/Cargo.toml` 的 `[dev-dependencies]` 段内，其余（workflow / 打包脚本 / 根 `Cargo.toml` / `.cargo/config*.toml` / presenter 的 `[dependencies]`）出现即红；`--self-test` 注入 7 条向量（5 红 / 2 绿）+ 基线；其中一条专治「core 的声明里偷偷带子 feature」（`update-test-key = ["update-test-source"]` ⇒ 红） | ≤400 |
| `packaging/tests/guard_vectors.json` | 守门自测向量（基线 + 32 条：26 红 / 6 绿），数据与代码分开 | ≤400 |
| `packaging/check_signing_ready.py` | 签名就绪度播报（M255）：**只读**、零第三方依赖，三行读数 —— ① `PRODUCTION` 表空不空（解析 `PINNED-KEYS` 标记之间）② 环境里有没有钥（`ULL_MANIFEST_SIGNING_KEY` / `--key`，顺手对账「这把钥在不在表里」，**种子一个字节都不打印**）③ 最近一次发布的 `.sig`/清单痕迹（取不到写「未知」，不写「无」）。退出码 `0` 就绪 / `1` 未就绪（打印「这会让已安装用户永远验不过（fail-closed）」+ 一行修复命令）/ `2` 用法错（含标记丢了 —— 那时形不成判断） | ≤400 |
| `packaging/tests/signing_ready_selftest.py` | 上一条的判据自测（M255）：表空必 `exit 1` 且三行都打印 / **副本**夹一条假 `PinnedKey` 变 `exit 0`（不碰真实 `trust.rs`）+ 不泄种子 / ② 对账 / ③ 有-无-未知（含 `.sig` 子串假读数）/ **`--key` 读不了 = 2**（不许 traceback + 1）/ **非 main ref 的 ② 写「未判定」不写「没配」** / 输出里的 `§N` 必须是 runbook 真实章节 / `--line` 写 summary / **两个方向的变异重放**（「退出码永远 0」⇒ 判据①必须红；「永远 1」⇒ 判据②必须红）。不覆盖：整个 workflow 被删后没人跑它（与 `gpu_shot_ci_selftest.py` 同一条边界） | ≤400 |
| `packaging/tests/signing_ready_wiring_selftest.py` | 上上条的**接线**自测（M255 r1 分出，只为 400 行红线）：没有别的 workflow `uses:` 它 / 全文无 error 级注解 / 就绪度那步 `rc=$?` 后 `exit 0` / 文件头两句口径都在 / **生产种子只在 `refs/heads/main` 注入**（注入行必须是 `github.ref == 'refs/heads/main' && secrets.…` 条件式；`on:` 块里不许出现 `pull_request`；非 main 必须给 `ULL_SIGNING_KEY_WITHHELD` 原因串）/ **文档指针存在性**（`signing-runbook.md` 与 `signing.md` 里的 `packaging/…`、`crates/…`、`.github/…` 路径必须真在仓库里）。三处变异重放：去掉 ref 门 / 加回 `pull_request` / 改坏文档路径 ⇒ 对应用例必须红 | ≤400 |
| `.github/workflows/signing-status.yml` | 就绪度**播报**接线（M255）：把三行搬进 job summary。**不是门禁**：未就绪用 `::warning::`、就绪度那步**恒 `exit 0`**、**不应被设为 required**、刻意不声明 `workflow_call:`（别人没法 `uses:` 它，接线自检另外扫全仓防「顺手接进 dev 线」）。**安全边界（r1 P2-1）**：`ULL_MANIFEST_SIGNING_KEY` 只在 `refs/heads/main` 上注入（条件式 env），且**没有 `pull_request` 触发** —— 仓库内分支的 PR 拿得到 Secret 而 CI 跑的是 PR 自己的代码，种子进过那种运行等于交给任何能推分支的人；非 main ref 由 `ULL_SIGNING_KEY_WITHHELD` 说明原因，于是 ② 读「未判定」不是「没配」。同 job 另有两步跑自测（会红 = 尺子坏了，与「未就绪」分开写）。新开文件而不是并进 `guards.yml`：那份已 399/400 行 | ≤400 |
| `packaging/docs/signing-runbook.md` | **人类照做**的签名手册（M255）：现状读数 → 5 步下单清单（生成钥 / Secret 名字逐字 / 贴 `PinnedKey` / 发带 `.sig` 的 Release / `--verify-pinned` 回验）→ 每步判据 → 11 条已知坑 → 发布后回验 → 播报≠门禁 → **没验的部分**（Authenticode、托管签名、真机更新端到端、本手册命令未真跑）。不含任何真实密钥，全是占位符 | ≤400 |
| `packaging/ci-runtime-smoke.md` | §11 的细节：断言口径/阈值依据/失效模式/复跑 | ≤400 |
| `packaging/docs/linux-evidence.md` | Linux 槽位的证据账（M184）：现有证据盘点、WSLg 真机实测输出、xvfb 复跑、CI 增量估算、未验清单 | ≤400 |
| `packaging/docs/gpu-evidence.md` | GPU 截图证据账（M249 + §8 CI 接线 M253：无 GPU 显式跳过、跳过 ≠ 通过、21 条接线变异）：判据逐条「抓得住什么/抓不住什么」、本机实跑退出码、**能证**（字形栅格与边缘 AA / 面板 α 与玻璃 / 两后端一致性）与**不能证**（首帧与帧率 / 整窗 α 与圆角观感，走 DWM 任何截图都拍不到）、历史 `--diff` 基线永久不可复核 | ≤400 |
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

## 14. 更新链路的测试通道（`update-test-source`，M169）

「发现新版本 → 下载 → 长度 + SHA256 双检 → 就位」这条链要能在**发布档（release）**上验收，就得让
二进制能指向本地更新源；而这条能力同时是「离任意本地目录最近的一条路」（终点是替换用户盘上的 exe），
所以它必须是**编译期**的，不是运行期设置项：

- **默认关**：`update-test-source` 是 `crates/core` / `cli` / `presenter` / `app-iced` 各一条 feature，
  默认不启用。官方发布构建不带它 ⇒ `core::update::job::default_source` 里那段分支**根本编不进来**
  （README 的「release builds do not contain that path at all」就是这么成立的）。
- **验收构建**：`cargo build --release --features update-test-source`，再配
  `ULL_UPDATE_DIR=<root>` + `ULL_UPDATE_TEST_SOURCE=1`（两档缺一不可）。`ull update check --json` 会
  如实写 `data.source: "file:<root>"`、`data.test_source: true`，人读输出另有一行点名。
- **守门**：`packaging/check_test_source_off.py`（CI：`.github/workflows/release-guard.yml`，两步：
  `--self-test` + 正式扫描）扫 `.github/workflows/**`（含复合 action）、`packaging/**` 的递归脚本、
  `Makefile`/`justfile`/`Taskfile` 与各 `Cargo.toml` 的 `default`。**唯一允许**出现该 feature 的地方是
  `.github/workflows/ci.yml` 里名为 `update-e2e` 的那一步，且该步必须带 `--test update_e2e_test`
  （文件名 / 步骤名 / 参数任一不符即红，见脚本的 `ALLOWED_STEP`）。
  归属按 **`jobs.*.steps[]` 的列表项**判（不是「最近一行 `- name:`」）：落不到具名步骤上的行——`jobs:`
  之前的孤儿行、项外键、没有 `name:` 的步骤、flow 风格步骤——**一律判红**（默认拒绝）。自测向量在
  `packaging/tests/guard_vectors.json`（32 条向量：26 条注入必须红 + 6 条合法写法必须绿，另加基线必须绿；
  条数以 `--self-test` 输出为准）。`.cargo/config*.toml` 也在扫描面里：`--cfg feature="…"` 与
  `rustc-wrapper` 都能绕过 `--features`（后者一律判红，合法用途要 review 明确同意）。
  抓不住（如实记）：变量间接拼接（`FEAT="update-""test-source"`）、**两步 `$GITHUB_ENV` 交接**
  （第一步把 feature 写进 env、第二步只用 `--features "$FEAT"`——第二步没有字面量，任何静态扫描都
  看不见，这是本守门的固有边界，不是漏判）、feature 改名后接回发布、`build.rs` 里 spawn cargo、以及
  「把别的步骤**改名**成 `update-e2e` 并带 `--test update_e2e_test` 却干别的事」。
- 验收本体：`cargo test --release -p unlucky-cli --features update-test-source --test update_e2e_test
  --test update_e2e_tamper_test`（正路 + 幂等 / 篡改摘要 + 篡改包体 + 截断）。CI 里同一个命令挂在
  `ci.yml` 的 `update-e2e` job（**只在 Windows**：夹具的「真形状」是 Windows 裸 `.exe` +
  `portable-exe`；Linux/macOS 的真实产物是 `.tar.gz` + `portable-zip`，在本模块会被直接拒）。
  **不验**真 GitHub 下载、GUI 点击与重启替换（需人工）。

## 15. 产物 PE 子系统（M196：GUI 必须 `WINDOWS_GUI`=2）

群友 2026-11-08 实测发布构建「运行时会带一个控制台窗口在底下」。根因：产物 PE 头的 `Subsystem` 仍是
CUI(3)，`grep -rn windows_subsystem crates/` 当时零命中。同一个修复 2026-11-01 做过一次（`37d3039`），
但只改了 `crates/app/src/main.rs`——那棵树自 M118 起被 workspace `exclude`、不进发布矩阵，所以那一笔
**从未到达产物**。教训不是「忘了写属性」，而是没有一步去看产物本身。

- 修在哪：`crates/app-iced/src/main.rs` 顶上的 `#![cfg_attr(windows, windows_subsystem = "windows")]`。
  PE 子系统是**每个 bin 目标**的属性，`cfg_attr` 是它的原生长相；不放 `build.rs` 是因为那里要绕
  `cargo:rustc-link-arg` 间接改链接命令行，且会把「这条 bin 是 GUI」挪到看不见 `[[bin]]` 声明的地方。
- 代价与补救：GUI 子系统进程**不继承**父进程控制台，stdout/stderr 在 cmd 里是 0 号无效句柄。所以
  `main()` 第一步调 `AttachConsole(ATTACH_PARENT_PROCESS)`（`crates/app-iced/src/platform/win32/console.rs`，
  四种启动情形的实测表在该文件顶部）：重定向/管道两个句柄本来就可用（一个字节都不动）、cmd 里直接跑
  附着后输出落进控制台、双击时附着失败且**绝不新建窗口**。
- 守门：`python3 packaging/check_pe_subsystem.py --gui <exe> --cli <exe>`，读 `MZ`→`e_lfanew`(`0x3C`)→
  `PE\0\0`→COFF 头→可选头 `+0x44` 的 2 个字节，打印**文件偏移**（release 产物实测：`Subsystem=2
  (WINDOWS_GUI) @ 文件偏移 0xDC`，`e_lfanew=0x80`、PE32+）。两向断言：GUI 产物必须 2，CLI 产物
  `ull.exe` 必须仍是 3（CONSOLE）——反方向不守时，把属性贴错 crate 会被漏掉，而 `ull --version | …`
  会静默丢输出。
- 接在哪：`.github/workflows/release-build.yml`（发布侧，两个 Windows 槽位各跑一次）与
  `.github/workflows/ci.yml` 的 `build` job（PR 阶段就红，不必等到 tag）。
- 抓不住（如实记）：只看那 4 个字节，不看 GUI 双击的实际观感——「不再多一个黑框」在本仓库的验证环境
  （RDP / 无干净无控制台桌面）只能由 PE 头 + 进程/窗口枚举间接证明，人眼验收归真机。
- **既无控制台也不重定向的宿主：输出照旧无处可去，退出码不变**（M196 r2 实测，反驳 r1 复审 P2-3 的
  「退出码会变成 `EXIT_INTERNAL`(2)」——那条是**纯推理、无实测**）。两个**近似**形态 × 两条命令，
  `child_exit_code` 全是 **0**：① 父进程有 std 句柄、子进程不继承（`dwFlags=0` + `bInheritHandles=FALSE`）；
  ② 父进程 `FreeConsole()` 后把 `in/out/err` 三个 std 句柄全置 0，再起子进程（子进程侧实测
  `stdin/stdout/stderr = 0x0`、`AttachConsole` 返回 0 / `GetLastError=6`）。原因在 Rust std 的 Windows
  实现：句柄取不到时 `Write`/`flush` 直接返回 `Ok`（独立探针实测 `SetStdHandle(out)` = `0x0` / `0x1234` /
  `(HANDLE)-1` 三种取值，`write=Ok(())`、`flush=Ok(())` 全成立），所以 `crates/cli/src/runner.rs` 那条
  「写失败 ⇒ `EXIT_INTERNAL`」的分支不会被触发。结论：本 diff **没有**给 CLI 退出码引入可观察差异；
  真正的差异只有「没控制台就没人看得见输出」这件事本身。
  **边界（别把上面读成「真服务也测过」）**：①② 都只是**近似**形态——真 SCM 服务与计划任务（session 0、
  no window station）、`start /b`、Windows Terminal / ConPTY 全部**未测**；对它们的结论只能靠「句柄无效
  时 std 写返回 `Ok`」这条机制外推，不是在那些形态里亲测。第三方交叉（**复审的读数，不是我跑的**）：
  r1 复审在 r2 用更干净的形态（父 `FreeConsole()` + 三句柄置 0 + 子进程
  `DETACHED_PROCESS | dwFlags=0 | bInheritHandles=FALSE`）独立复现，产品二进制 `--version` /
  `instances list` / `--help` 全 0（`logs list` 是 1，但重定向跑同一条也是 1 ⇒ 用户错误，不是写失败）。
  想让这种情形有专属退出码/文案，得在 CLI 侧显式判一次能力（`platform::win32::console` 的
  `output_visible()` 就是现成判据），那在 `crates/cli/**`、不在 M196 的 scope 内——这里只如实记录形态。

## 16. 产物署名与清单签名（M190）

**编号来历（M203）**：本节原先也编作 §14，与 §14「更新链路的测试通道（M169）」撞号。撞号按「谁后插进来谁让位」
处理：M169 的 §14 与 M196 的 §15 都被仓库里的引用钉着（`packaging/check_test_source_off.py` 引 §14；
`.github/workflows/release-build.yml` 与 `crates/app-iced/src/platform/win32/console.rs` 引 §15），
所以本节后移编 §16 并放到文末——**内容一字未改，只改了编号和位置**。

这一节只放「去哪看」的三句话，手册本体在 **`packaging/docs/signing.md`**：

- **PE 版本资源**（Windows「属性 → 详细信息」里的发布者/产品/版权/四段式版本）：口径唯一来源
  `packaging/pe_version.rs`，被 `crates/cli/build.rs` 与 `crates/app-iced/build.rs` 各 `include!` 一次；
  判据是**读产物字节**的 `packaging/check_pe_version.py`（CI：`release-build.yml` 的 Windows 槽位）。
- **清单签名**：`manifest.json.sig` 与 `manifest.json` 并列上传，是分离的 Ed25519 签名文件（签的是清单
  的**原始字节**，不是某个规范化后的 JSON）。信任锚是 `crates/core/src/update/trust.rs` 里的
  **编译期公钥数组**（keyring）；源里没有 `.sig`、钥不在表里、签名对不上，一律 `signature_*` 分类拒装，
  **不可重试**。签名脚本 `packaging/sign_manifest.py`（零第三方依赖，`--self-test` 跑 RFC 8032 向量）。
- **签名就绪度与操作步骤（M255）**：今天 `PRODUCTION` 表在 `PINNED-KEYS` 标记之间是**空的** ⇒ 任何正式
  二进制都验不过任何清单（fail-closed）。读数一条命令：`python3 packaging/check_signing_ready.py`
  （三行 + `0/1/2`，未就绪时打印一行修复命令）；CI 里同一条读数在 `signing-status.yml` 的 job summary
  上，**那是播报不是门禁**（不应设为 required）。要动手做的那 5 步在
  [`docs/signing-runbook.md`](docs/signing-runbook.md)（不含任何真实密钥）。
- **不得混淆**：项目自有签名解决的是「更新器怎么判断这份清单是不是我们发的」，**不解决 SmartScreen**
  ——用户双击 exe 时看到的「未知发布者」要等 Authenticode 证书，那是另一条线。
