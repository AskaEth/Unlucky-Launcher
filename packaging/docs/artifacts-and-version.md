# 产物矩阵与版本来源（M145 拆自 `packaging/README.md`）

本文承载原 `packaging/README.md` 的 §1、§2，**节号沿用原编号**（全仓 `packaging/README.md §N` 这类引用照旧有效，
对照表在 [`packaging/README.md`](../README.md) 的索引里）。内容一字未改、只搬家：本行以上是 M145 新增的导航说明，
以下逐行取自原文件。

## 1. 产物矩阵

| 槽位 | 本版发布 | 产物文件名 | 形态 | 归档内容 |
|------|---------|-----------|------|---------|
| `windows-x64` | ✅ 发布 | `unlucky-launcher_<ver>_windows_x64.exe` | **裸 exe**（双击即用） | exe 单文件；三份合规文本作为并列 Release 资产 |
| `linux-x64` | ✅ 发布 | `unlucky-launcher_<ver>_linux_x64.tar.gz` | 单二进制归档 | `unlucky-launcher` + `LICENSE` + `NOTICE` + `README.md`；CI 在原生 `ubuntu-22.04`（glibc 2.35 地板）上构建，**并有同槽位的 xvfb 运行时门禁**（真渲一帧 + `[theme]` / `[backend] final` 日志断言 + 像素判定）；真机验收证据见 `docs/linux-evidence.md` |
| `windows-arm64` | ❌ 不发布（构建取证中） | `unlucky-launcher_<ver>_windows_arm64.exe` | **裸 exe**（同 x64 策略） | 同上；CI 在**原生** `windows-11-arm`（Windows 11 Arm64 + VS2026 镜像）上构建，**M164 起编译前体检只取证、不再判定红绿**（此前五轮全红其实全红在那个误判的体检步），**红了不挡发布**（M142，见 §3） |
| `linux-arm64` | ❌ 已构建、不发布（运行时零证据） | `unlucky-launcher_<ver>_linux_arm64.tar.gz` | 单二进制归档（原生 `ubuntu-22.04-arm` 构建，与 x64 同 glibc 地板） | 同上；M184 起**默认就编**（`mode: ci`），但没有任何 ARM64 真机跑过一帧 ⇒ 不进 `PUBLISH_SLOTS`。解锁条件 = 真机渲出一帧 + 主题日志 |
| `macos-arm64` | ❌ 不构建（未验证） | `unlucky-launcher_<ver>_macos_arm64.tar.gz` | 单二进制归档 | 同上 |
| `macos-x64` | ❌ 不构建（未验证） | `unlucky-launcher_<ver>_macos_x64.tar.gz` | 单二进制归档 | 同上 |
| 每槽位 | 随所属槽位 | `ull_<ver>_<平台>_<架构>[.exe]` | **裸 CLI 文件**（独立资产，不进 tar.gz） | 零 UI 依赖的 `ull` 本体，进同一份 `SHA256SUMS.txt` |
| 全平台 | 仅已发布资产 | `SHA256SUMS.txt` | 校验清单 | 每行 `<sha256>␠␠<文件名>`（`sha256sum -c` 可直吃） |
| 全平台 | 仅已发布资产 | `manifest.json` | **更新清单**（D50，见 `README.md` §13） | 渠道 + 版本 + 各平台产物（`target`/`format`/`url`/`size`/`sha256`）；生成器 `make_manifest.py` |

约定细节：

- **「不构建」= CI 默认根本不跑**（M139）：macOS 那两档未验证，产物不给用户、构建配额也不花；要整表六槽位取证走
  `gh workflow run release.yml -f all_slots=true`（§3）。发布白名单**是 `windows-x64` + `linux-x64`**（M184）。
  「已构建但不发布」是独立的一档（`linux-arm64`、`windows-arm64`）：它们 `mode: ci`、每轮都编，产物留在
  `dist-<slot>` artifact 里取证，裁剪步会把它们剔出 Release。
  （M164/M184）名单 ⇄ 产物两个方向都有守卫 `packaging/check_publish_slots.py`：声明了缺产物 ⇒ 红；有产物但没声明 ⇒
  `::warning::` 明说「未发布、会被裁掉」；`mode: ci` 却本次没产物 ⇒ 也打 `::warning::`（那一档红了要看得见）。
- **产物命名**：`<名字>_<版本号>_<平台>_<架构>`，下划线分段、**不带实现细节字样**（不出现 `iced`）；真相源是 `make_artifacts.py::asset_stem()`。
- **"单文件"指可执行文件本身零运行时依赖**（无 WebView、无 JRE、无 sidecar、无 DLL 旁挂）；归档里的三份合规文本是许可义务、不是运行时依赖，Windows 因为"一个 exe 装不下文本"，
  把 `LICENSE` / `NOTICE` / `README.md` 作为 Release 的并列资产分发，条款同样随下载走。
- 归档解压后是**一个顶层目录** `unlucky-launcher_<ver>_<平台>_<架构>/`，用户不会解压出一堆散文件。
- **`ull` 是并列的裸资产，不塞进 tar.gz**：headless 用户要的就是「下一个小文件直接跑」；归档里放
  LICENSE/NOTICE/README.md 是许可义务，而这三份本来就作为并列 Release 资产分发，裸 `ull` 不缺条款。
- `unlucky-launcher` 本体也认全部 CLI 子命令（M82），`ull` 不是第二个界面，而是**不链接图形栈**的同一套命令（差异见 §4 与 §7.4），两者不是重复品。
  M118：GUI 的**构建产物**名是 `unlucky-launcher-iced`（`crates/app` 已被 workspace `exclude`，不再产出 `unlucky-launcher`）；对外文件名仍是 `unlucky-launcher`，改名在 `make_artifacts.py` 落盘时（`LAUNCHER_BIN` 找产物 / `LAUNCHER_ASSET` 定对外名），所以下载链接 / `SHA256SUMS` / 本文矩阵不变。
- `<ver>` 不带前导 `v`（tag 是 `v0.2.2`，文件名是 `0.2.2`）——由脚本统一裁剪。
- UNIX 归档内 `mtime`/`uid`/`gid` 归零、权限固定（二进制 0755、文本 0644），
  同源构建的 sha256 不受打包时刻影响。
- **架构不靠猜**：脚本打包前回读二进制的可执行文件头（PE 机器码 / ELF `e_machine` / Mach-O
  `cputype`），与槽位声明的架构不符即失败——不会出现"名字是 arm64、内容还是 x64"的假产物。

## 2. 版本来源（唯一真相源）

版本号只有一个来源：**`Cargo.toml` 的 `[workspace.package] version`**。

- `<ver>` 由 `make_artifacts.py` 通过 `cargo metadata --no-deps` 读出（不是正则扒文件），
  任何一个 workspace member 版本不一致时直接失败。
- 带 tag 构建时（CI）额外传 `--version "$TAG"`：脚本裁剪 `v` 后与 `Cargo.toml` 交叉校验，
  **不一致即失败**——避免出现 tag 说 0.2.2、产物里写 0.2.1 的发布事故。
- `ull --version` 必报同一版本（脚本打包前跑一次冒烟 `ull --version` 断言版本串在其中），
  这是 `env!("CARGO_PKG_VERSION")` 的实际输出，不是估算。
- 发布流程：先改 `Cargo.toml` 版本 + 提交 → `git tag v<ver>` → 推送 tag 触发流水线。

### 2.1 里程碑快照的提交信息该长什么样（M268）

发布提交的信息**会出现在公开界面上**（commit 列表、PR、`git log`，以及——没有 `run-name` 时——Actions 的运行名），
所以它是对外文案，不是内部工作记录。

默认行为（GitHub 文档原文）：`run-name` 省略或只有空白时，运行名取「事件相关的信息」，push / pull_request 触发的
run 就是**提交信息原文**（PR 则是 PR 标题）。M268 之前 `.github/workflows/` 里 `run-name` 零命中，于是 v0.2.4 那一条的
标题直接是提交信息：「release: v0.2.4 里程碑快照（孤儿提交，无父；树 = dev 68362e5）」——D41 快照模型的实现术语
（无父提交、树取自 dev）泄漏到了任何人点开仓库就能看到的地方。v0.2.1~v0.2.3 那几条看着正常，只是因为那几次的
提交信息本身干净，不是因为流水线做了什么。

现在的兜底：`release.yml` 与 `release-guard.yml` 都加了显式 `run-name`（`Release v0.2.4` / `Release guard · main`），
发布相关的 run 名不再由提交信息决定。`release-publish.yml` / `release-build.yml` 是 `workflow_call` 可复用工作流，
**不产生自己的 run**，所以那里不加（理由写在该文件头部注释里）。**但提交信息本身照样公开可见，规矩照旧要守。**

规矩只有两条：

1. **版本 + 一句话**，例如 `release: v0.2.4`。想交代一句就写对外的话：
   `release: v0.2.4（首个带 Linux 产物的稳定版）`。
2. **不许写实现细节**：「孤儿提交」「无父」「树 = dev <hash>」「squash 了哪几个提交」这类快照模型的内部术语一个都不要。
   过程写进 `DevDocs/` 或 PR 描述，不写提交信息。

正面例子（就这样写）：

```text
release: v0.2.4
```

反面例子（真实发生过，就是泄漏到 Actions 列表的那一条）：

```text
release: v0.2.4 里程碑快照（孤儿提交，无父；树 = dev 68362e5）
```

问题不在长度，在于这段话讲的是**我们怎么做的快照**（无父提交、树取自 dev 68362e5），跟用户无关，还暴露了内部模型。

**抓得住什么 / 抓不住什么**：

- 抓得住（只有这两条）：加了 `run-name` 的两个工作流——`Release`（= `release.yml`）与 `Release guard`——的运行名不再
  由提交信息决定，Actions 列表里固定是 `Release v0.2.4` / `Release guard · main` 这类人读名字。下次谁把提交信息写成
  内部术语，**这两行不会显示它**。（已发生的那条 v0.2.4 不会追溯改名：历史 run 名是快照。）
- 抓不住（一半）：**同一份提交信息在别的 run 上照样公开**。`ci.yml` / `guards.yml` / `dev-guards.yml` /
  `gpu-evidence.yml` / `signing-status.yml` 都还是默认行为（没加 `run-name`），所以发布提交推进 `main` 时，那几行仍以
  提交信息为名。要么给它们也加 `run-name`，要么就别在提交信息里写内部术语——**目前是后者**。
- 抓不住（另一半）：**提交信息本身**。`git log` / commit 页面 / PR 里那段字照旧公开，`run-name` 管不到；这一层只有本节
  + review 时人看一眼。没有任何门禁会因此变红——写一个能判「这段话是不是内部术语」的脚本不现实，别指望它存在。
