//! 中文字体下载地址的覆盖键（M168：D28 后半「下载开源中文字体」）。
//!
//! 从 `keys.rs` 拆出（那边贴着 400 行硬线）：主表只留一行 `.with(font::cdn_spec())`。
//!
//! ## 为什么需要这个键
//!
//! 内置 pin（`unlucky_i18n::font::NOTO_SANS_SC`）把 URL 钉到了 jsDelivr 的一个 git commit 上
//! ——那是**一个**出口。用户所在网络如果连不上它（或想走自建镜像 / 代理），要有办法换，否则
//! 「自动下载中文字体」在某些网络上等于不存在。
//!
//! ## 为什么只换 URL，不换哈希
//!
//! 字体是**注册进渲染器的可执行内容**，不是一张图。这个键只允许改「从哪拿」，`sha256` 与字节数
//! 仍取内置那一份：镜像给了别的内容 ⇒ 校验当场拒 ⇒ 降级回系统字体，并在设置页那一行说「镜像
//! 内容校验不通过」。**宁可不下载，也不把没校验过的东西注册成字体。**
//!
//! 想让用户能换「整套字体」（换族、换子集）就得连哈希一起进配置——那是另一件事：现在的 pin 是
//! 代码里的常量，改它要发版。理由是此刻只有一套官方子集，多条可写路径只会多一个「校验值和字体
//! 对不上」的支持面。

use crate::setting::spec::{AiPolicy, SettingSpec};

/// `ui.font_cdn` 的登记项（走 [`SettingSpec::text`]：空串 = 用内置镜像）。
#[must_use]
pub fn cdn_spec() -> SettingSpec {
    SettingSpec::text(
        "ui.font_cdn",
        "",
        512,
        AiPolicy::audited(),
        "中文字体下载地址（空 = 内置国内可达镜像；填整条 https URL，内容必须与内置 sha256 一致）",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 默认值是空串（= 用内置镜像），且不是 `Path` 类型——空串对 `Path` 会被 schema 拒。
    ///
    /// 抓得住：有人把键改成 `SettingSpec::path(...)`（空串过不了校验 ⇒ 出厂配置自己就非法）。
    #[test]
    fn the_cdn_key_defaults_to_the_builtin_mirror() {
        let spec = cdn_spec();
        assert_eq!(spec.key, "ui.font_cdn");
        assert_eq!(spec.default, json!(""));
        assert!(
            spec.kind.check(&json!("")).is_ok(),
            "空串（= 内置镜像）必须过校验"
        );
        assert!(
            spec.kind
                .check(&json!("https://mirror.example/NotoSansSC.ttf"))
                .is_ok(),
            "一条 https URL 必须过校验"
        );
    }
}
