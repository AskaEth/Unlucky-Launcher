//! 代理与 gh-proxy 三键（M197 从 `keys.rs` 拆出：那边正好贴着 400 行硬线）。
//!
//! 三个键是一件事的两层：前两个是 **HTTP 代理**（整条请求走代理），第三个是 **gh-proxy 前缀**
//! （只把 GitHub 上的 URL 改写成 `<prefix><url>`，其余流量直连）。放在一起是因为用户问的往往是
//! 「我下不动 GitHub 怎么办」，答案要么在这三个里面、要么在镜像表里。
//!
//! | 键 | 语义 | 谁消费 |
//! |---|---|---|
//! | `download.proxy_mode` | `none` / `system` / `manual` | 引擎的 HTTP 客户端（`http::build_client`） |
//! | `download.proxy_url` | manual 模式的代理地址 | 同上 |
//! | `download.gh_proxy` | 空 = 跟随内置默认；`off` = 关；其它 = 覆盖前缀 | `app-iced` 的 JDK 下载作业（M197），其它下载走同一份 `region.json` |
//!
//! `download.gh_proxy` 的语义细节在 `core::download::region`：**空串不等于关**——空串是「没有
//! 覆盖」，仍按地区判定与内置镜像表走（国内判成镜像优先时用 `gh-proxy.com`）。要关必须写 `off`，
//! 因为「我不想用加速」和「我还没决定」是两件事。

use crate::setting::spec::{AiPolicy, SettingSpec};

use super::PROXY_MODES;

/// 三个键的完整登记项（主表一行 `.with_all(proxy::specs())`）。
#[must_use]
pub fn specs() -> Vec<SettingSpec> {
    vec![
        SettingSpec::enumeration(
            "download.proxy_mode",
            "none",
            PROXY_MODES,
            AiPolicy::sensitive(),
            "代理模式：none / system / manual",
        ),
        SettingSpec::text(
            "download.proxy_url",
            "",
            512,
            AiPolicy::sensitive(),
            "manual 代理地址（如 http://127.0.0.1:7890）",
        ),
        // M197：gh-proxy 前缀的用户覆盖（任务书那句「（默认值可改）」的落点）。
        SettingSpec::text(
            "download.gh_proxy",
            "",
            256,
            AiPolicy::sensitive(),
            "GitHub 下载的加速前缀（gh-proxy）。留空 = 跟随内置默认（镜像表里的 gh-proxy.com，仅镜像优先/国内判定时启用）；填 off 关闭；填一个 URL 前缀则覆盖它",
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 三个键都在、且**空串 ≠ 关**这条语义写在描述里（M197 复审要求它与行为一致）。
    ///
    /// 变异证明：把 `gh_proxy` 的描述改成「留空 = 关闭」⇒ 本用例红（实现里空串是「跟随内置默认」，
    /// `downloads::engine::gh_proxy_from("")` 返回 `Builtin` 而不是 `Off`）。
    #[test]
    fn the_gh_proxy_description_says_empty_is_not_off() {
        let specs = specs();
        let ids: Vec<&str> = specs.iter().map(|spec| spec.key).collect();
        assert_eq!(
            ids,
            vec![
                "download.proxy_mode",
                "download.proxy_url",
                "download.gh_proxy"
            ]
        );
        let gh = specs
            .iter()
            .find(|spec| spec.key == "download.gh_proxy")
            .expect("gh_proxy 已登记");
        assert!(gh.doc.contains("留空 = 跟随内置默认"), "{}", gh.doc);
        assert!(gh.doc.contains("填 off 关闭"), "{}", gh.doc);
    }

    /// **`KEY_GH_PROXY` 与 schema 登记的绑定**（M205）：`download/config.rs` 那个常量是宿主读数用的
    /// 唯一键名，本文件 `specs()` 里是 schema 的登记——两侧各写一个字符串，此前没有任何断言把它们
    /// 绑在一起，常量被改名/写错时全绿，行为只在宿主侧静默退化成 `Builtin`（用户填的 `off` 被忽略）。
    ///
    /// 抓得住：常量值写错（如 `download.ghproxy`）⇒ 找不到登记项，第一句红；登记的 `max_len` 从 256
    /// 改掉、或默认值从空串改成别的 ⇒ 各自红（类型/默认值是宿主的读侧契约）。
    /// 抓不住：两侧**同时**改名（那是一次公开改名，本用例判断不了它对不对，只能保证两侧一致）。
    #[test]
    fn the_gh_proxy_key_is_bound_to_the_schema_registration() {
        use crate::download::config::KEY_GH_PROXY;
        use crate::setting::spec::ValueKind;

        let Some(gh) = specs().into_iter().find(|spec| spec.key == KEY_GH_PROXY) else {
            panic!("下载键常量 {KEY_GH_PROXY:?} 在 schema 里没有登记项（常量值与 specs() 的键名不一致）");
        };
        assert_eq!(gh.kind, ValueKind::Text { max_len: 256 }, "{}", gh.key);
        assert_eq!(gh.default, serde_json::json!(""), "{}", gh.key);
    }
}
