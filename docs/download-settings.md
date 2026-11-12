# 下载设置到底改了谁（M193）

一句话：**`download.concurrency` / `download.retry_limit` / `download.gh_proxy` 现在真的进引擎**；
其余四个 `download.*` 键（`region` / `mirror_preference` / `proxy_mode` / `proxy_url`）**引擎侧零消费者**，
界面上改了不会改变任何下载行为——这一页把它们逐条标出来，别再让「设置项说假话」。

出处：M190 的 204 条 finding 复核（`DevDocs/PlanDoc/research/191-finding-triage.md` §3 A-3）挖出的
「用户能改、改了没用」缺口：schema 说并发 8，引擎恒跑 64，两处各写一个数。

## 1. 取证：八个下载类键的真实生效值

「真实生效值」是**改造前**（M193 之前）代码里实际会用的值，不是界面上写的值。

| 键 | schema 默认 / 范围 | 引擎侧消费者 | 真实生效值（改造前） | 本轮 |
|---|---|---|---|---|
| `download.concurrency` | 8 / 1–256 | **无**（恒用 `EngineConfig::default()`） | **64**（`Semaphore` 容量） | ✅ 接进引擎，范围收成 1–64 |
| `download.retry_limit` | 3 / 0–10 | **无** | 3（碰巧与 schema 相同，但改它无效） | ✅ 接进引擎，范围 0–10（0 与 1 都 = 只试一次） |
| `download.gh_proxy` | 空串 / ≤256 字符（M197 登记） | 共享引擎装配点（`shared::build` 的 gh-proxy 候选链）+ CLI 的 `download::tuning` | 地区档案里的内置值（镜像优先时 `gh-proxy.com`） | ✅ 下沉到共享装配点，GUI 与四条 CLI 命令一致 |
| `download.region` | auto / auto·cn·global | **无**（`RegionProfile::pinned` 无生产调用点） | 永远按「时区+语言」自动判定 | ❌ 未接（见 §5） |
| `download.mirror_preference` | auto / auto·official·mirror | **无** | 永远按地区模式（镜像优先/直连） | ❌ 未接（见 §5） |
| `download.proxy_mode` | none / none·system·manual | **无**（`http::build_client` 从不调 `.proxy()`） | 只走环境变量代理（reqwest 默认），`system`/`manual` 都不生效 | ❌ 未接（见 §5） |
| `download.proxy_url` | "" | **无** | 空（从不使用） | ❌ 未接（见 §5） |
| `download.action.detect_region` | （动作键，不是设置） | presenter 的 `detect_region` | 清 `region.json` + 失效共享引擎 | ✅ 本来就真 |

复现方法（改造前）：`grep -rn "download.concurrency" crates/` 只命中 `setting/keys.rs` 的定义与
agent 的测试；同理 `download.proxy_mode` / `proxy_url` / `mirror_preference` 只有 presenter 的显示行。

## 2. 唯一来源：默认值谁说了算

```text
crates/core/src/download/config.rs
  DEFAULT_MAX_CONCURRENCY = 8          ← 唯一写死数字的地方
  MAX_CONCURRENCY = 64                 ← 硬上限
  DEFAULT_MAX_ATTEMPTS_PER_SOURCE = 3
  MAX_ATTEMPTS_PER_SOURCE = 10
        │
        ├─→ EngineConfig::default()                 （引擎默认）
        ├─→ DownloadTuning::default()               （宿主没灌任何值时）
        └─→ setting::keys 的 schema 默认值与范围     （界面/AI/CLI 看到的契约）
```

守卫 `the_schema_contract_is_single_sourced`（`download/config.rs`，它合并了原先的「默认值相等」与
「区间同源」两条）让「改一处另一个不变」直接变红——这是防 8 vs 64 复发的唯一手段。
另有 `the_key_docs_quote_the_real_limits` 钉住 schema 描述里的数字（见 §6.1.1 表格的第三行）。

## 3. 接线链（宿主 → 引擎）

```text
config/launcher.json
  download.concurrency ─┐
  download.retry_limit ─┼→ presenter::host::settings::download::tuning_of
  download.gh_proxy ────┘        │  DownloadTuning::from_settings（默认值/夹取/回落都在 core）
                                 ↓
                     shared::set_tuning(<cache 根>, tuning)   变了吗？没变就什么都不做
                                 ↓ 变了 → 淘汰该根的登记项
                     shared::engine_for(<cache 根>) → shared::build
                                 ↓
                     EngineConfig { max_concurrency, max_attempts_per_source } + RegionProfile.gh_proxy
                                 ↓
                     Semaphore(容量) / attempts_for() / MirrorTable::expand() 的 gh-proxy 候选
```

**同步点是显式的**，两处，共用同一个函数 `apply_tuning`：

* `presenter/src/host/engine.rs::engine()` —— 每次取引擎前（幂等，值没变就一行不动）；
* `presenter/src/host/dispatch.rs` 的 `SetSetting` 分支 —— 用户改完设置立刻生效，并让常驻持有
  引擎的自更新 `Factory` 换一份（`refresh_update_factory`）。

调优按**数据根**存（与引擎登记处同键），不是进程级：同一进程里多个数据根时互不串味。

## 3.1 哪些路径吃这套设置

**四条 CLI 命令与 GUI 都吃**（M203 起。M193 时期这张表把三条 CLI 记成「自建裸引擎、零消费者」，那已不成立——教训见本节末）。

| 路径 | 入口 | 吃设置吗 |
|---|---|---|
| GUI 的**全部**下载：内容库（Modrinth/CurseForge）、版本清单、资源/库/日志文件、字体、JDK 安装、自更新 | `presenter::host::engine::engine` → `core::download::shared::engine_for` | ✅ 吃（并发/尝试次数/gh-proxy 都吃） |
| CLI `ull install` | `crates/cli/src/commands/install/mod.rs:78` 的 `build_engine` → `crates/cli/src/download.rs::engine` | ✅ 吃（并发 / 尝试次数 / gh-proxy） |
| CLI `ull java` | `crates/cli/src/commands/java.rs:312` 同上 | ✅ 吃（并发 + 尝试次数 + gh-proxy） |
| CLI `ull pack` | `crates/cli/src/commands/pack.rs:286` 同上 | ✅ 吃（并发 / 尝试次数 / gh-proxy） |
| CLI `ull update`（**自更新路径**） | `crates/cli/src/commands/update.rs:64` 复用 `install::build_engine` | ✅ 吃（**并发这条路径上不可观测**：自更新只下**一个** artifact，闸门开 1 还是 8 都只有一条连接；真正随设置变的是每源尝试次数与 gh-proxy 前缀） |

四条 CLI 路径共用同一次装配：`crates/cli/src/download.rs` 从 **CLI 自己的** `launcher.json`（与
`ull config set` 同一份文件）读三个 `download.*` 键 → `DownloadTuning::from_settings`（默认值/夹取/回落
都在 core）→ `shared::engine_with_tuning(<cache 根>, tuning)`。所以 `presenter` 的 `shared::build` 不再是
**全仓唯一**的装配点，但装配规则仍只有 core 那一份。

回落仍是静默的：`SettingStore::load` 永不失败（坏值只进 `problems()`），类型不对 → `None`，越界 → 出厂
值，不报错、不中断安装。历史教训保留在此：M193 把出厂并发从 64 改成 8 之后，当时还在用裸
`EngineConfig::default()` 的那几条命令**静默**跟着降了——「用出厂值」和「读到了设置」在输出上一模一样，
这正是 §6 里三条 CLI 变异要抓的东西。

## 4. 三态语义：`download.gh_proxy`

| 值 | 含义 | 效果 |
|---|---|---|
| 空串（出厂） | **没有覆盖**，跟随内置默认 | 镜像优先/国内判定时套 `mirrors.json` 里的 `gh-proxy.com` |
| `off`（也认 `none`/`direct`/`disable`/`disabled`/`no`） | 明确关掉 | 一个 gh-proxy 候选都不生成 |
| 其它非空 | 覆盖前缀 | 用它替换内置前缀（类别仍只在镜像优先/国内判定时启用） |

**空串 ≠ 关**：「我还没决定」和「我明确不要」是两件事。设置页的说明文案（presenter 行 + iced 行）
两档必须都写出来——这是最容易让用户以为「关掉了、其实还在套前缀」的地方。

边界：非法值（负数、超上限、手改坏值）**一律回落出厂默认**，不报错、不阻断下载；schema 在写入口
就拒绝越界值，引擎侧再夹一次是纵深防御（老文件里可能躺着旧 schema 允许的大数）。

**`download.retry_limit = 0` 是合法值**（P2-1 复审的结论）：旧版 GUI 的档位表里就有 `0`，所以
范围保留 `0~10`，语义定为 **`0` 与 `1` 都 = 只试一次、不重试**（`attempts = max(1, value)`）。
这样老配置零迁移、`ull config get` 读到的 0 与引擎真做的事一致（试 1 次），不会再出现
「界面显示 0、引擎按 3 跑」。

## 5. 没接的四个键（已登记为 finding）

`download.region` / `download.mirror_preference` / `download.proxy_mode` / `download.proxy_url`
**引擎侧零消费者**：界面上改了不改变任何下载行为。要接它们分别需要：

* `region` → `RegionProfile::pinned(mode, now)`（值 `cn`/`global` 映射到 `MirrorFirst`/`Direct`，
  并把 `trigger` 置成 `UserPinned` 让 24h 重判不覆盖它）；
* `mirror_preference` → 引擎候选排序里插一档「强制官方优先 / 强制镜像优先」；
* `proxy_mode` / `proxy_url` → `http::build_client` 里加 `.proxy(reqwest::Proxy::all(url)?)`。

三条都在 M193 的 scope 外（`http.rs` / 引擎候选排序 / 一处新装配），已单独立 finding。

## 6. 守卫与变异证明（逐条只改实现侧，跑完即还原）

| # | 变异（只改实现侧） | 守卫用例 | 结果 |
|---|---|---|---|
| M1 | `shared::build` 的 `tune.apply_to(...)` 换回裸 `EngineConfig::default()` | `download::shared::tests::the_setting_and_the_gh_proxy…` | 红（探针 8，期望 2） |
| M2 | `shared::build` 的 `tune.apply_gh_proxy(region)` 换回 `region` | 同上 | 红（`off` 档仍有 gh-proxy 候选） |
| M3 | 删掉 `set_tuning` 里的 `registry().remove` | 同上 | 红（换参数后交回旧引擎） |
| M4 | 删掉构造点的 `clamp(1, MAX_CONCURRENCY)` | 同上 | 红（9999 张许可真开了） |
| M5 | `EngineConfig::default()` 的并发写回字面量 64 | `download::config::tests::the_schema_contract_is_single_sourced` | 红 |
| M6 | schema 的并发默认值写成字面量 16 | 同上 | 红（两处各写一个数） |
| M7 | 删掉 `host/engine.rs::engine` 里的 `apply_tuning` | `host::settings::tests::the_concurrency_setting…` | 红（探针 8，期望 2） |
| M8 | 下载设置页的 gh-proxy 行改回 `("Preferred source", "Automatic (region)")` | `the_downloads_rows_state_semantics…` | 红 |
| M9 | `from_settings` 的尝试次数去掉 `.max(1)`（0 原样交给引擎） | `download::config::tests::missing_or_illegal_values_fall_back_to_the_defaults` | 红（`left: 0 / right: 1`） |
| M10 | schema 描述里的「1~64」改成「1~99」（第三处字面量） | `download::config::tests::the_key_docs_quote_the_real_limits` | 红 |
| M11 | `commands::pack::build_engine` 换回裸 `EngineConfig::default()`（其余三条不动） | `cli::download::tests::pack_engine_takes_the_concurrency_from_the_setting_store` | 红（探针 8 ≠ 2），另四条 CLI 用例绿 |
| M12 | `commands::update::engine` 换回裸 `EngineConfig::default()` | `cli::download::tests::update_engine_takes_the_concurrency_from_the_setting_store` | 红（8 ≠ 4），另四条绿 |
| M13 | `download::tuning` 不读 `KEY_GH_PROXY`（第三参传 `None`） | `cli::download::tests::the_gh_proxy_setting_reaches_the_candidate_chain` | 红（`off` 档仍有 gh-proxy 候选） |

## 6.1 gh-proxy 那一行的形状（P3-1）：M201 之后已解决

M193 时期这一组把「GitHub 加速前缀」的三态画成三行（`GitHub 加速前缀 / 留空 = 跟随内置默认`、
`关掉它 / 填 off = 关闭`、`换掉它 / 填 URL 前缀 = 覆盖`）。这是**已知的观感问题**：
①`关掉它` / `换掉它` 读起来像按钮而不是取值说明；②同一个设置画成三行比三态本身更难懂。

当时的结论是「正确的家是下一条『iced 设置数据通路』mission」——**那一条已落地（M201）**，现状：

| 项 | M193 时期 | M201 之后 |
|---|---|---|
| 控件 | 三行语义行（`&'static str`） | **一行真控件**：标题 + 说明 + 输入框 + 「应用」按钮（`pages/settings/download.rs::text_row`） |
| 三态怎么说 | 拆成三行、每行一句 | 一句话说完（留空 / off / 前缀），末尾接「当前生效：…」（presenter 的 `gh_proxy_state`，读的是**引擎生效档位**，不是文件原文，见 P3-5） |
| 当前值 | 不显示 | 输入框里是快照里那个值（`States::settings` ← `CoreEvent::SettingsChanged`） |
| 改它 | 改不了 | 输入 → 本地草稿 → 点「应用」→ `AppCommand::SetSetting` → host 写盘 + 重发快照 |

那三行**还在** `GROUP.rows` 里，但只在**没有快照**时画（harness / `pages_demo` / 首帧，走 `frame::demo_card`）：
它们说的是语义（「出厂 8 · 可调 1~64」）而不是当前值，所以不会再被当成界面上的真值（M193 用户投诉的形状）。
有快照时这一组画的是真控件，`GROUP.rows` 一行都不画。

### 6.1.1 「数字的第二来源」现状（P3-2 登记，M207 复核）

| 位置 | 能不能引用常量 | 守卫 |
|---|---|---|
| `presenter/src/host/settings/download.rs` 的行描述 | 能（`format!` + `DEFAULT_MAX_CONCURRENCY` / `MAX_CONCURRENCY`） | 已改；**有快照时界面显示的就是这一份文案** |
| `app-iced/src/pages/settings/download.rs` 的 `GROUP.rows` | 不能：`&'static [(&'static str, &'static str)]` 是编译期字面量，要改运行时生成得动 `Group` 的类型 | 只有「字符串还在」的探针（`the_downloads_rows_state_semantics_and_never_fake_a_current_value` 断言 `"1~64"` 在）：改常量不会红 |
| `setting/keys.rs` 的 `SettingSpec.doc` | 不能：`doc: &'static str`（`spec.rs:183`），`concat!` 只吃字面量、插不进常量 | ✅ `the_key_docs_quote_the_real_limits`（`config.rs`）：把「1~64」「出厂 8」「出厂 3」与常量比对，改常量不改文案就红 |

第二行的**地位变了（M201）**：它不再是产品界面上的数字来源——有快照时界面画的是第一行那份（用常量
`format!` 出来的），`GROUP.rows` 只在没有快照的兜底屏出现，而且那里的量词是「出厂 / 可调」而不是「当前」。
于是它现在的风险是「兜底演示行与常量不一致（演示屏说谎）」，不是「设置页骗人」。

`SettingSpec.doc` 那一处**不是注释**：`catalog.rs` 会把它当 JSON schema 的 `description` 发出去
（AI 工具与文档可见），所以它必须有守卫——这也是三处里唯一补上了守卫的一处。

## 7. 没验的部分

* **真实网络下的并发/重试行为未验**：守卫读的是 `Semaphore` 的真实许可数与 `attempts_for()` 的
  配置读点；「2 条真连接同时下载时服务器只看到 2 条」和「某个源真的被试 3 次」没有在真网络下跑过
  （离线回环能验连接复用，验不了限流的端到端时序）。
* **macOS / Linux 未验**：本轮只在 Windows 上编译与跑测试。
* **`download.gh_proxy` 的端到端只到「候选生成」为止**（键已合入，M197/M203 之后不再是「登记了但读不到」）：
  三处各有守卫——core 的「调优 → 候选链」（`gh_proxy_override_reaches_the_profile_and_the_chain`）、
  CLI 的「文件 → 候选链」（`the_gh_proxy_setting_reaches_the_candidate_chain`）、GUI 的「文件 → 宿主 →
  core 三态」（`the_gh_proxy_key_travels_from_the_file_to_the_core_tri_state`）。**没验的是真网络下那个前缀
  能不能通**：离线只验候选链的生成，没发过一次真的 `gh-proxy.com` 请求。
* **真机上的手点交互没验**（设置页自 M201 起已是真控件；M193 时期文档里那句「这一组只能看、改不了」已作废）：
  Downloads 组的行来自 presenter 快照（`presenter::host::settings::groups` → `CoreEvent::SettingsChanged` →
  `pages::Model.settings`），画的是 choice chip / 文本输入 + 「应用」/ 动作按钮，写回走
  `AppCommand::SetSetting`；没有快照时（harness / `pages_demo` / 首帧）才退回 `GROUP.rows` 那几行语义演示行。
  本次（M207）只读了代码与单测，没在 GPU 窗口里点过 chip / 输入 / 应用，也没量过「点了以后界面何时变」
  （设计是**不做乐观回显**，只认 host 发回的快照——这一点同样没在真机上确认过）。
* **UA 里的联系方式在真站点上的后果未知**（M207）：`User-Agent` 现在只有一份定义
  （`core::download::http::USER_AGENT` = `Unlucky-Launcher/<CARGO_PKG_VERSION> (+仓库 URL)`），
  两条 HTTP 路径发的是同一个字符串。证据只在**离线回环**上（发出去的字节 ==
  `EngineConfig::default().user_agent`）；**没有向 Modrinth / CurseForge 发过一条带新 UA 的真实请求**，
  限流阈值、CF 会不会因为多了 `(+URL)` 而拒、Modrinth 是否会因此放宽，全部未知。
