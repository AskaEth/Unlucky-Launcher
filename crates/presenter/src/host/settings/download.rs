//! 设置页「下载」分组 + 下载调优的**宿主侧翻译**（M193）。
//!
//! ## 为什么单开一个文件
//!
//! 同 `host/settings_tests.rs` 的理由：`host/settings.rs` 正好顶在 400 行硬线上，而本轮要往这一组加
//! 「`download.gh_proxy` 三态行 + 并发硬上限说明 + 调优翻译」约 24 行。不拆就得砍 `settings.rs` 里
//! 那些**留着防复发**的论证注释（`ai_group` 的 M174 回归说明、`parse_value` 的 P1-1 浮点键事故）
//! ——那比多一个文件糟得多。搬出来还有一个附带好处：这一组是**唯一**要往 core 灌下载调优的一组，
//! 代码与它的守卫（`settings_tests.rs`）现在挨在一起。
//!
//! ## 谁在读 / 谁在写
//!
//! ```text
//! config/launcher.json   ←── store.set（`settings::set`，本模块只读）
//!   download.concurrency ─┐
//!   download.retry_limit ─┼─→ DownloadTuning（core 决定默认值与夹取）─→ shared::set_tuning
//!   download.gh_proxy ───┘                                             （引擎重建时生效）
//! ```
//!
//! **调优的同步点是显式的**：`host/engine.rs` 每次取引擎前、`dispatch.rs` 的 `SetSetting` 分支里，
//! 都调 [`apply_tuning`]（幂等：值没变就什么都不做）。它**不是**藏在某个读函数里的副作用——
//! 「读起来是纯读、实际改全局状态」正是本仓吃过亏的那类形状。

use unlucky_core::download::config::{
    DownloadTuning, GhProxyOverride, DEFAULT_MAX_ATTEMPTS_PER_SOURCE, DEFAULT_MAX_CONCURRENCY,
    MAX_CONCURRENCY,
};
use unlucky_core::download::shared;

use crate::dto::{SettingGroupView, SettingRowView};

use super::super::env::HostEnv;
use super::{selected, text};

/// 并发数的推荐档位（`selected()` 找不到时回 0；手改出来的非档位值会被插进选项表，见 [`group`]）。
const CONCURRENCY_LADDER: [&str; 5] = ["4", "8", "16", "32", "64"];
/// 单源尝试次数的推荐档位（`0` 是旧版 GUI 提供过的档位，语义 = 只试一次，见 schema 描述）。
const RETRY_LADDER: [&str; 6] = ["0", "1", "2", "3", "5", "10"];

/// 下载（D7）：并发/重试/地区/镜像偏好/代理/GitHub 加速前缀。
///
/// ## P3-5：这两行显示的是**引擎生效值**，不是文件原值
///
/// `download.concurrency` / `download.retry_limit` 的生效判定在 core（[`DownloadTuning`]）：
/// 越界、负数、手改坏值一律**回落出厂值**（不夹到边界——「想 9999 给 64」仍然是界面说谎）。
/// 所以这一组必须画 [`tuning_of`] 的结论；文件里的原值只在**两者不一致**时作为一句解释出现
/// （「原值 9999 已按出厂 8 生效」）。判定的两个输入都是**现读**的：文件原值取自
/// `doc().get(..)`（显式写过的值），生效值取自 core 的翻译层 —— 没有一个数字来自常量或参数。
pub(super) fn group(env: &HostEnv) -> SettingGroupView {
    let tuning = tuning_of(env);
    let concurrency = (tuning.max_concurrency as i64).to_string();
    let retry = i64::from(tuning.max_attempts_per_source).to_string();
    let concurrency_note = applied_note(
        raw_text(env, unlucky_core::download::config::KEY_CONCURRENCY),
        tuning.max_concurrency as i64,
        DEFAULT_MAX_CONCURRENCY as i64,
    );
    let retry_note = applied_note(
        raw_text(env, unlucky_core::download::config::KEY_RETRY_LIMIT),
        i64::from(tuning.max_attempts_per_source),
        i64::from(DEFAULT_MAX_ATTEMPTS_PER_SOURCE),
    );
    let concurrency_options = ladder(&CONCURRENCY_LADDER, &concurrency);
    let retry_options = ladder(&RETRY_LADDER, &retry);
    let concurrency_options: Vec<&str> = concurrency_options.iter().map(String::as_str).collect();
    let retry_options: Vec<&str> = retry_options.iter().map(String::as_str).collect();
    let regions = ["auto", "cn", "global"];
    let mirrors = ["auto", "official", "mirror"];
    let proxies = ["none", "system", "manual"];
    let region_value = text(env, "download.region");
    let mirror_value = text(env, "download.mirror_preference");
    let proxy_value = text(env, "download.proxy_mode");
    SettingGroupView::new("download", "下载与镜像（D7）")
        .with(SettingRowView::choice(
            unlucky_core::download::config::KEY_CONCURRENCY,
            "并发数",
            &with_note(
                format!(
                    "同时下载的文件数：出厂 {DEFAULT_MAX_CONCURRENCY}，可调 1~{MAX_CONCURRENCY}（引擎硬上限，防止填 9999 把机器拖死）；越界或手改坏值按出厂值生效"
                ),
                concurrency_note.as_deref(),
            ),
            &concurrency_options,
            selected(&concurrency_options, &concurrency),
        ))
        .with(SettingRowView::choice(
            unlucky_core::download::config::KEY_RETRY_LIMIT,
            "单源尝试次数",
            &with_note(
                format!(
                    "同一个源最多试几次（含首次；出厂 {DEFAULT_MAX_ATTEMPTS_PER_SOURCE}）。0 与 1 都等于「只试一次、不重试」——0 是旧版提供的档位，不代表「不下载」。换源不受此限"
                ),
                retry_note.as_deref(),
            ),
            &retry_options,
            selected(&retry_options, &retry),
        ))
        .with(SettingRowView::choice(
            "download.region",
            "地区判定",
            "auto = 时区+语言+探测；决定是否优先国内镜像",
            &regions,
            selected(
                &regions,
                if region_value.is_empty() {
                    "auto"
                } else {
                    &region_value
                },
            ),
        ))
        .with(SettingRowView::choice(
            "download.mirror_preference",
            "镜像偏好",
            "auto = 按健康度评分；official = 官方源优先",
            &mirrors,
            selected(
                &mirrors,
                if mirror_value.is_empty() {
                    "auto"
                } else {
                    &mirror_value
                },
            ),
        ))
        .with(SettingRowView::choice(
            "download.proxy_mode",
            "代理模式",
            "none / system（跟随系统）/ manual（下面填地址）",
            &proxies,
            selected(
                &proxies,
                if proxy_value.is_empty() {
                    "none"
                } else {
                    &proxy_value
                },
            ),
        ))
        .with(SettingRowView::text(
            "download.proxy_url",
            "HTTP 代理",
            "manual 模式生效；留空则用系统代理",
            &text(env, "download.proxy_url"),
        ))
        // M193：gh-proxy 三态。**留空与 off 是两件事**，描述里必须都写出来——用户最容易把
        // 「我还没决定」当成「我不要加速」，于是以为自己关掉了、实际还在套前缀。
        //
        // P3-5：末尾那半句是**引擎生效档位**（`tuning.gh_proxy` 的翻译结论），不是文件原文。
        .with(SettingRowView::text(
            unlucky_core::download::config::KEY_GH_PROXY,
            "GitHub 加速前缀（gh-proxy）",
            &format!(
                "留空 = 跟随内置默认（镜像优先/国内判定时自动套 gh-proxy.com）；填 off = 关闭（不生成任何 gh-proxy 候选）；填一个 URL 前缀 = 覆盖它；当前生效：{}",
                gh_proxy_state(&tuning.gh_proxy)
            ),
            &text(env, unlucky_core::download::config::KEY_GH_PROXY),
        ))
        .with(SettingRowView::action(
            super::super::engine::DETECT_REGION_ACTION,
            "重新检测地区",
            "清掉 24h 地区缓存，下次下载重新判定",
            "detect_region",
        ))
}

/// 推荐档位 + 「当前值不在档位里就把它插进去」（否则这一行会把 2 显示成 4 —— 又一个假值）。
fn ladder(ladder: &[&str], current: &str) -> Vec<String> {
    let mut options: Vec<String> = ladder.iter().map(|item| (*item).to_string()).collect();
    if !current.is_empty() && !options.iter().any(|option| option == current) {
        options.push(current.to_string());
        options.sort_by_key(|option| option.parse::<i64>().unwrap_or(i64::MAX));
    }
    options
}

/// 文件里**显式写过**的原值，转成**上屏文本**（`None` = 文件没写这个键）。
///
/// 与 `super::int` 的区别：那个走 `effective()`（文件值 → schema 默认值），本函数只问文件本身。
/// 「原值 X 已按出厂 N 生效」这句话的两个数必须现读（一个来自文件、一个来自 core 的翻译层），
/// **不许**拿常量或请求参数冒充——那是 U1 明令禁止的假证据。
///
/// ## 为什么返回字符串而不是 `i64`（r1 代码复审 p2）
///
/// 手改文件可以把一个整数键写成 `"auto"` / `8.5` / `true`：`document::load` 对坏值只记
/// `LoadProblem`、原样留在 `values` 里，所以 `doc().get(key)` 真的能拿到 `Some(String("auto"))`。
/// 早先这里用 `Value::as_i64`，非整数一律 `None` ⇒ 行上画着生效值 8，却**不告诉用户他写的
/// `auto` 被忽略了**——「设置项说假话」的最后一个洞。改成上屏文本后，越界整数（`9999` / `-4`）、
/// 非整数 JSON（`8.5`）、别的类型（`true`）、字符串（`auto`）**都能进那句提示**；
/// 数字 `to_string()` 出来仍是 `9999`，所以旧断言与旧措辞一字未变。
fn raw_text(env: &HostEnv, key: &str) -> Option<String> {
    use serde_json::Value;
    let store = env.store();
    match store.doc().get(key)? {
        // `null` 等于「没写」：`effective()` 也是这么当的（`filter(|value| !value.is_null())`）。
        Value::Null => None,
        // 字符串照原文上屏（不要 serde 的引号）：用户写的是 `auto`，提示里就该是 `auto`。
        Value::String(text) => Some(text.clone()),
        other => Some(other.to_string()),
    }
}

/// 文件原值 ≠ 生效值时的说明；`None` = 没什么可说的（没写过，或写的就是生效值）。
///
/// `raw` 是**上屏文本**（见 [`raw_text`]），`effective` / `factory` 是 core 翻译层的结论。
fn applied_note(raw: Option<String>, effective: i64, factory: i64) -> Option<String> {
    let raw = raw?;
    // 比的是「上屏文本」：文件写 `8`、生效也是 8 ⇒ 没什么可说的（`8.0` 会照实提示，因为那是一次
    // 类型不一致的坏值）。
    if raw == effective.to_string() {
        return None;
    }
    Some(if effective == factory {
        format!("原值 {raw} 已按出厂 {factory} 生效")
    } else {
        format!("原值 {raw} 已按 {effective} 生效")
    })
}

/// 说明 +（有差异时）那一句「原值 … 生效」。
///
/// **那句结论独占一行（M201 r1 视觉复审 P2-2）**：原先用「；」接在说明末尾，于是在设置页 424 列的
/// 表单宽里，越界那句的最后一个「效」被挤到第三行、单字成行（浅色 1180 / 深色 1180 / 窄窗 880
/// 三张都是同一处孤字，x 396–407）。那句话恰好只在本 mission 的主场景（手改坏值/越界）出现，
/// 让「用户最需要看懂的那一句」排得最难看。改法**不动排版**（换行由渲染器按 `\n` 分行），
/// 只把结论从段落里挪出来——两个数（文件原值 / 生效值）与出厂/非出厂的两种措辞一个字都没丢。
fn with_note(base: String, note: Option<&str>) -> String {
    match note {
        Some(note) => format!("{base}\n{note}"),
        None => base,
    }
}

/// gh-proxy 三态的上屏短句（与 [`GhProxyOverride`] 一一对应）。
fn gh_proxy_state(state: &GhProxyOverride) -> String {
    match state {
        GhProxyOverride::Builtin => "跟随内置默认".to_string(),
        GhProxyOverride::Off => "已关闭（off）".to_string(),
        GhProxyOverride::Prefix(prefix) => format!("覆盖为 {prefix}"),
    }
}

/// 从 `launcher.json` 读出下载调优（默认值与夹取口径在 core，本函数只做「取原始值」）。
#[must_use]
pub(crate) fn tuning_of(env: &HostEnv) -> DownloadTuning {
    use unlucky_core::download::config::{KEY_CONCURRENCY, KEY_GH_PROXY, KEY_RETRY_LIMIT};
    let store = env.store();
    let doc = store.doc();
    let gh = doc
        .effective_str(KEY_GH_PROXY)
        .map(str::to_string)
        .unwrap_or_default();
    DownloadTuning::from_settings(
        doc.effective_i64(KEY_CONCURRENCY),
        doc.effective_i64(KEY_RETRY_LIMIT),
        Some(gh.as_str()),
    )
}

/// 把当前设置灌进 core 的下载调优（按数据根），返回**是否真的变了**（变了就失效该根的引擎）。
///
/// 幂等：值没变时 `shared::set_tuning` 一行不动，所以「每次取引擎前都调」是安全的。
pub(crate) fn apply_tuning(env: &HostEnv) -> bool {
    shared::set_tuning(env.layout.cache(), tuning_of(env))
}

#[cfg(test)]
#[path = "download_tests.rs"]
mod tests;
