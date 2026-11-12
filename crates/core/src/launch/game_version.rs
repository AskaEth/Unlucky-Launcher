//! 游戏版本号解析与比较（供命令行组装的分支门限使用）。
//!
//! 依据 `analyse/02` §5.2（版本号驱动的分支：legacy/modern 参数模板、log4j 注入、
//! `--quickPlay*` 门限、`--graphicsBackend` 门限）。
//!
//! 说明：这是 launch 侧的最小比较器。若 M14（game）后续提供权威 `GameVersion` 比较器，
//! 只需替换本文件内部实现，`is_at_least` / `is_before` 的调用点不用动
//! （见 `DevDocs/SubAgentsTalk/launch-接口约定.md` §7-3）。

use std::cmp::Ordering;
use std::fmt;

/// 版本的发布阶段。派生 `Ord` 的变体顺序即排序顺序：Snapshot < PreRelease < ReleaseCandidate < Release。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    /// 快照（`26.2-snapshot-2`），数字为快照序号。
    Snapshot(u32),
    /// 预发布（`1.20-pre1`）。
    PreRelease(u32),
    /// 候选发布（`1.20-rc1`）。
    ReleaseCandidate(u32),
    /// 正式版。
    Release,
}

/// 游戏版本号。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameVersion {
    raw: String,
    parts: Vec<u32>,
    stage: Stage,
}

impl GameVersion {
    /// 解析版本串；无法识别（如 `20w14a`）时返回 [`GameVersion::unknown`]。
    pub fn parse(raw: &str) -> Self {
        let trimmed = raw.trim();
        let (base, stage) = split_stage(trimmed);
        let parts = base
            .split('.')
            .map_while(|piece| piece.parse::<u32>().ok())
            .collect::<Vec<_>>();
        if parts.is_empty() {
            return Self::unknown(trimmed);
        }
        Self {
            raw: trimmed.to_string(),
            parts,
            stage,
        }
    }

    /// 未知/无法解析的版本（比较结果为“不满足任何门限”，由调用方给默认分支）。
    pub fn unknown(raw: &str) -> Self {
        Self {
            raw: raw.to_string(),
            parts: Vec::new(),
            stage: Stage::Release,
        }
    }

    /// 原始版本串。
    pub fn raw(&self) -> &str {
        &self.raw
    }

    /// 是否解析出了数字版本（`false` 表示未知形状，例如周快照）。
    pub fn is_known(&self) -> bool {
        !self.parts.is_empty()
    }

    /// 是否是**周快照**形态（`20w14a` / `26w14a`）。
    ///
    /// 周快照是「年(2 位) + `w` + 周(2 位) + 字母」的独立命名族，与 `26.2-snapshot-N`
    /// 分属两套编号，**无法**可靠排序；所以 [`Self::is_known`] 对它恒为 `false`
    /// （不猜，免得误触发「老版本」分支去套 legacy 参数模板）。要判能力请改看版本 JSON 的
    /// 库表（[`crate::game::capability`]），别在这里编一套排序规则。
    pub fn is_weekly_snapshot(&self) -> bool {
        is_weekly_snapshot_str(&self.raw)
    }

    /// 主版本号（`1.20.1` → 1，`26.2` → 26）。
    pub fn major(&self) -> u32 {
        self.parts.first().copied().unwrap_or(0)
    }

    /// 是否 `>= other`。自身或 `other` 不可解析时返回 `false`。
    pub fn is_at_least(&self, other: &str) -> bool {
        let other = Self::parse(other);
        if !self.is_known() || !other.is_known() {
            return false;
        }
        self.cmp(&other) != Ordering::Less
    }

    /// 是否 `< other`。自身不可解析时返回 `false`（未知版本不触发“老版本”分支）。
    pub fn is_before(&self, other: &str) -> bool {
        let other = Self::parse(other);
        if !self.is_known() || !other.is_known() {
            return false;
        }
        self.cmp(&other) == Ordering::Less
    }

    /// 去掉尾部 0 后的分量序列（`1.20.0` 与 `1.20` 等价）。
    fn trimmed_parts(&self) -> &[u32] {
        let mut end = self.parts.len();
        while end > 0 && self.parts[end - 1] == 0 {
            end -= 1;
        }
        &self.parts[..end]
    }
}

impl Ord for GameVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        self.trimmed_parts()
            .cmp(other.trimmed_parts())
            .then_with(|| self.stage.cmp(&other.stage))
    }
}

impl PartialOrd for GameVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for GameVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.raw)
    }
}

impl Default for GameVersion {
    fn default() -> Self {
        Self::unknown("")
    }
}

fn split_stage(raw: &str) -> (&str, Stage) {
    for marker in ["-snapshot", " snapshot"] {
        if let Some(pos) = raw.find(marker) {
            let number = trailing_number(&raw[pos + marker.len()..]);
            return (&raw[..pos], Stage::Snapshot(number));
        }
    }
    for marker in ["-pre", "-preview"] {
        if let Some(pos) = raw.find(marker) {
            let number = trailing_number(&raw[pos + marker.len()..]);
            return (&raw[..pos], Stage::PreRelease(number));
        }
    }
    if let Some(pos) = raw.find("-rc") {
        let number = trailing_number(&raw[pos + 3..]);
        return (&raw[..pos], Stage::ReleaseCandidate(number));
    }
    (raw, Stage::Release)
}

fn trailing_number(text: &str) -> u32 {
    let digits = text.trim_start_matches(['-', '.', '_']);
    digits
        .split(|ch: char| !ch.is_ascii_digit())
        .next()
        .and_then(|piece| piece.parse::<u32>().ok())
        .unwrap_or(0)
}

/// `20w14a` / `26w14a` 形态判定（纯函数，便于单测）。
fn is_weekly_snapshot_str(raw: &str) -> bool {
    let bytes = raw.trim().as_bytes();
    bytes.len() == 6
        && bytes[..2].iter().all(u8::is_ascii_digit)
        && bytes[2].eq_ignore_ascii_case(&b'w')
        && bytes[3..5].iter().all(u8::is_ascii_digit)
        && bytes[5].is_ascii_alphabetic()
}
