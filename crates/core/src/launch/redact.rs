//! 日志与事件流的脱敏（写入前执行，落盘即脱敏）。
//!
//! 规则见 `DevDocs/SubAgentsTalk/launch-接口约定.md` §4：
//! token 整串替换、用户主目录替换、长度 ≥64 的不透明串（base64/hex token）替换。
//! 依据 `analyse/02` §6.4 / §8.4-5（HMCL 只过滤 access token，我们要求导出前全量脱敏）。
//!
//! 第四条规则（M40 从 `auth::body` 上提）：**带点 token** 整串替换。JWT 形状是
//! `header.payload.signature`（XSTS/XBL 票据正是这个），点号把连续段切开，单看
//! 「连续不透明串 ≥64」这条会整串原样放过。规则只在这里实现一份，所有调用方
//! （launch 会话 / 诊断 / auth 错误摘要 …）共用，避免各自补一套后漂移。
//!
//! 形状判据本身在 [`shape`]，本模块只做「扫描 + 搬运」：把文本切成连续段，
//! 逐段问一次判据，命中就换成占位符。
//!
//! # 判据只砍「真凭据不可能有的形状」（M240）
//!
//! 两条形状规则都是纯启发式，M239 实测它们会误杀三类正常文本（品牌串、CDN 主机名、长路径），
//! 而这三类被抹掉的面正是用户要复制/要读的面（`--dry-run` 的 argv、诊断规则的输入）。
//! 修法不是抬阈值，而是给每条规则加**必要条件**（详见 [`shape`] 的表）：
//!
//! - `=` 不算 token 字符（赋值边界）：`-D<key>=<品牌>` 不再被当成三段式 token；
//! - 带点 token 必须含大写字母、且首段不是主机名（JWT 头的 base64url 第三个字符恒为
//!   `I/J/K/L`，且 base64url 不含 `/`）；
//! - ≥64 的不透明串若是「路径形状」（被 `/` 切开且每段都是词形名字）就整条保留，
//!   但**段本身**仍够长的照抹（`/…/objects/<64 位 hex>` 里只有那段消失）。
//!
//! **口径（别读成"抹除能力没有下降"）**：登记过明文值的那条替换路径没动，JWT / sha256 /
//! base64 的正例全部仍被抹；但有三类形状是「旧行为抹、新行为留」——`=` 边界、填充不计段长、
//! 全小写点分串。三条都在 [`shape`] 的「旧行为抹、新行为留」一节里逐条写明理由，
//! 前两条是本次要修的误杀，第三条是代价。
//!
//! # 句读不是 token 本体（M243 BEARERDOT）
//!
//! `Bearer eyJ….eyJ….c2ln….`（行尾一个英文句点）在 M240 之后仍整串原样落盘：句点自己属于
//! dotted 字符集，被算进 run 后 `split('.')` 多切出一个空段，「每段 ≥8」当场不成立。修法同样
//! 不是抬阈值——判定前把**末尾**句读切出去（[`shape::split_trailing_punct`]），搬运方
//! （[`push_dotted`]）再把句读原位写回，所以占位符只吃 token 本体，句子标点一个不丢。
//! 只削末尾：中间的句点仍是 token 的一部分（JWT 的三段就是靠它切的）；`(`、`)`、`,`、`;`
//! 本来就在字符集外，一并列进规则是为了「句读不参与判定」这件事只有一个出处。

use std::borrow::Cow;
use std::path::Path;

use serde_json::Value;

mod shape;

use self::shape::{
    is_dns_host, is_dotted_token, is_dotted_token_char, is_opaque_char, is_path_run,
    split_trailing_punct, MAX_OPAQUE_RUN,
};

/// access token / session token 的替换标记。
pub const TOKEN_PLACEHOLDER: &str = "<redacted:token>";
/// 用户主目录的替换标记。
pub const HOME_PLACEHOLDER: &str = "<home>";
/// 形似 token 的长不透明串的替换标记。
pub const OPAQUE_PLACEHOLDER: &str = "<redacted:opaque>";

/// 脱敏器：持有需要抹掉的敏感值（token、主目录）。
#[derive(Debug, Clone, Default)]
pub struct Redactor {
    secrets: Vec<String>,
    homes: Vec<String>,
}

impl Redactor {
    /// 空脱敏器（不做任何替换）。
    pub fn new() -> Self {
        Self::default()
    }

    /// 登记一个敏感值（access token、userhash、xuid 等）。
    ///
    /// 太短的值（< 6 字节）会被忽略：替换一个 3 字符串会把正常日志搅乱。
    pub fn with_secret(mut self, secret: impl Into<String>) -> Self {
        let secret = secret.into();
        if secret.trim().len() >= 6 && !self.secrets.contains(&secret) {
            self.secrets.push(secret);
        }
        self
    }

    /// 登记一个需要抹掉的主目录（替换成 `<home>`）。
    pub fn with_home(mut self, home: impl AsRef<Path>) -> Self {
        let home = home.as_ref().to_string_lossy().into_owned();
        if home.len() > 3 && !self.homes.contains(&home) {
            self.homes.push(home);
        }
        self
    }

    /// 登记一个主目录字符串。
    pub fn with_home_str(mut self, home: impl Into<String>) -> Self {
        let home = home.into();
        if home.len() > 3 && !self.homes.contains(&home) {
            self.homes.push(home);
        }
        self
    }

    /// 是否登记了任何规则。
    pub fn is_empty(&self) -> bool {
        self.secrets.is_empty() && self.homes.is_empty()
    }

    /// 对文本脱敏；无需修改时返回借用，避免无谓拷贝。
    pub fn scrub<'a>(&self, text: &'a str) -> Cow<'a, str> {
        let mut current: Cow<'a, str> = Cow::Borrowed(text);
        for secret in &self.secrets {
            if current.contains(secret.as_str()) {
                current = Cow::Owned(current.replace(secret.as_str(), TOKEN_PLACEHOLDER));
            }
        }
        for home in &self.homes {
            if current.contains(home.as_str()) {
                current = Cow::Owned(current.replace(home.as_str(), HOME_PLACEHOLDER));
            }
        }
        if has_dotted_token(&current) {
            current = Cow::Owned(mask_dotted_tokens(&current));
        }
        if has_opaque_run(&current) {
            current = Cow::Owned(mask_opaque(&current));
        }
        current
    }

    /// 对整条命令行脱敏（`cmd_built.argv` 必然含 accessToken，必须走这里）。
    pub fn scrub_all(&self, values: &[String]) -> Vec<String> {
        values
            .iter()
            .map(|value| self.scrub(value).into_owned())
            .collect()
    }

    /// 递归脱敏一个 JSON 值。
    ///
    /// **只改字符串**：数值/布尔/null 与字段名原样保留——满足「launch 只做脱敏、不改写字段」
    /// 的契约（`DevDocs/SubAgentsTalk/launch-接口约定.md` §9.3）。诊断结论里的证据行常带
    /// 崩溃报告/`hs_err` 的绝对路径，正是要过 `<home>` 规则的地方。
    pub fn scrub_json(&self, value: &Value) -> Value {
        match value {
            Value::String(text) => Value::String(self.scrub(text).into_owned()),
            Value::Array(items) => {
                Value::Array(items.iter().map(|item| self.scrub_json(item)).collect())
            }
            Value::Object(map) => Value::Object(
                map.iter()
                    .map(|(key, value)| (key.clone(), self.scrub_json(value)))
                    .collect(),
            ),
            other => other.clone(),
        }
    }
}

/// 文本里是否含长度 ≥64 的“不透明串”（base64/hex token 的特征）。
pub fn has_opaque_run(text: &str) -> bool {
    let mut start: Option<usize> = None;
    for (idx, ch) in text.char_indices() {
        if is_opaque_char(ch) {
            start.get_or_insert(idx);
            continue;
        }
        if let Some(from) = start.take() {
            if run_needs_masking(&text[from..idx]) {
                return true;
            }
        }
    }
    start.is_some_and(|from| run_needs_masking(&text[from..]))
}

/// 这一段不透明连续串要不要动手：够长，且（不是路径 或 路径里某一段自己就够长）。
fn run_needs_masking(run: &str) -> bool {
    run.len() >= MAX_OPAQUE_RUN
        && (!is_path_run(run) || run.split('/').any(|chunk| chunk.len() >= MAX_OPAQUE_RUN))
}

/// 把长度 ≥64 的不透明串替换成 [`OPAQUE_PLACEHOLDER`]（路径形状的串见 [`push_path_masked`]）。
pub fn mask_opaque(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut run_start: Option<usize> = None;
    for (idx, ch) in text.char_indices() {
        if is_opaque_char(ch) {
            if run_start.is_none() {
                run_start = Some(idx);
            }
        } else {
            if let Some(start) = run_start.take() {
                push_masked(&mut out, &text[start..idx]);
            }
            out.push(ch);
        }
    }
    if let Some(start) = run_start {
        push_masked(&mut out, &text[start..]);
    }
    out
}

fn push_masked(out: &mut String, run: &str) {
    if !run_needs_masking(run) {
        out.push_str(run);
    } else if is_path_run(run) {
        push_path_masked(out, run);
    } else {
        out.push_str(OPAQUE_PLACEHOLDER);
    }
}

/// 路径整条保留，但**段本身**够长的照抹。
///
/// 路径判据放行的是"目录/包名段"，不许顺手放行一整段凭据：`/srv/mc/assets/objects/<64 位 hex>`
/// 里的目录留下，那个 64 字符的段照样变成占位符（它与无点号密钥在形状上不可区分）。
fn push_path_masked(out: &mut String, run: &str) {
    for (idx, chunk) in run.split('/').enumerate() {
        if idx > 0 {
            out.push('/');
        }
        if chunk.len() >= MAX_OPAQUE_RUN {
            out.push_str(OPAQUE_PLACEHOLDER);
        } else {
            out.push_str(chunk);
        }
    }
}

/// 文本里是否含「带点 token」。
pub fn has_dotted_token(text: &str) -> bool {
    let mut start: Option<usize> = None;
    for (idx, ch) in text.char_indices() {
        if is_dotted_token_char(ch) {
            start.get_or_insert(idx);
            continue;
        }
        if let Some(from) = start.take() {
            if run_has_dotted_token(&text[from..idx]) {
                return true;
            }
        }
    }
    start.is_some_and(|from| run_has_dotted_token(&text[from..]))
}

/// 一段里有没有该抹的部分：URL 形状只判**路径那一半**，主机名放行。
fn run_has_dotted_token(run: &str) -> bool {
    match split_authority(run) {
        Some((_, rest)) => run_has_dotted_token(rest),
        None => is_dotted_token(split_trailing_punct(run).0),
    }
}

/// 把「带点 token」整串替换成 [`TOKEN_PLACEHOLDER`]（段外文本原样保留）。
pub fn mask_dotted_tokens(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut start: Option<usize> = None;
    for (idx, ch) in text.char_indices() {
        if is_dotted_token_char(ch) {
            start.get_or_insert(idx);
            continue;
        }
        if let Some(from) = start.take() {
            push_dotted(&mut out, &text[from..idx]);
        }
        out.push(ch);
    }
    if let Some(from) = start {
        push_dotted(&mut out, &text[from..]);
    }
    out
}

fn push_dotted(out: &mut String, run: &str) {
    // URL 形状先切：主机名原样留下，路径那半继续判——JWT 摆在 URL 路径里也照抹。
    if let Some((prefix, rest)) = split_authority(run) {
        out.push_str(prefix);
        out.push('/');
        push_dotted(out, rest);
        return;
    }
    // 句读不参与判定，但要原位保留：`Bearer <JWT>.` 的句点不许被占位符吃掉。
    let (body, punct) = split_trailing_punct(run);
    if is_dotted_token(body) {
        out.push_str(TOKEN_PLACEHOLDER);
    } else {
        out.push_str(body);
    }
    out.push_str(punct);
}

/// 把 URL 形状的 run 切成（主机名前缀, 路径那半）。路径那半不含分隔用的那个 `/`。
///
/// 前缀里含 scheme 之后的 `//`（`:` 会切断 run，所以 `https:` 不在 run 里）。
/// 判据只看主机名部分是不是 DNS 形状，路径原样交回去继续判。
fn split_authority(run: &str) -> Option<(&str, &str)> {
    let slashes = run.len() - run.trim_start_matches('/').len();
    let (authority, _) = run[slashes..].split_once('/')?;
    if !is_dns_host(authority) {
        return None;
    }
    let cut = slashes + authority.len();
    Some((&run[..cut], &run[cut + 1..]))
}

#[cfg(test)]
#[path = "redact/tests.rs"]
mod tests;
