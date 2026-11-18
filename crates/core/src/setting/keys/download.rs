//! `download.*` 的七个键：四个基础键（M292 从 `keys.rs` 纯搬移拆出：那边 396 行、贴着 400 行硬线）
//! + M292 新增的三个超时/阈值键。
//!
//! 搬移范围与顺序都未改：并发 / 单源尝试次数 / 地区判定 / 镜像偏好，四项逐字搬来，主表只留
//! `.with_all(download::specs())` 一行。语义与「谁消费」仍写在各自的 `doc` 里（那是给界面与
//! AI 的 JSON Schema `description`，不是本文件的注释），所以这里不再抄第二份。
//!
//! 与 [`super::proxy`] 的分工：代理 / gh-proxy 三个键在那边（它们问的是「怎么把包拿到手」，
//! 而本文件这些问的是「怎么排队、怎么重试、从哪个源拿、允许慢到什么程度」）。
//!
//! **三个超时键的默认值与 M289 的硬编码逐值相等**（32 KiB/s / 120s / 60s），所以「不设」等于
//! 「M292 之前的行为」。**改它们的代价**只写在一处：`core::download::stall` 的模块头——本文件与
//! `download/config.rs` 都只指回那里，不各写一份。
//!
//! 三者的上界（`download::config` 的 `MAX_*` 常量）就是「把挂住变成无界」那道兜底：比它更大的值
//! 在写入口被 schema 拒、引擎侧再回落一次出厂值。

use crate::setting::spec::{AiPolicy, SettingSpec};

use super::{MIRROR_PREFERENCES, REGIONS};

/// 七个键的完整登记项（主表一行 `.with_all(download::specs())`）。
#[must_use]
pub fn specs() -> Vec<SettingSpec> {
    vec![
        SettingSpec::int(
            "download.concurrency",
            crate::download::config::DEFAULT_MAX_CONCURRENCY as i64,
            (1, crate::download::config::MAX_CONCURRENCY as i64),
            AiPolicy::silent(),
            "同时下载的文件数（1~64，出厂 8）。这是引擎真实持有的并发许可，不是展示值；越界或手改坏值按出厂值生效",
        ),
        SettingSpec::int(
            "download.retry_limit",
            crate::download::config::DEFAULT_MAX_ATTEMPTS_PER_SOURCE as i64,
            (0, crate::download::config::MAX_ATTEMPTS_PER_SOURCE as i64),
            AiPolicy::silent(),
            "单个下载源的最多尝试次数（含首次；出厂 3）。0 与 1 都等于「只试一次、不重试」——0 是旧版 GUI 提供过的档位，保留合法但不代表「不下载」。换源不受此限",
        ),
        SettingSpec::enumeration(
            "download.region",
            "auto",
            REGIONS,
            AiPolicy::audited(),
            "下载地区判定：auto（时区+语言+探测）/ cn / global",
        ),
        SettingSpec::enumeration(
            "download.mirror_preference",
            "auto",
            MIRROR_PREFERENCES,
            AiPolicy::audited(),
            "镜像偏好：auto（健康度评分）/ official（官方源优先）/ mirror（镜像优先）",
        ),
        // M292：三个超时/阈值键。默认值与 M289 的硬编码逐值相同（不设 = 当前行为），
        // 「改它们要付什么代价」只写在 `core::download::stall` 的模块头，这里只给范围与默认值。
        SettingSpec::int(
            crate::download::config::KEY_THROUGHPUT_FLOOR,
            i64::from(crate::download::config::DEFAULT_THROUGHPUT_FLOOR_KIB),
            (
                i64::from(crate::download::config::MIN_THROUGHPUT_FLOOR_KIB),
                i64::from(crate::download::config::MAX_THROUGHPUT_FLOOR_KIB),
            ),
            AiPolicy::silent(),
            "分块下载的最低下限平均吞吐（KiB/s，4~4096，出厂 32）：下载总期限 = 总字节 ÷ 这个数。调低能让慢链路下完，代价是真挂住的作业占用更久（见 core::download::stall 模块头）",
        ),
        SettingSpec::int(
            crate::download::config::KEY_CHUNK_STALL,
            crate::download::config::DEFAULT_CHUNK_STALL.as_secs() as i64,
            (
                i64::from(crate::download::config::MIN_CHUNK_STALL_SECS),
                i64::from(crate::download::config::MAX_CHUNK_STALL_SECS),
            ),
            AiPolicy::silent(),
            "分块下载「这么久没有任何新字节」就判失速（秒，30~600，出厂 120）。调大 = 卡住时报错更晚；与重试次数相乘后才是最坏等待",
        ),
        SettingSpec::int(
            crate::download::config::KEY_RESPONSE_TIMEOUT,
            crate::download::config::DEFAULT_RESPONSE_TIMEOUT.as_secs() as i64,
            (
                i64::from(crate::download::config::MIN_RESPONSE_TIMEOUT_SECS),
                i64::from(crate::download::config::MAX_RESPONSE_TIMEOUT_SECS),
            ),
            AiPolicy::silent(),
            "单次请求等响应头的上界（秒，10~600，出厂 60）：只算到响应头到手，不含正文，所以对慢链路下大文件是安全的一档",
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 七项的键名与顺序。搬移若换了顺序，`launcher_schema()` 的 `specs()` 投影顺序也跟着变
    /// （那是可见行为：`ull config list` / AI 的 JSON Schema 都按它排），而 `keys.rs` 里那行
    /// `.with_all(download::specs())` 看不出这件事。
    ///
    /// 默认值 / 区间 / 与 `EngineConfig` 的同源关系不在这里守——那是
    /// `download/config/tests.rs` 的 `the_timeout_keys_are_single_sourced_and_default_to_todays_values`
    /// （跨 crate 模块的那份口径只写一处，别在这里抄第二份）。
    ///
    /// 变异证明：把三个新键挪到 `region` 前面 ⇒ 红；把 `stall_timeout_secs` 改名 ⇒ 红。
    #[test]
    fn the_download_keys_are_registered_in_this_order() {
        let keys: Vec<&str> = specs().iter().map(|spec| spec.key).collect();
        assert_eq!(
            keys,
            [
                "download.concurrency",
                "download.retry_limit",
                "download.region",
                "download.mirror_preference",
                "download.throughput_floor_kib",
                "download.stall_timeout_secs",
                "download.response_timeout_secs",
            ]
        );
    }
}
