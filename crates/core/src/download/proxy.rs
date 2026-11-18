//! `download.proxy_mode` / `download.proxy_url` → reqwest 客户端的代理三态（M295）。
//!
//! # 现状（M295 之前）
//!
//! `grep -rn "proxy_mode\|proxy_url\|ProxyMode" crates/core/src --include=*.rs | grep -v setting/keys`
//! **零命中**：两个键在引擎侧零消费者。`http::build_client` 当时只设 UA / 连接超时 / 重定向 /
//! 默认头，**从不 `.proxy()`**，于是设置页画着的「直连 / 系统代理 / 手动代理」三档 chip 与那个
//! 地址文本框，对任何一次下载都不生效。`system` 那一档「碰巧能用」只是因为 reqwest 自己会读进程
//! 环境变量（`HTTP_PROXY` 等），不是这段代码做的——这也是本模块必须把它写成**显式一档**的理由：
//! 三档都要是这段代码的判决，而不是「碰巧」。
//!
//! # 三档语义（全仓唯一一处翻译）
//!
//! | `download.proxy_mode` | 翻译成 | 落到 client 上 | 含义 |
//! |---|---|---|---|
//! | `none`（schema 出厂值，键不存在也走这档） | [`ProxySetting::Direct`] | `.no_proxy()` | 什么都不走：连进程环境里的 `HTTP_PROXY` 也不读 |
//! | `system` | [`ProxySetting::System`] | **不调任何代理 API** | 跟随进程环境变量（`HTTP_PROXY` / `HTTPS_PROXY` / `ALL_PROXY` / `NO_PROXY`）= reqwest 的默认行为，也是 M295 之前所有下载的实际行为 |
//! | `manual` | [`ProxySetting::Manual`] | `.proxy(Proxy::all(url))` | 整条流量走 `proxy_url`（http 与 https 目标都经它；`Proxy::all` 会顺手关掉「自动读系统代理」） |
//!
//! 未知值 / 空串 ⇒ `none`（与 schema 出厂值一致：静默回落、不报错、不中断下载，口径与
//! `preference` 那两个枚举键相同）。
//!
//! **默认值为什么要分两处**（`EngineConfig::default()` 是 `System`、`DownloadTuning::default()`
//! 是 `Direct`）：`EngineConfig::default()` 是「这个子系统**没读**设置」的档（`auth` 的登录、
//! 发布版 GUI 的 JDK 下载页、地区探测都直接用它），保持 M295 之前的行为（跟随环境变量）才不算
//! 静默改网络路径；而读了设置的装配点（`shared::build` / `engine_with`）一律由
//! `DownloadTuning` 显式盖上去，那一路的出厂值是 `none` —— 与设置页画的那颗 chip 一致，
//! 「界面说 none、实际偷偷走系统代理」正是本任务要治的病。
//!
//! ## `proxy_url` 在非 manual 模式下：**忽略**
//!
//! 一句话决定：`proxy_url` 只在 `manual` 档被读；`none` / `system` 下它**既不校验、也不进入**
//! [`ProxySetting`]，值原样留在 `launcher.json` 里（模式切回 `manual` 就立刻生效）。模式是唯一
//! 开关，所以不存在「填了地址、模式没选对、半生效」这种中间态。
//!
//! `manual` + **空地址** ⇒ [`ProxySetting::System`]：设置页那行文案承诺过「manual 模式生效；留空则
//! 用系统代理」，这里按文案兑现。不选 `Direct` —— 那会让「留空」变成第三种没人写过的语义。
//!
//! # 口令：**所有**用户可见 / 会落盘的字符串都先脱敏（r1 复审 p2-1）
//!
//! `proxy_url` 允许带 userinfo（`http://user:password@host:port`），而这个键在本仓就是凭据
//! （schema 是 `AiPolicy::sensitive()`、`ull config get` 打 `<redacted>`）。M295 第一版把
//! `manual` 的原串整条写进 `summary()`（→ CLI stderr）与解析失败文案，等于把口令还原成明文。
//! 现在**唯一**的出口是 [`redact_proxy_url`]：`user:password@` → `user:***@`（用户名保留，口令打掉；
//! 没有口令的地址**逐字不变**）。[`ProxySetting::describe`] / 错误文案 / `Debug` 都过它，
//! 宿主直接拼 `problem()` 的返回值即可，**不许**自己再拼一遍地址。
//!
//! # 非法值不得穿透（两类都走同一条路：有界错误 + 回落直连 + 告知）
//!
//! 1. **用不了**：由 `validate` **自己**判四类——空地址 / scheme 不支持（`socks5://` 等）/
//!    缺少主机名 / 端口不合法（空、`0`、越界）。每类各一句中文原因，**不读** reqwest 的
//!    `builder error`、也不匹配 `url` 的英文文案（那是「摊原文」，等于没解释）；
//! 2. **reqwest 层残余的拒绝**（`validate` 放行但它不认，例如 userinfo 里有非法字符）：中文结论在
//!    前，reqwest 原文只作为括号里的证据。
//!
//! 两种情况都是 `InvalidRequest` + 带（脱敏后）原串的可读文案，**不 panic**。
//!
//! 装配点（[`super::engine::DownloadEngine::new`]）的处置是「回落直连 + 把原因留在引擎上」
//! （`DownloadEngine::proxy_problem`）：一个设置字符串不该让用户连文件都下不了，但也不许它悄悄变成
//! 「用了某个不知道是什么的代理」。**回落必须被看见**：CLI 打一行 stderr，GUI 的设置页把
//! [`ProxySetting::problem`] 的结论画在「HTTP 代理」那一行（两个宿主都调同一个判定，不各写一份）。
//!
//! 一条实测（别把它当 bug 修掉）：reqwest 的 `Proxy::all` 对**没有 scheme** 的串会自作主张补
//! `http://` —— `127.0.0.1:7890` 是合法写法。所以「宽口径」只补到「少写 scheme」为止，判据自己算。
//!
//! # 不落盘
//!
//! 与 `preference` 同一条纪律：翻译只发生在装配那一刻，`launcher.json` 里存的永远是用户写的原文。
//!
//! # 没验的部分
//!
//! 真机上的「系统代理」在 Windows/macOS 上的注册表/偏好设置路径**没有**被读（本仓 reqwest 没开
//! `client-proxy-system` feature ⇒ `system` 只认环境变量），PAC 脚本与需要认证（`Proxy-Authorization`）
//! 的代理同样没测。

use std::fmt;

use reqwest::ClientBuilder;

use super::error::{DownloadError, DownloadErrorKind};

/// `download.proxy_mode` + `download.proxy_url` 的翻译结论（**全仓唯一**一份）。
///
/// `Debug` 手写（不 derive）：`Manual` 里的地址会过 [`redact_proxy_url`] —— 引擎 / 配置都会被
/// `{:?}` 打进日志与诊断，那条路径不该是口令的第二出口。
#[derive(Clone, PartialEq, Eq, Default)]
pub enum ProxySetting {
    /// `none`（schema 出厂值）：**真直连**，连环境变量里的代理也不读。
    #[default]
    Direct,
    /// `system`：跟随进程环境变量（reqwest 的默认行为，本档不调任何代理 API）。
    System,
    /// `manual`：整条流量走这个地址。
    Manual {
        /// `download.proxy_url` 的原串（已去前后空白；**未**校验，校验在 [`Self::apply`]）。
        /// **原串留着**：真发请求要用它；一切对外字符串都先过 [`redact_proxy_url`]。
        url: String,
    },
}

impl fmt::Debug for ProxySetting {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Direct => f.write_str("Direct"),
            Self::System => f.write_str("System"),
            Self::Manual { url } => write!(f, "Manual {{ url: {:?} }}", redact_proxy_url(url)),
        }
    }
}

impl ProxySetting {
    /// 设置原文 → 代理三态（**唯一**翻译入口：两个键一起读，规则见模块头）。
    ///
    /// `mode` / `url` 的 `None` 是「键不存在 / 类型不对」（宿主侧 `effective_str` 的两种失败），
    /// 与空串一样按「没填」处理：`mode` 没填 ⇒ `none`，`url` 没填 + `manual` ⇒ `system`。
    #[must_use]
    pub fn from_raw(mode: Option<&str>, url: Option<&str>) -> Self {
        let url = url.map(str::trim).unwrap_or_default();
        match mode
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "system" => Self::System,
            "manual" if url.is_empty() => Self::System,
            "manual" => Self::Manual {
                url: url.to_string(),
            },
            // `none` / 空串 / 未知值（手改文件写出来的 `mars`）都落这一档：与 schema 出厂值一致。
            _ => Self::Direct,
        }
    }

    /// 稳定字符串 id（与 schema 的取值逐字相同；日志/诊断用）。
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Direct => "none",
            Self::System => "system",
            Self::Manual { .. } => "manual",
        }
    }

    /// 一行「引擎实际拿到了什么」（`DownloadTuning::summary` 用：三档语义不同，只印 `as_str`
    /// 会把「跟随环境变量」与「某个具体地址」印成同一句话）。
    ///
    /// 地址过 [`redact_proxy_url`]：这行会进 CLI 的 stderr（`download::announce`）、进而进 CI 日志。
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Direct => "none（直连）".to_string(),
            Self::System => "system（跟随环境变量）".to_string(),
            Self::Manual { url } => format!("manual（{}）", redact_proxy_url(url)),
        }
    }

    /// 这一档在当前构建里**能不能真的用上**（`None` = 没问题；`Some(原因)` = 用不了，会回落直连）。
    ///
    /// 与 [`Self::apply`] 是**同一个判定**（都走 `manual_proxy`）：宿主拿它给用户一句可见的提示
    /// （GUI 设置页那一行 / CLI 那行 stderr），不必为了一个提示真的去造 client。文案已脱敏。
    #[must_use]
    pub fn problem(&self) -> Option<String> {
        match self {
            Self::Manual { url } => manual_proxy(url).err().map(|err| err.message),
            Self::Direct | Self::System => None,
        }
    }

    /// 把这一档盖到 client builder 上（**唯一**应用点：`http::build_client`）。
    ///
    /// # Errors
    ///
    /// 只在 `manual` 档、且地址用不了时：`kind = InvalidRequest`，`message` 带上（脱敏后的）原串与
    /// **能读**的原因。**不 panic、不静默改成别的档**——回落由装配点决定，见模块头。
    pub fn apply(&self, builder: ClientBuilder) -> Result<ClientBuilder, DownloadError> {
        match self {
            Self::Direct => Ok(builder.no_proxy()),
            // 故意的空操作：reqwest 的默认行为就是「有显式代理用显式的，没有就读环境变量」，
            // 这一档要的正是后者。写成 `.proxy(Proxy::system())` 会更像「显式」，但 reqwest 0.12
            // 没有这个公开构造函数（`system` 只在 `auto_sys_proxy` 为真时内部生效）。
            Self::System => Ok(builder),
            Self::Manual { url } => Ok(builder.proxy(manual_proxy(url)?)),
        }
    }
}

/// **口令脱敏的唯一实现**：`scheme://user:password@host` → `scheme://user:***@host`。
///
/// 规则（三条都要背下来，改它之前先看 [`Self`] 的用例）：
/// * 只动 **userinfo**（`//` 之后、第一个 `/` `?` `#` 之前，最后一个 `@` 的左边）——路径里、
///   查询里出现 `@` / `:` 一律不碰；
/// * userinfo 里**没有 `:`**（只有用户名、没有口令）⇒ 原样返回：用户名不是凭据，打掉它只会让用户
///   看不出自己在用哪个账号；
/// * 没有 userinfo / 整串不是一个 URL ⇒ **逐字原样**返回（脱敏函数不许顺手把正常地址改坏，
///   比如补上尾斜杠、改大小写、百分号归一）。
#[must_use]
pub fn redact_proxy_url(raw: &str) -> String {
    // 1. 跳过 scheme（没有 `://` 就整串当 authority，`127.0.0.1:7890` 这种写法也照样处理）。
    let authority_start = raw.find("://").map_or(0, |at| at + 3);
    let tail = &raw[authority_start..];
    let authority_end = tail.find(['/', '?', '#']).unwrap_or(tail.len());
    let authority = &tail[..authority_end];
    // 2. 最后一个 `@` 之前是 userinfo（口令里可以有 `@`，host 里不能）。
    let Some((userinfo, host)) = authority.rsplit_once('@') else {
        return raw.to_string();
    };
    // 3. 只有用户名、没有口令 ⇒ 原样（用户名不是凭据）。
    let Some((user, _password)) = userinfo.split_once(':') else {
        return raw.to_string();
    };
    format!(
        "{}{user}:***@{host}{}",
        &raw[..authority_start],
        &tail[authority_end..]
    )
}

/// `manual` 档的解析与判定（`apply` 与 `problem` 的**同一条**路）。
///
/// 顺序：**先自己判**（[`validate`]：空 / scheme / host / 端口四类各给一句中文原因），再用
/// `reqwest::Proxy::all` 真的造代理。为什么不靠 `Proxy::all` 的报错：它的 Display 只有一句
/// `builder error`（原因挂在 `source()` 上，甚至可能是空的），把原文摊给用户等于没解释；
/// 为什么不靠 `url::ParseError` 的 Display：那是英文（`invalid port number`），而且 reqwest 的
/// 宽口径（给少写 scheme 的串补 `http://`）会让「解析成功/失败」本身就不可靠——端口越界正是
/// 撞在这上面（r2 复审要求：四类各给一句，不许摊原文）。
fn manual_proxy(url: &str) -> Result<reqwest::Proxy, DownloadError> {
    let shown = redact_proxy_url(url);
    validate(url).map_err(|why| {
        DownloadError::new(
            DownloadErrorKind::InvalidRequest,
            format!("代理地址用不了（{shown}）：{why}"),
        )
    })?;
    // 残余失败（validate 放行、但 reqwest 不认，例如 userinfo 里有非法字符）：中文结论在前，
    // reqwest 的原文只作为括号里的**证据**，不当结论。
    reqwest::Proxy::all(url).map_err(|err| {
        DownloadError::new(
            DownloadErrorKind::InvalidRequest,
            format!(
                "代理地址用不了（{shown}）：地址写法不被接受（{}）",
                reason(&err)
            ),
        )
    })
}

/// 代理地址的**四类**判定：空 / scheme 不支持 / 缺少主机名 / 端口不合法。
///
/// 每条都**自己算**（不解析 reqwest 的错误、也不匹配英文文案），所以判定与 reqwest/url 的版本、
/// 宽口径、错误措辞无关。这里只判「能不能拿去连」，不做归一化——通过之后原串仍会被
/// `reqwest::Proxy::all` 拿去用（`127.0.0.1:7890` 这种少写 scheme 的写法是合法的）。
fn validate(raw: &str) -> Result<(), String> {
    let text = raw.trim();
    if text.is_empty() {
        return Err("地址是空的（manual 档要填 `http://host:port`）".to_string());
    }
    let (scheme, rest) = match text.split_once("://") {
        Some((scheme, rest)) => (Some(scheme.to_ascii_lowercase()), rest),
        None => (None, text),
    };
    if let Some(scheme) = &scheme {
        if !matches!(scheme.as_str(), "http" | "https") {
            return Err(format!("本构建不支持 {scheme} 代理（只支持 http / https）"));
        }
    }
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    // 最后一个 `@` 之后才是 host[:port]（userinfo 里的 `@` `:` 不参与）。
    let host_port = authority.rsplit_once('@').map_or(authority, |(_, hp)| hp);
    let (host, port) = match host_port.strip_prefix('[') {
        // IPv6 字面量：`[::1]:8080`
        Some(after_open) => match after_open.split_once(']') {
            Some((host, after)) => (host, after.strip_prefix(':')),
            None => return Err("IPv6 地址的方括号没有闭合".to_string()),
        },
        None => match host_port.rsplit_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (host_port, None),
        },
    };
    if host.is_empty() {
        return Err("缺少主机名（`host` 是空的）".to_string());
    }
    if let Some(port) = port {
        if port.is_empty() {
            return Err("端口是空的（`host:` 后面没有数字）".to_string());
        }
        match port.parse::<u16>() {
            Ok(0) => return Err("端口 0 不是有效端口（要 1–65535）".to_string()),
            Ok(_) => {}
            Err(_) => return Err(format!("端口 {port} 不是 1–65535 之间的数字")),
        }
    }
    Ok(())
}

/// 把错误的 `source` 链拼成**一句能读的原因**（r1 复审 p3-3）。
///
/// 为什么不能只用 `Display`：`reqwest::Error` 的 Display 是 `builder error`（kind 的名字），真正的
/// 原因（`invalid port number`）挂在 `source()` 上——只印前者等于「为什么」那半句没写。重复的片段
/// 只留一次（reqwest 会把同一个 kind 套两层）。
fn reason(err: &(dyn std::error::Error + 'static)) -> String {
    let mut parts = vec![err.to_string()];
    let mut source = err.source();
    while let Some(inner) = source {
        let text = inner.to_string();
        if !parts.contains(&text) {
            parts.push(text);
        }
        source = inner.source();
    }
    parts.join("：")
}

#[cfg(test)]
#[path = "proxy/tests.rs"]
mod tests;
