//! 退役守卫的**针**（M317 / P28-2：针与断言必须分居两个文件）。
//!
//! # 为什么针要单独一个文件
//!
//! 守卫的逻辑是「`sources.rs` 里不再出现自带请求头与手写 JSON 解析」。如果针与被检查的源码
//! 同住一个文件、或者针由被检查的源码自己拼出来，那么「把那段代码删掉」会连针一起删掉 ——
//! 用例照样全绿，守卫等于一句注释承诺。
//!
//! 所以：**针在这里，断言在 `retired_curseforge.rs`**。删掉/注释掉 `sources.rs` 里的东西只会动
//! 被检查的那一侧，这一侧一个字节都不变，于是断言该红就红。
//!
//! # 两种破坏都要能红
//!
//! | 破坏 | 该红的断言 |
//! |------|-----------|
//! | 把 `x-api-key` 请求头**注释掉**（`// ("x-api-key", key)`） | 下面 `FORBIDDEN` 的命中数 > 0（注释里的针也是「文件里还有那段代码」） |
//! | 把委托那段**删掉**（`client.search(..)` 没了） | `REQUIRED` 缺一条 |
//! | 把 `sources.rs` 整个换成旧实现 | 两条同时红 |

/// 被检查的源码（并列的兄弟文件：`host/addon/sources.rs`）。
pub(super) const SOURCE: &str = include_str!("../sources.rs");

/// 退役之后 `sources.rs` **一个都不许再有**的东西。
///
/// 每条都对应退役前的旧实现：请求头（`:218` 那个）、手拼的端点与查询参数、手写解析
/// （`serde_json::from_str` 与 `.get("data")` / `.get("latestFilesIndexes")` 那一片）。
/// 注意带引号：本文件的文档在讲方言时用反引号写站点字段名（反引号包着的 `x-api-key`），**代码**里才带双引号。
/// 有了引号，「注释掉的那一行」（`// ("x-api-key", key)`）照样命中 —— 那正是 P28-2 要的。
pub(super) const FORBIDDEN: &[(&str, &str)] = &[
    ("\"x-api-key\"", "请求头只许在 core 的 CurseForge::get 里拼"),
    ("\"X-API-KEY\"", "请求头只许在 core 的 CurseForge::get 里拼"),
    (
        "api.curseforge.com",
        "端点拼装归 core（curseforge::model::CF_API）",
    ),
    ("gameId=", "查询参数拼装归 core（CurseForge::search_url）"),
    ("classId=", "查询参数拼装归 core（CurseForge::search_url）"),
    ("serde_json", "JSON 解析归 core（curseforge::model::parse）"),
    (
        "Value::",
        "手写 JSON 取值归 core（curseforge::model::parse）",
    ),
    (
        "\"latestFilesIndexes\"",
        "站点字段名归 core（curseforge::model::parse）",
    ),
    (
        "\"downloadCount\"",
        "站点字段名归 core（curseforge::model::parse）",
    ),
];

/// 退役之后 `sources.rs` **必须还在**的东西：委托给 core 的那几行。
///
/// 少一条就说明文件被掏空/换回旧实现（「删掉」这种破坏由这一组抓住）。
pub(super) const REQUIRED: &[(&str, &str)] = &[
    ("unlucky_core::addon::curseforge", "CF 的客户端来自 core"),
    ("CurseForge::with_key", "有 Key 时建带 Key 的客户端"),
    (
        "CurseForge::new",
        "没 Key 时建不带 Key 的客户端（具名状态由 core 给）",
    ),
    ("CfTransport", "传输层是注入的（离线用例才喂得动）"),
    // 必须是**CF 专属**写法：裸 `.search(` 会同时命中 Modrinth 那条
    // `client.search(&search_query(..))`，于是「只删 CF 的调用、留着客户端构造」这种破坏能全绿
    // （M316 r1 复审实测 MUT-C2）。`client.search(&cf_query(` 只出现在 CF 那条线上。
    (
        "client.search(&cf_query(",
        "CF 的检索必须真的交给客户端（不是只造个客户端就返回）",
    ),
];
