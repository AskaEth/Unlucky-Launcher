//! 失速诊断与下载的每一处上界（M289 / 台账 P19）。
//!
//! # 事实：下载引擎曾是全仓唯一没有超时的那条路（r1 复审 p2-1 后修正）
//!
//! 「全仓下载路径只有两处无界 `await`」那句话**是错的**，被复审用一个 `silent()` 探针
//! 当场打穿。下载引擎**自己那条路**上，M289（含 r1）收口的是**五处 `await`**，落在**四条**
//! 位置——`bytes.rs` 那条里有**两处**，所以「数 `await` 是五、数 bullet 是四」：
//!
//! 1. 单连接请求的 `send()`（`transfer.rs`）；
//! 2. 分块请求的 `send()`（`chunked.rs`）；
//! 3. `download_bytes` 的 `send()`（`bytes.rs`，生产路径：版本清单 / 版本 JSON /
//!    Modrinth 元数据）；
//! 4. 同一个 `download_bytes` 的整包读 `response.bytes()`（`bytes.rs`）——它**只管大小不管时间**
//!    （`MAX_IN_MEMORY_BYTES` 不是时间上界），r1 换成流式累加 + `super::http::read_step`；
//! 5. `acquire_owned()`（`engine.rs`）：等许可也是一次 `await`，卡死的作业攥着许可，
//!    同缓存根的 `download()` 就全堵在这里（P19 (c′) 的同族形态）。
//!
//! 这五处**没有剩下的**：M289 修掉第 1、2 条，r1 修掉第 3~5 条（原来那句「修掉了其中五处」的
//! 「其中」是错的）。口径可复现，取 M289 之前那个 tip，**三条命令缺一不可**（少给第三条就只
//! 能复现四处，第五处会像是凭空出现的）：
//!
//! 1. `git grep -n "\.send()" cc0def3^ -- crates/core/src` → `transfer.rs:136`、`chunked.rs:226`、
//!    `bytes.rs:109`（第 1~3 条）；同一次命中里还有 `region.rs:288`（已包在 `PROBE_TIMEOUT` 里，
//!    不算）、`auth/http.rs:112` 与 `auth/microsoft/preflight.rs:136`（各有界）以及模块外那四条
//!    （见下节）；
//! 2. `git grep -n "\.bytes()" cc0def3^ -- crates/core/src/download` → `bytes.rs:130`（第 4 条，
//!    整包读；另两条命中是 `str::bytes()` 迭代器，不是 IO）；
//! 3. `git grep -n "acquire_owned" cc0def3^ -- crates/core/src/download` → **唯一命中**
//!    `engine.rs:306`（第 5 条）。
//!
//! 三条合起来 = 五处。**「五处」不是全仓口径**：模块外另有四条裸 `send()`（`game/java/api.rs` /
//! `net.rs` / `api/version.rs`、`update/github.rs`），那是 M290 的事，四条都已收 —— M423 把 CF 那
//! 一处也收掉之后，当下全 core 已无无界 `send()`，见下面「全 core 的 `send()` / 整段正文读清单」。
//!
//! `build_client` 只设了 `connect_timeout`，reqwest 自己的 `timeout` 默认 `None` ——
//! 于是「服务器接了连接、不回一个字节」= 这个 future 永不结束，而下 188MB 的作业
//! 就在那里静静挂着（`ull java install` 走的是 `silent()` 引擎：零事件、零日志，
//! 用户只能看到一个不动的主界面）。
//!
//! # 哪些入口有界、谁托管给谁
//!
//! | 入口 | 上界 | 阶段 |
//! |------|------|------|
//! | `DownloadEngine::download` 单连接 | `http::send_bounded` + `http::read_step` | 等响应头 / 等字节 |
//! | `DownloadEngine::download` 分块 | 同上 + [`StallStage::AwaitChunkJoin`]（无进度窗口与总期限） | 等分块回收 |
//! | `DownloadEngine::download_bytes` | `http::send_bounded` + `read_step` 流式累加（顺带用 `MAX_IN_MEMORY_BYTES` 卡住无 `Content-Length` 的洪流） | 等响应头 / 等字节 |
//! | `http::text_bounded`（模块外几条「一锤子 GET 取文本」的正文） | `read_step` 空闲判定 + 头/循环两道 `MAX_IN_MEMORY_BYTES` | 等字节 |
//! | `DownloadEngine::acquire`（许可） | `EngineConfig::permit_timeout` | [`StallStage::AcquirePermit`] |
//! | 校验（`verify_part`） | `EngineConfig::verify_timeout` | [`StallStage::Verify`] |
//! | `font::fetch_blocking` | 自带的整作业预算（外加上面全部） | —— |
//! | `region::probe_reachable` | 自带的 3s | —— |
//!
//! 因此本模块内**直接写的 `.send()`** 只剩两处：本模块的 `super::http::send_bounded`（唯一
//! 构造点）与 `region.rs` 的 3s 探测（自带界）——口径
//! `git grep -n "\.send()" crates/core/src/download | grep -vE '^[^:]+:[0-9]+:[[:space:]]*//'`
//! （后半截剔掉「内容以 `//` 开头」的行：本文件的文档正文里反复引用这个模式，不剔就大半是自命中）
//! ⇒ 只剩**两条代码命中**（前者包在 `deadlines.response` 里、后者包在 `PROBE_TIMEOUT` 里）。
//! 新加下载入口时**先看这张表**，别再出现第二个 `download_bytes`。
//!
//! # 全 core 的 `send()` / 整段正文读清单（M290 → M423 收口；**当下状态**）
//!
//! 这是**当下**的清单，不是「以后也不会再有无界 `send()`」的保证：表与代码在同一个 commit 里，
//! 谁新写一条裸 `send()` 或裸 `Response::text()`，它当场过期。四条口径都在当前 tip 上跑过：
//!
//! 1. `git grep -n "\.send()" crates/core/src` —— 逐条判「代码 / 注释·用例」后**只剩四条代码
//!    命中**：`download/http.rs`（`send_bounded` 本体）、`download/region.rs`（自带
//!    `PROBE_TIMEOUT`）、`auth/http.rs`、`auth/microsoft/preflight.rs`（各自的上界见下面第一张
//!    表）；剩下的命中都不是生产代码：本文件的文档正文（本节就在引用这个模式）、
//!    `addon/curseforge/tests/transport.rs`、`game/java/api/version.rs` 的 `bounded_probes` 用例、
//!    `game/java/net.rs` 的变异说明；
//! 2. `git grep -n "\.bytes()" crates/core/src/download` —— **零条 IO 命中**：一条是 `bytes.rs`
//!    里「不能再退回 `response.bytes()`」的注释，两条（`font/spec.rs` / `types.rs`）是
//!    `str::bytes()` 迭代器，其余是本文件的文档正文；
//! 3. `git grep -n "acquire_owned" crates/core/src/download` —— **唯一命中** `engine.rs`（第五处
//!    `await`，包在 `permit_timeout` 里；`Semaphore` 在 core 里也只有 `download` 用）；
//! 4. `git grep -n "\.text()" crates/core/src` —— 唯一代码命中在 `auth/microsoft/preflight.rs`
//!    （与它那条 `send()` 同属一次请求、同一个整体 deadline）；`microsoft.rs` 那条是版本串断言，
//!    `download/http.rs` 与 `game/java/net.rs` 是注释，其余是本文件的文档正文。
//!
//! 四条合起来 ⇒ 全 core **没有**「既无自身上界、又没有外部 deadline」的 `send()`、整段正文读，
//! 或等许可的 `await`：
//!
//! | 位置 | 现状 |
//! |------|------|
//! | `download/http.rs`（`send_bounded` 本体） | 有界：`timeout(deadlines.response, request.send())`（本模块内，上一节） |
//! | `download/region.rs` | 有界：自带 `PROBE_TIMEOUT` 3s（本模块内，上一节） |
//! | `auth/http.rs` | **可以留**：RequestBuilder 自己挂了 `.timeout(REQUEST_TIMEOUT)`，reqwest 的该超时覆盖到响应体读完 |
//! | `auth/microsoft/preflight.rs` | **可以留**：`probe()` 的 `send()` / `text()` 由 `check_with_deadline` 的整体 deadline 套住 |
//! | `download/engine.rs`（`acquire_owned`） | 有界：`EngineConfig::permit_timeout`（本模块内，上一节） |
//!
//! 下面这几处**当前 tip 已经不在上面那四条 grep 的代码命中里**（已改成带界调用）：列出来是为了
//! 回答「这条路原来有谁、谁收的」，不是还欠着的债。
//!
//! | 位置 | 现状 |
//! |------|------|
//! | `game/java/api.rs`（Adoptium latest 查询） | 已收：`http::send_bounded` + `http::text_bounded`（M290） |
//! | `game/java/net.rs`（四家发行版的一锤子 GET） | 同上（M290） |
//! | `game/java/api/version.rs`（GA 清单翻页） | 同上（M290 r1；这一处是**只数 `send()`** 那条序数下的第四处：`transfer` → `chunked` → `bytes` → 这里，与上面那五处 `await` 不是同一套序数） |
//! | `update/github.rs`（更新清单） | 同上；界取 `engine.config().deadlines()`（M290） |
//! | `addon/curseforge/transport.rs`（CF 的六个调用点，落在五个 URL builder 上：`mod_url` 被 `project` / `project_class` 各用一次） | **已收**（M423）：`EngineTransport::get` 的两处 `await` 各接一档 —— ①`builder.send()` → `http::send_bounded`（发出请求 → **响应头到手**）；②`response.text()` → `http::text_bounded`（正文按**等下一块字节**的空闲判定累加）。两处界都取 `self.engine.config().deadlines()`：前者用 `Deadlines::response`、后者用 `Deadlines::read_idle`；不设「整次检索总预算」 |
//!
//! 这几条**已收**的文本路径只用 `await_headers` / `await_bytes` 两档 —— **M423 的 CF 也算在内**：
//! `EngineTransport::get` 一次检索只发**一条**请求（不分块 ⇒ 没有分块回收），也不取引擎的下载许可
//! （只读 `engine.config()` 与 `engine.client()`）⇒ `await_chunk_join` / `acquire_permit` 在这里
//! 不适用（不是「没做到」，是「不适用」）。
//!
//! # 取舍：三段可以共存的界，**不是**「整作业必须在 N 秒内下完」
//!
//! 两个先例为什么取值天差地别，看它们的作业有多大：
//!
//! * [`super::region::probe_reachable`] 的 3s（`PROBE_TIMEOUT`）—— 探测一个**极小响应**
//!   （`Range: bytes=0-0`）。整个作业就是一次只用拿几个字节的 GET，所以「单次请求上界」与
//!   「整作业预算」在这里是同一件事，3s 对两者都成立；它紧是因为**慢探测本身就是坏结果**。
//! * `font.rs::ATTEMPT_DEADLINE` / `font.rs::TOTAL_DEADLINE` —— 字体是几百 KB 的
//!   单文件，整作业几秒就完，所以能一刀切死「慢」；代价是它必须对最后一个候选放宽
//!   （每个候选都从 0 字节重下，单次上限就是一条吞吐门槛，卡死最后一个等于把
//!   「慢但能下完」判成永久失败——那里的注释已经记了这笔账）。
//!
//! 下 188MB 的 JDK 与两者都不同：整作业预算必须 ≥ 慢链路上的真实耗时，否则
//! 「慢但有效」被判死——这是加超时最常见的自杀方式。所以这里**不设**「作业必须 N 秒
//! 内下完」，而设三段互不顶替的界：
//!
//! 1. [`StallStage::AwaitHeaders`]（`EngineConfig::response_timeout`，**单次请求**）：
//!    `send()` 只到「响应头到手」为止，不含正文，所以对 188MB 的作业它是一次性开销，
//!    取多紧都不会误杀正在下的大文件；
//! 2. [`StallStage::AwaitBytes`]（`EngineConfig::read_timeout`，**空闲**判定）：
//!    字节一直在动就一直续期。每秒只挤几 KB 的链路也算「在动」，只有**真的不动**了才判失速；
//! 3. [`StallStage::AwaitChunkJoin`]（分块作业）：一个分块卡住不许 4 条连接陪着挂到天荒地老。
//!    这里同时给「无进度窗口」与「总期限」，但总期限按**平均吞吐下限**反推
//!    （[`job_budget`]），不是拍一个固定秒数。
//!
//! **不要**把它改成 `reqwest::ClientBuilder::timeout(...)`：reqwest 的 `timeout` 覆盖
//! **整个响应体**读取，那正好就是「慢但有效被判死」的那个坑（它的默认 `None` 也是 P19
//! 记的另一半事实）。每个阶段各自的界才分得清「连不上」「不回话」「不回字节」。
//!
//! # 「慢但有效」的适用边界（r1 复审 p2-2 要求写明；M292 起可在设置里改）
//!
//! 第 3 条那句「慢链路照样活」有一个**前提**，等价说法是：**平均吞吐 ≥ 32 KiB/s 才活得下来**
//! —— 32 KiB/s 是**出厂默认值**，不是不可变的事实（M292 起三个数都是设置键，见下一节）。
//! 算术：188MB 在 20 KiB/s 上要 160.4 min，而出厂预算只有 100.3 min ⇒ 必被杀。而且一旦被杀，
//! `DownloadError::job_budget_exhausted()` 让引擎**不再重试、也不再换源**（同一份字节、
//! 同一条链路，候选源不可能更快）——所以在 <32 KiB/s 的链路上，这个文件大小是「下不完」，
//! 不是「慢一点」。
//!
//! 这条边界的命中面不远：仓内就有 ≈67 KB/s 的真机观测（`font.rs::ATTEMPT_DEADLINE` 那张实测表；
//! `font/tests/support.rs::serve_trickle` 的注释里也复述了这条）。
//! 真实公网慢链路的取值**没有真机验证**（M289 报告列为未验项，M292 也没有补）。
//!
//! # 改这几个数要付的代价（**全仓只写这一处**：键的 `doc` 与 `download::config` 都只指回这里）
//!
//! 三个键（登记在 `setting::keys::download`）在引擎侧落成
//! `EngineConfig::{min_avg_throughput, chunk_stall_timeout, response_timeout}`，通路是
//! `DownloadTuning::apply_to`；**不设 = 与 M289 逐值相同的出厂值**，所以「读这一节」只在真的
//! 填了数字之后才有意义。
//!
//! * **把吞吐下限调低**（`download.throughput_floor_kib`）：慢链路能下完 —— 20 KiB/s 的用户把
//!   它设成 16，188MB 的预算从 100.3 min 变成 201 min。代价是「真挂住」的作业也被允许占着许可
//!   **更久**：总期限是 `总字节 / 下限`，下限减半 ⇒ 期限翻倍。它换到的是「慢」而不是「无界」：
//!   字节完全不动仍然由 `stall_timeout_secs` 那一档兜（那是 `NoProgress`，与总期限是两个原因）。
//! * **把无进展窗口 / 单次请求上界调大**（`stall_timeout_secs` / `response_timeout_secs`）：
//!   检测变慢。这两档都是**单次尝试**的界，所以最坏等待是
//!   `Σ(候选数 × 每源尝试次数 × 界)` —— 600s 的窗口（本键上界）× 3 次重试 × 6 个候选就是 3 小时。
//!   上界常量（`download::config` 的 `MAX_*`）就是这道兜底：比它更大的值在**写入口**被 schema 拒、
//!   引擎侧再回落一次出厂值，所以「窗口 24 小时」写不进去，不存在「调大等于无界」的那条路。
//! * **与未暴露的 `permit_timeout`（120 min）的耦合**：那个数是按「出厂下限下的**单次尝试**预算」
//!   反推的。下限调到 8 KiB/s 时 188MB 的单次预算变成 400 min > 120 min，于是
//!   `download.concurrency = 1` 时**排队**的那条可能等不到许可（正在下的那条不受影响，它已经
//!   持有许可）。这是已知的耦合，M292 没解决（暴露 `permit_timeout` 或让它随预算派生是另一个
//!   决定）；命中前提是「把下限调得很低 **且** 并发很小 **且** 单文件很大」。
//!
//! # 数字从哪来（默认值全仓只写这一处；能不能改也写在这里）
//!
//! * `response_timeout` = 60s（**可改**：`download.response_timeout_secs`，10~600s）：连接已有
//!   10s 的 `connect_timeout` 兜底，这 60s 是**连接建立之后**等响应头的余额——CDN 排队、
//!   回源冷启动都在几十秒量级；响应头本身只有几百字节，能回的话 60s 内必回。
//! * `read_timeout` = 30s（既有值，不动；**没有**设置键）：空闲判定，持续有字节就永远不触发。
//! * `chunk_stall_timeout` = 120s（**可改**：`download.stall_timeout_secs`，30~600s）：4 条连接
//!   **整体**一个新字节都没有两分钟，只可能是分块任务卡住（写盘卡在杀软/网络盘上时，读超时
//!   不会响，只有这个会）。
//! * [`MIN_AVG_THROUGHPUT`] = 32 KiB/s（**可改**：`download.throughput_floor_kib`，4~4096 KiB/s）：
//!   分块作业的总期限 = 总字节 / 这个数（188MB ≈ 100 分钟）。适用边界与代价见上一节。
//! * `chunk_job_floor` = 600s（**没有**设置键）：小作业的下限。吞吐算式对几 MB 的文件没有意义
//!   （`8MB / 32KiB/s` 才 4 分钟，太紧），所以小作业一律拿到这 10 分钟。
//! * `permit_timeout` = 120min（**没有**设置键）：等许可的上界。它必须 **≥ 单次尝试的总期限**
//!   （最坏 ≈ 100 min 预算 + 120s 校验），否则 `download.concurrency = 1` 时正常排队
//!   会被误杀；它挡的是「许可被永久占住」（M289 之前每一处无界 `await` 都能造出来）。
//!   **口径是「一次尝试」，不是「一个作业」**（P21 第 3 条）：一个 `download()` 调用的最坏时长是
//!   `Σ(候选数 × 每源尝试次数 × 每次尝试上界)`，理论上十几小时——但预算耗尽会让引擎不再重试、
//!   也不再换源（`job_budget_exhausted` 短路整个作业），所以那条算术走不到底。
//!   与调低的下限之间的耦合见上一节最后一条。
//! * `verify_timeout` = 120s（**没有**设置键）：188MB 的 SHA-1 在正常盘上不到 1s，120s 只抓
//!   「整文件重算摘要被卡住」（网络盘掉线、杀软锁文件），不参与慢链路判定。
//!
//! # 口径（读这两个数之前先看这里）
//!
//! `received` / `expected` 是**该阶段自己的口径**：等字节 = 这条连接收到多少 / 该连接
//! 被要求收多少（分块就是那一段的长度）；等分块回收 = 整个作业收了多少 / 作业总长；
//! 等许可 = 都还没有（`received = 0`、`expected` 是这次请求的期望大小）。

use std::fmt;
use std::time::Duration;

/// 分块作业的**出厂**最低平均吞吐（字节/秒）：整作业总期限按它反推（见 [`job_budget`]）。
///
/// 这是 `download.throughput_floor_kib` 的默认值来源（`download::config` 的
/// `DEFAULT_THROUGHPUT_FLOOR_KIB` 由它算出）。用户改过之后 [`job_budget`] 收的是
/// `EngineConfig::min_avg_throughput`，**不是**这个常量——别把它当成「引擎一定用的值」。
pub const MIN_AVG_THROUGHPUT: u64 = 32 * 1024;

/// 卡在哪一段（诊断字段；`as_str` 是稳定 id，`label` 是给人看的中文）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StallStage {
    /// 连接还没建立（DNS / TCP / TLS）。
    Connect,
    /// 连接已建立，等响应头（`send()` 的上界）。
    AwaitHeaders,
    /// 响应头已到，等下一块正文字节（读空闲判定的上界）。
    AwaitBytes,
    /// 还没开始下载：在等引擎的并发许可（`Semaphore` 队列）。
    ///
    /// 单独一档而不是塞进 `AwaitBytes`：这里上游根本还没轮到我们，「服务器不给数据」与
    /// 「引擎不给我们许可」是两种完全不同的故障（复审 r1 p2-1：挂死的作业攥着许可会把
    /// 同缓存根的 `download()` 全堵死）。
    AcquirePermit,
    /// 分块作业在等分块任务回收（整作业的无进度 / 总期限上界）。
    AwaitChunkJoin,
    /// 整文件重算摘要（校验阶段）。
    Verify,
}

impl StallStage {
    /// 稳定字符串 id（日志 / 诊断页用，不要拿来显示给用户）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Connect => "connect",
            Self::AwaitHeaders => "await_headers",
            Self::AwaitBytes => "await_bytes",
            Self::AcquirePermit => "acquire_permit",
            Self::AwaitChunkJoin => "await_chunk_join",
            Self::Verify => "verify",
        }
    }

    /// 给人看的中文短名。
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Connect => "建立连接",
            Self::AwaitHeaders => "等响应头",
            Self::AwaitBytes => "等字节",
            Self::AcquirePermit => "等许可",
            Self::AwaitChunkJoin => "等分块回收",
            Self::Verify => "校验",
        }
    }
}

/// 为什么判失速：期限用完，还是「一段时间没有新字节」。
///
/// 两者必须分得开：一条 1 KB/s 但一直在动的链路**没有**无进展，只是慢；
/// 而 4 条连接同时不动两分钟与「整作业平均吞吐跌破下限」是两种不同的坏法，
/// 修法也不同（前者等一等/换源，后者基本只能换源）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StallReason {
    /// 一段期限用完（等响应头 / 校验 / 整作业总期限）。
    Deadline,
    /// 已有一段时间没有任何新字节。
    NoProgress,
}

impl StallReason {
    /// 稳定字符串 id。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Deadline => "deadline",
            Self::NoProgress => "no_progress",
        }
    }

    /// 给人看的中文短名。
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Deadline => "期限用完",
            Self::NoProgress => "没有新字节",
        }
    }
}

/// 一次失速的现场：阶段 + 已收 / 预期字节 + 无进展时长（口径见模块头）。
///
/// 挂在 [`super::error::DownloadError::stall`] 上（结构化），同时由
/// [`super::error::DownloadError::stalled`] 抄一份进 `message`——因为整体失败时
/// `suppressed` 只搬 `message`，而上层（CLI / 诊断页）正靠那一行回答「卡在哪」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StallReport {
    /// 卡在哪一段。
    pub stage: StallStage,
    /// 为什么判失速。
    pub reason: StallReason,
    /// 已收字节（口径见模块头）。
    pub received: u64,
    /// 预期字节（未知时 `None`：总长不明的大文件）。
    pub expected: Option<u64>,
    /// 已经多久没有新字节。
    pub idle: Duration,
}

impl StallReport {
    /// 「一段期限用完」的现场。
    #[must_use]
    pub const fn deadline(
        stage: StallStage,
        received: u64,
        expected: Option<u64>,
        waited: Duration,
    ) -> Self {
        Self {
            stage,
            reason: StallReason::Deadline,
            received,
            expected,
            idle: waited,
        }
    }

    /// 「已 `idle` 没有新字节」的现场。
    #[must_use]
    pub const fn no_progress(
        stage: StallStage,
        received: u64,
        expected: Option<u64>,
        idle: Duration,
    ) -> Self {
        Self {
            stage,
            reason: StallReason::NoProgress,
            received,
            expected,
            idle,
        }
    }
}

impl fmt::Display for StallReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "阶段={}（{}）；已收 {} 字节",
            self.stage.label(),
            self.stage.as_str(),
            self.received
        )?;
        match self.expected {
            Some(expected) => write!(f, " / 预期 {expected} 字节")?,
            None => write!(f, " / 预期未知")?,
        }
        write!(
            f,
            "；已 {}ms 无进展；原因={}",
            self.idle.as_millis(),
            self.reason.label()
        )
    }
}

/// 两段可以共存的界（等响应头 / 等下一块字节），由 `EngineConfig::deadlines()` 构造。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Deadlines {
    /// 发出请求 → 响应头到手的上界（单次请求，不含正文）。
    pub response: Duration,
    /// 两块正文之间的空闲上界（字节在动就一直续期）。
    pub read_idle: Duration,
}

/// 分块作业的总期限：`max(floor, 总字节 / min_throughput)`。
///
/// `min_throughput` 是**字节/秒**，来自 `EngineConfig::min_avg_throughput`（出厂
/// [`MIN_AVG_THROUGHPUT`]，可被 `download.throughput_floor_kib` 改）。除零在**这一层**就挡住：
/// `0` 按 1 B/s 算（期限变成天文数字但仍是有限的 `Duration`，不 panic）——设置层的区间过滤
/// 本来就不让 0 进来，这里是纵深防御（手拼 `EngineConfig` 的调用点、老 schema 允许的旧值）。
///
/// 为什么不是固定秒数：固定值对小作业太松、对大作业太紧；按平均吞吐反推，
/// 「能下得完的慢链路」永远不会撞上它，撞上的只有「慢到已经没意义」的作业。
#[must_use]
pub fn job_budget(total: u64, floor: Duration, min_throughput: u64) -> Duration {
    // `total / 1 = total` 秒在 `Duration` 里装得下（u64 秒 ≈ 5840 亿年），不会溢出。
    floor.max(Duration::from_secs(total / min_throughput.max(1)))
}

#[cfg(test)]
#[path = "stall/tests.rs"]
mod tests;
