# 下载设置到底改了谁（M193）

一句话：**`download.concurrency` / `download.retry_limit` / `download.gh_proxy` 真的进引擎**；
`download.region` / `download.mirror_preference` 自 **M294** 起也真的进引擎（GUI 主路径、发布版 JDK
下载页、CLI 四条命令都认，见 §5）；`download.proxy_mode` / `download.proxy_url` 自 **M295** 起接进
HTTP 客户端的代理三态（GUI 主路径、CLI 四条命令都认，**发布版 GUI 的「下载 JDK」页仍不认**，见
§3.1.1）。还剩的缺口只有 §3.1.1 表里那几个 ❌——这一页把八个下载类键逐条标出来，别再让
「设置项说假话」。

出处：M190 的 204 条 finding 复核（`DevDocs/PlanDoc/research/191-finding-triage.md` §3 A-3）挖出的
「用户能改、改了没用」缺口：schema 说并发 8，引擎恒跑 64，两处各写一个数。

## 1. 取证：八个下载类键的真实生效值

「真实生效值」是**改造前**（M193 之前）代码里实际会用的值，不是界面上写的值。

| 键 | schema 默认 / 范围 | 引擎侧消费者 | 真实生效值（改造前） | 本轮 |
|---|---|---|---|---|
| `download.concurrency` | 8 / 1–256 | **无**（恒用 `EngineConfig::default()`） | **64**（`Semaphore` 容量） | ✅ 接进引擎，范围收成 1–64 |
| `download.retry_limit` | 3 / 0–10 | **无** | 3（碰巧与 schema 相同，但改它无效） | ✅ 接进引擎，范围 0–10（0 与 1 都 = 只试一次） |
| `download.gh_proxy` | 空串 / ≤256 字符（M197 登记） | 共享引擎装配点（`shared::build` 的 gh-proxy 候选链）+ CLI 的 `download::tuning` | 地区档案里的内置值（镜像优先时 `gh-proxy.com`） | ✅ 下沉到共享装配点，GUI 与四条 CLI 命令一致 |
| `download.region` | auto / auto·cn·global | **M294 起有**：`DownloadTuning::with_source_selection` → `RegionProfile::overridden`，装配点是 `shared::build` 与 `app-iced` 的 JDK 下载页 | 永远按「时区+语言」自动判定 | ✅ 接进引擎（M294）：显式 `cn`/`global` 压过探测 |
| `download.mirror_preference` | auto / auto·official·mirror | 同上（同一个入口） | 永远按地区模式（镜像优先/直连） | ✅ 接进引擎（M294）：优先级 `mirror_preference` > `region` > 探测 |
| `download.proxy_mode` | none / none·system·manual | **M295 起有**：`DownloadTuning::from_raw` → `ProxySetting`，唯一消费点是 `http::build_client` | 只走环境变量代理（reqwest 默认）：`none` / `manual` 都不生效，`system` 只是「碰巧」 | ✅ 接进引擎（M295）：`none` = `.no_proxy()`、`system` = 跟随环境变量、`manual` = `Proxy::all(url)` |
| `download.proxy_url` | "" | 同上（**只在 manual 档被读**） | 空（从不使用） | ✅ 接进引擎（M295）；非 manual 档**忽略**（值原样留在文件里） |
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
  download.gh_proxy ────┤        │  DownloadTuning::from_raw（默认值/夹取/回落都在 core）
  download.proxy_mode ──┤        │  ← M295：两键进 RawTuning，三档翻译在 ProxySetting
  download.proxy_url ───┤        │
  download.region ──────┤        │
  download.mirror_preference ─┐  │  ← M294：with_source_selection（GUI/CLI/JDK 页共用的唯一入口）
                                 ↓
                     shared::set_tuning(<cache 根>, tuning)   变了吗？没变就什么都不做
                                 ↓ 变了 → 淘汰该根的登记项
                     shared::engine_for(<cache 根>) → shared::build
                                 ↓
                     EngineConfig { max_concurrency, max_attempts_per_source, proxy }
                     + RegionProfile { mode（源选择叠出来的）, gh_proxy }
                                 ↓
                     Semaphore(容量) / attempts_for() / MirrorTable::expand() 的候选顺序 + gh-proxy 候选
                     / http::build_client 的 .no_proxy() | 环境变量 | .proxy(Proxy::all(url))
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
| GUI 主路径的下载：内容库（Modrinth/CurseForge）、版本清单、资源/库/日志文件、字体、自更新（**不含**「下载 JDK」页，那条见 §3.1.1） | `presenter::host::engine::engine` → `core::download::shared::engine_for` | ✅ 吃（并发/尝试次数/gh-proxy 都吃） |
| CLI `ull install` | `crates/cli/src/commands/install/mod.rs:78` 的 `build_engine` → `crates/cli/src/download.rs::engine` | ✅ 吃（并发 / 尝试次数 / gh-proxy） |
| CLI `ull java` | `crates/cli/src/commands/java.rs:312` 同上 | ✅ 吃（并发 + 尝试次数 + gh-proxy） |
| CLI `ull pack` | `crates/cli/src/commands/pack.rs:286` 同上 | ✅ 吃（并发 / 尝试次数 / gh-proxy） |
| CLI `ull update`（**自更新路径**） | `crates/cli/src/commands/update.rs:64` 复用 `install::build_engine` | ✅ 吃（**并发这条路径上不可观测**：自更新只下**一个** artifact，闸门开 1 还是 8 都只有一条连接；真正随设置变的是每源尝试次数与 gh-proxy 前缀） |

四条 CLI 路径共用同一次装配：`crates/cli/src/download.rs` 从 **CLI 自己的** `launcher.json`（与
`ull config set` 同一份文件）读八个 `download.*` 键（六个数字/三态 + M294 的两个源选择枚举）→
`DownloadTuning::from_raw(..).with_source_selection(..)`（默认值/夹取/回落都在 core）→
`shared::engine_with_tuning(<cache 根>, tuning)`。所以 `presenter` 的 `shared::build` 不再是
**全仓唯一**的装配点，但装配规则仍只有 core 那一份。

### 3.1.1 「源选择」两键（M294）与代理两键（M295）的覆盖面

| 键族 | 路径 | 吃吗 | 装配点 |
|---|---|---|---|
| `region` / `mirror_preference` | GUI 主路径（内容库、版本清单、资源、字体、自更新…） | ✅ | `shared::engine_for` → `shared::build`（`tune.apply_source_selection`） |
| | 发布版 GUI 的「下载 JDK」页 | ✅（M294 r1 补） | `app-iced` 的 `pages/settings/java/downloads/engine.rs::engine_with`（独立装配点：它要 `with_progress` 拿走引擎） |
| | CLI `ull install` / `java` / `pack` / `update` | ✅（M294 r1 补） | 四条共用 `cli/src/download.rs::tuning`（`.with_source_selection`） |
| `proxy_mode` / `proxy_url` | GUI 主路径 | ✅（M295） | `shared::build`：`tune.apply_to(EngineConfig::default())` 把 `proxy` 盖上去 → `http::build_client` |
| | 发布版 GUI 的「下载 JDK」页 | ❌（M295 未接） | 那一处直接用 `EngineConfig::default()`（= `System`，跟随环境变量代理），**不读**这两个键 |
| | CLI `ull install` / `java` / `pack` / `update` | ✅（M295） | 同一个 `cli/src/download.rs::tuning`（进 `RawTuning`，宿主零字符串匹配） |

「源选择」两个键的翻译层是 `DownloadTuning::with_source_selection`（`auto` 回落、优先级、未知值
怎么退都只写在 core 一处）；代理两键的翻译层是 `ProxySetting::from_raw`（三档 + 那两句「非 manual
忽略地址」「manual 留空 = 系统代理」，同样只写在 core 一处）。两边都是：**宿主只负责把
`launcher.json` 里的键读出来并显式传下去**。


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

## 5. 源选择两键（M294）与代理两键（M295）都已接

### 5.1 已接：`download.region` / `download.mirror_preference`（M294）

两个键都落成**同一个开关**上的覆盖——`RegionProfile.mode`（引擎里「官方在前还是镜像在前」只有它，
`mirrors.rs::expand` 与 `gh_proxy_candidates` 都只认它）。优先级是一条可读规则：

```text
显式 mirror_preference（official / mirror）   ← 最具体，永远优先
  > 显式 region（cn / global）
    > 自动探测（时区 + 语言 + 系统信号）与 region.json 的 24h 缓存
```

* 分工：`region` 定「我在不在大中华区」（`greater_china` → `is_domestic()` → JDK 发行版默认序）
  与该地区默认模式；`mirror_preference` 只定「谁在前」，不动地区判定。
* 叠加原语是 `RegionProfile::overridden(mode, greater_china)`（**不是** `pinned`：`pinned` 会把
  `greater_china` 一起改掉，而显式 `official` 不该抹掉「我在国内」这个事实）。探测逻辑一行没改，
  也没有第二套探测。
* **读时叠加、不落盘**：覆盖发生在装配引擎那一刻（`shared::build` / `engine_with`），`region.json`
  里永远只有自动判定的结论，所以 24h 重判不会被覆盖层永久冻住。
* 回落：只认 schema 的那几个词（大小写/空白不敏感），其余一切（空串、`mars`、非字符串）当 `auto`，
  静默退到下一层、不报错、不中断下载。
* gh-proxy 三态在源选择**之后**叠（`apply_source_selection` 的次序），所以
  「`region=cn` + `gh_proxy=off`」⇒ 一个前缀都不生成（守卫：`an_explicit_gh_proxy_off_wins_over_a_mirror_first_region`）。

### 5.2 已接：`download.proxy_mode` / `download.proxy_url`（M295）

**M295 之前引擎侧零消费者**：`http::build_client` 只有 UA / 连接超时 / 重定向 / 默认头，**从不调
`.proxy()`**。于是 `manual`（指定地址）与 `none`（强制直连）都不生效；`system` 那一档「碰巧能用」
只是因为 reqwest 默认会读进程环境变量代理（`HTTP_PROXY` 等），不是那段代码做的。

现在两个键翻译成 `EngineConfig.proxy`（`core::download::proxy::ProxySetting`），唯一消费点是
`http::build_client`：

| `proxy_mode` | `ProxySetting` | 落到 client 上 | 含义 |
|---|---|---|---|
| `none`（schema 出厂值，键不存在也走这档） | `Direct` | `.no_proxy()` | **真直连**：连进程环境里的 `HTTP_PROXY` 也不读 |
| `system` | `System` | **不调任何代理 API** | 跟随进程环境变量（`HTTP_PROXY` / `HTTPS_PROXY` / `ALL_PROXY` / `NO_PROXY`）= M295 之前所有下载的实际行为 |
| `manual` | `Manual { url }` | `.proxy(reqwest::Proxy::all(url)?)` | 整条流量走 `proxy_url`（http 与 https 目标都经它；`Proxy::all` 顺手关掉「自动读系统代理」） |

三条决定写在这里，省得下一个人再猜：

* **`proxy_url` 在非 manual 模式下被忽略**：`none` / `system` 下它既不校验、也不进 `ProxySetting`，
  值原样留在 `launcher.json` 里（切回 `manual` 立刻生效）。模式是唯一开关，不存在「填了地址、模式
  没选对、半生效」的中间态。
* **`manual` + 空地址 = 用系统代理**：设置页那行文案一直这么承诺（「manual 留空则用系统代理」），
  核心按文案兑现，而不是落成第三种没人写过的语义。
* **非法地址不穿透**：`proxy_url` 写成解析不了的串时，`ProxySetting::apply` 返回有界错误
  （`InvalidRequest` + 带原串的可读文案），**不 panic**；`DownloadEngine::new` 据此回落**直连**重建
  （不是 `reqwest::Client::new()`——那会跟随环境变量代理，回落就不确定了），原因留在
  `DownloadEngine::proxy_problem()` 上，CLI 的 `download::engine` 打一行 stderr。配置字段**不撒谎**：
  `config().proxy` 仍是用户写的 `manual`。

**这是一次真实的行为变化**：`none` 从「碰巧跟随环境变量」变成「真直连」。所以

* 靠 `HTTP_PROXY` 上网的用户（公司代理 / CI）要在设置页选一次 `system`，否则 M295 之后下载会直连；
* 反过来，之前选过「直连」却仍在被环境变量代理的人，现在拿到的是他选的那一档。
* `region::probe_signals`（地区探测）、`auth` 的登录与发布版 GUI 的「下载 JDK」页**都不读**这两个键，
  它们仍用 `EngineConfig::default()`（= `System`，跟随环境变量）：这些路径的默认值故意与
  `download.proxy_mode` 的出厂值（`none`）不同，才没被这次接线静默改了网络路径。

`setting/keys/proxy.rs` 模块头那句「`download.proxy_mode` → 引擎的 HTTP 客户端
（`http::build_client`）」**从 M295 起是真的**（此前是空头支票）。

两条实现注记（改这段代码前先读）：

* **宿主忘了读 = 编译不过**：两个键是 `RawTuning` 的字段，两个宿主的构造都是**完整字段字面量**
  （没有 `..RawTuning::default()`），漏一个就是 `E0063`。编译器能拦「忘了读」，拦不住「读到了但
  传空」——后者由 §6 的 M20 / M21 咬。
* **`reqwest` 会替用户补 `http://`**：`Proxy::all("127.0.0.1:7890")` 是合法写法（少写 scheme 不算
  「解析不了」）。真正解析不了的只剩端口越界（`…:99999`）、方括号不闭合这一类——用例挑的是前者。

### 5.3 口令脱敏与「回落必须被看见」（M295 r1 复审 p2-1 / p2-2）

**p2-1：口令不许出现在任何用户可见 / 会落盘的字符串里。** `proxy_url` 允许带 userinfo
（`http://user:password@host:port`），而这个键在本仓就是凭据（schema 是 `AiPolicy::sensitive()`，
`ull config get` 打 `<redacted>`，`crates/cli/tests/config_secret_test.rs` 专门守它）。M295 第一版
把 `manual` 的原串整条写进 `summary()`（→ CLI stderr，进而 CI 日志 / 用户的截图）与解析失败文案，
等于把口令还原成明文，而且**不需要用户犯错**。现在：

* **唯一实现**：`core::download::proxy::redact_proxy_url`——`user:password@` → `user:***@`（用户名保留、
  口令打掉；userinfo 里没有 `:` ⇒ **逐字原样**；路径/查询里的 `@` `:` 一律不碰，也不做任何 URL 归一）；
* **四个出口都过它**：`ProxySetting::describe()`（`summary()` / CLI stderr）、`apply()` 的错误文案
  （`proxy_problem()` / GUI 提示）、手写的 `Debug`（配置与引擎被打进日志时）、`problem()`；
  宿主**不许**自己再拼一遍地址；
* 例外（刻意留着）：设置页那个**输入框**显示文件原值——它是编辑面，把 `user:***@host` 回填进去、
  用户再点「应用」就会把掩码写回配置、把真口令冲掉。文件本身也是明文，掩码的价值在输出面不在编辑面。

**p2-2：回落必须被看见。** 引擎对「解析不了 / 本构建用不了」的处置是**回落直连**（见下），M295 第一版
只有 CLI 说了，GUI 一个字都没有——用户在「直连能通、但他要求走代理（公司策略 / 隐私）」的场景下会以为
流量在代理里，实际裸奔。现在两个宿主都调**同一个判定** `ProxySetting::problem()`（与 `apply` 共用
`manual_proxy`，所以不会各说一套）：

| 面 | 形态 | 何时出现 |
|---|---|---|
| CLI | 装配引擎时一行 stderr：`下载代理未生效，本次按直连：<原因>` | 每条命令装配引擎时（`download::engine`） |
| GUI 设置页 | 「HTTP 代理」这一行的第二段（`\n已回落到直连：<原因>`） | 打开设置页即见（`host::settings::download::group`） |

**取舍（显式写下来）**：为什么是「回落直连」而不是「拒绝下载」？拒绝对用户更不挡路——下载一个坏掉的
代理地址不该让整个安装停摆，而设置字符串又很容易写错（少个端口、多了个 scheme）。代价是「用户以为走
代理」，所以配一条硬要求：**回落必须有可见提示**，两处都要有（GUI 常驻、CLI 每次装配一行）。
要改成 fail-closed 只需让 `build_client` 的错误直接冒泡，但那是产品决定，不在 M295 的范围。

**p3-1 / p3-3：四类原因自己算，不摊 reqwest / url 的原文。** 两轮补丁的教训：第一版靠
`Proxy::all` 的报错（只有一句 `builder error`），第二版靠 `url::ParseError` 的 Display（英文
`invalid port number`，而且 reqwest 的宽口径会让「解析成功/失败」本身不可靠）——两种都还是把别人的
原文当结论。现在 `validate()` 自己判四类，各类一句中文：

| 情形 | 文案（接在「代理地址用不了（<地址>）：」之后） |
|---|---|
| scheme 不是 http/https（`socks5://` 等） | `本构建不支持 socks5 代理（只支持 http / https）` |
| 缺主机名（`http://` / `http://:7890`） | `缺少主机名（host 是空的）` |
| 端口越界（`…:99999`）/ `0` / 空 | `端口 99999 不是 1–65535 之间的数字` / `端口 0 不是有效端口（要 1–65535）` / `端口是空的（host: 后面没有数字）` |
| 空地址 | `地址是空的（manual 档要填 http://host:port）` |

`validate()` 放行但 reqwest 仍拒绝的残余情形（例如 userinfo 里有非法字符）：中文结论在前，reqwest
原文只作为**括号里的证据**（`地址写法不被接受（…）`）。判据是「文案里命中该类中文原因，且不含
`builder error` / `invalid port number`」——不是「`problem()` 返回了 Some」。

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
| M14（M294） | `shared::build` 的 `tune.apply_source_selection(region)` 换回 `apply_gh_proxy(region)` | `download::preference::tests::the_settings_reach_the_engines_candidate_plan` | 红（`left: ["bmclapi","official"] / right: ["official","bmclapi"]`，4 passed 1 failed） |
| M15（M294） | `cli::download::tuning` 删掉 `.with_source_selection(..)` | `cli::download::tests::the_source_selection_settings_reach_the_cli_engine` | 红（`official 必须第一条`：`left: ["bmclapi","official"]`，0 passed 1 failed） |
| M16（M294） | `app-iced` 的 `engine_with` 删掉 `selection.apply(profile)`（换回裸档案） | `…::downloads::engine::tests::the_source_selection_settings_reach_the_jdk_download_engine` | 红（`left: ["bmclapi","official"]`，0 passed 1 failed） |
| M17（M295） | `ProxySetting::apply` 的 manual 档删掉 `.proxy(proxy)` | `…::engine_mock::proxy::manual_mode_really_sends_the_request_through_the_proxy` | 红（`AllSourcesFailed`：`.invalid` 解析不了 ⇒ 请求根本没到假代理；2 passed 1 failed） |
| M18（M295） | `ProxySetting::apply` 的 `Direct` 档删掉 `.no_proxy()` | `…::engine_mock::proxy::direct_ignores_the_env_proxy_and_system_follows_it` | 红（direct 档在教育环境变量的假代理上拿到了正文：`[112, 114, …]` = `proxied payload`） |
| M19（M295） | `ProxySetting::apply` 的 `System` 档改成 `.no_proxy()` | 同上 | 红（system 档不再跟随环境变量 ⇒ `AllSourcesFailed`） |
| M20（M295） | presenter 的 `tuning_of` 把 `proxy_mode` / `proxy_url` 传 `None`（= 没读这两个键） | `host::settings::download::tests::selection_and_proxy::the_proxy_keys_travel_from_the_file_to_the_engine_config` | 红（`left: (Direct, None)` vs `right: (System, None)`；2 passed 1 failed） |
| M21（M295） | CLI 的 `download::tuning` 同上 | `cli::download::tests::proxy::the_proxy_settings_reach_the_cli_engine_and_the_console_line` | 红（`left: Direct` vs `right: Manual { url: … }`；1 passed 1 failed） |
| M22（M295） | `ProxySetting::apply` 的 `Proxy::all(url)?` 改成 `expect(...)` | `…::engine_mock::proxy::an_unparseable_proxy_url_falls_back_to_direct_and_says_why`（+ core 单测 `proxy::tests::unsupported_schemes_and_bad_ports_explain_themselves`） | 红（两处都 panic：`代理地址: … InvalidPort`） |
| M23（M295 r1/r2） | `redact_proxy_url` 开头直接 `return raw.to_string()` | `download::proxy::tests::a_password_in_the_proxy_url_never_reaches_a_user_visible_string`（另两条同族一起红） | 红（`FAILED. 2 passed; 3 failed`）。红原文：`生效值行泄漏口令：… 代理=manual（http://user:PWMUSTNOTLEAK@127.0.0.1:7890） …`；socks 那条也一起漏（`代理地址用不了（socks5://user:PWMUSTNOTLEAK@…）`） |
| M24（M295 r1） | presenter `group()` 里 `proxy_note` 换成 `None`（= GUI 不再说「已回落到直连」） | `host::settings::download::tests::selection_and_proxy::a_broken_proxy_url_is_shown_as_falling_back_to_direct` | 红（`FAILED. 3 passed; 1 failed`）：`用不了的代理地址必须在设置页说出来（独占一行）："只在 manual 模式生效…原值保留"` |
| M25（M295 r1/r2） | `validate` 的 scheme 白名单写成 `if false`（= socks5 又变回「能用」） | `download::proxy::tests::unsupported_schemes_and_bad_ports_explain_themselves`（+ engine_mock 的 socks 用例） | 红（`FAILED. 4 passed; 1 failed`）：`本构建用不了的 scheme 必须报错` |
| M26（M295 r2） | `validate` 的端口判定关掉（`if let Some(port) = None::<&str>`） | 同上 | 红（`FAILED. 4 passed; 1 failed`）：`http://127.0.0.1:99999 ⇒ 代理地址用不了（…）：地址写法不被接受（builder error：invalid port number）`——**没有自己的判定时，reqwest 的原文就会漏出来**（就是 r2 复审看到的那个形态） |
| M27（M295 r2） | `validate` 的「主机名为空」判定关掉 | 同上 | 红（`FAILED. 4 passed; 1 failed`）：`http://:7890 ⇒ …地址写法不被接受（builder error：empty host）` |

**M20 / M21 的一条附注**：直接把那两个字段**删掉**根本编译不过——两个宿主的 `RawTuning` 是完整字段
字面量（没有 `..RawTuning::default()`），编译器会报 `E0063 missing fields proxy_mode and proxy_url`。
所以「宿主忘了读这两个键」这一档是**编译期**拦住的；M20/M21 验的是「读到了但没用」（传 `None`），
那是编译器管不着的下一半。

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
* **镜像可达性没验**（M294）：源选择两键的用例全部离线（`region.json` 预置 + `expand()`/`plan()` 断源名
  顺序），**没有向 BMCLAPI / TUNA / gh-proxy.com 发过一条真实请求**。所以「设成 `cn` 之后用户真的更快」
  这件事本页不做承诺——只承诺「候选顺序按设置变了」。真机侧同样没点过 GUI 的 JDK 下载按钮观察命中源。
* **UA 里的联系方式在真站点上的后果未知**（M207）：`User-Agent` 现在只有一份定义
  （`core::download::http::USER_AGENT` = `Unlucky-Launcher/<CARGO_PKG_VERSION> (+仓库 URL)`），
  两条 HTTP 路径发的是同一个字符串。证据只在**离线回环**上（发出去的字节 ==
  `EngineConfig::default().user_agent`）；**没有向 Modrinth / CurseForge 发过一条带新 UA 的真实请求**，
  限流阈值、CF 会不会因为多了 `(+URL)` 而拒、Modrinth 是否会因此放宽，全部未知。
* **代理只验到「明文 HTTP 真的经由代理」这一层**（M295）：判据是本地假代理（`127.0.0.1` 的
  `TcpListener`）+ RFC 2606 的 `.invalid` 目标——目标域名自己解析不了，能拿到正文就只可能是因为
  请求走了代理；代理记到的请求行是**绝对 URI**（`GET http://…/path`），那是「经由代理」的直接证据。
  **没验**：
  * 真机上的「系统代理」：Windows 注册表 / macOS 网络偏好设置里的代理**没有被读**——本仓 reqwest
    没开 `client-proxy-system` feature ⇒ `system` 档只认环境变量（`HTTP_PROXY` 等）。所以「我在
    Windows 设置里开了系统代理」这件事对 `system` 档**无效**，这是已知限制，不是本页的承诺；
  * PAC 脚本（`AutoConfigURL` / `PROXY.pac`）没测，代码里也没有任何 PAC 支持；
  * 需要认证的代理（`Proxy-Authorization` / `Proxy::basic_auth`）没测；userinfo 里的账号口令
    是否被 reqwest 转成 `Proxy-Authorization` 也没验（我们只保证**文案**里没有口令，见 §5.3）；
  * SOCKS 代理**不支持**（reqwest 的 `socks` feature 没开）。M295 r1 起它不再是「静默直连」：
    `socks5://` 会被 scheme 白名单拦下有界错误并提示「本构建不支持 socks5 代理」（§5.3），但「能用
    SOCKS」这件事仍然没有；
  * HTTPS 目标的 `CONNECT` 隧道没测：夹具只讲明文 HTTP，`manual` 档对 https 目标的行为只在
    「`Proxy::all` 会拦下 https」这一层有依据；
  * 代理自身连不上/半死时被归到 `Network` 还是 `Timeout`（以及会不会被 `attempts_for` 重试）没测；
  * **没在真的代理软件上点过**（Clash / v2rayN / 公司代理），也没在「只靠 `HTTP_PROXY` 上网」的
    环境里回归过 `none` 由「碰巧跟随环境变量」变成真直连这件事——那是一次真实的行为变化（见 §5.2）。
  * 「回落提示」只在 CLI 的真实二进制上端到端验过（`ull install --dry-run` 两行都不含口令）；
    **GUI 那一行只在单元层面验**（`group()` 产出的 `description` 文本），没在真窗口里看过它渲染、
    折行、以及它与 424 列表单宽度的相容性（设置行的渲染在 `app-iced`，不在本页 scope）。
* **发布版 GUI 的「下载 JDK」页仍不认代理两键**（M295）：那一处是独立装配点，直接用
  `EngineConfig::default()`（= `system`，跟随环境变量）。它与「并发 / 三段时间」是同一条已知缺口
  （`app-iced/…/java/downloads/engine.rs` 模块头的对照表），要接就得把那一处并进 `wanted(cache_root)`。
