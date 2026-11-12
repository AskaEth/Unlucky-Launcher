//! 内嵌补充清单 `unlisted-versions.json`（205 条，编进二进制）。
//!
//! 官方清单本来就有 `old_beta`/`old_alpha`，但**不完整**：大量 alpha/classic 构建、
//! `pending` 实验快照、`_unobfuscated` 版本从未进官方清单。
//! HMCL 的做法是把社区维护的补充清单编进发行包，换取"零网络依赖也能列出旧版"，
//! 依据见 `DevDocs/analyse/03-version-download.md` §2.3。
//!
//! 数据来源：HMCL `HMCLCore/src/main/resources/assets/game/unlisted-versions.json`
//! （实测 205 条：`old_alpha 88` / `old_beta 16` / `snapshot 70` / `pending 20` /
//! `unobfuscated 11`），原样拷进 `game/data/` 由 `include_str!` 内嵌。
//!
//! **与 HMCL 的差异**：同 id 冲突时我们让**官方条目胜出**。理由是官方 v2 清单带
//! `sha1`/`size` 且更新鲜；HMCL 因先插 unlisted 进 `TreeSet` 而让社区条目抢了同 id，
//! 会把官方 URL 换成社区镜像 URL（analyse/03 §2.5）。补充清单只用来补官方缺口。
//! 后续计划（analyse/03 §2.3 建议）：把该清单改为可远程更新的数据 + ETag 缓存，
//! 内嵌版本降级为离线回退。

use std::sync::OnceLock;

use super::version::{RemoteVersion, VersionList};

/// 内嵌清单的原始 JSON（`crates/core/src/game/data/unlisted-versions.json`）。
const EMBEDDED_UNLISTED_JSON: &str = include_str!("data/unlisted-versions.json");

/// 内嵌清单预期的条目数，用于数据完整性自检与文档口径一致。
pub const EMBEDDED_VERSION_COUNT: usize = 205;

static UNLISTED: OnceLock<VersionList> = OnceLock::new();

/// 内嵌补充清单（进程内只解析一次）。
///
/// # Panics
///
/// 内嵌数据是编译期常量，可解析性由 `tests/game/unlisted.rs` 守门；
/// 若构建产物被破坏导致解析失败则 panic——这是构建错误，不是运行时输入错误。
pub fn versions() -> &'static VersionList {
    UNLISTED.get_or_init(|| {
        VersionList::from_json_str(EMBEDDED_UNLISTED_JSON)
            .expect("内嵌 unlisted-versions.json 必须可解析")
    })
}

/// 内嵌清单的条目（借用）。
pub fn entries() -> &'static [RemoteVersion] {
    &versions().versions
}

/// 把内嵌补充清单合并进给定清单，返回新增条目数。
///
/// 同 id 时**保留已有条目**（官方胜出，理由见模块注释）；新条目按数据文件原顺序追加，
/// 调用方如需展示顺序再调用 [`VersionList::sort_newest_first`]。
pub fn merge_into(target: &mut VersionList) -> usize {
    target.merge_missing(entries().iter().cloned())
}
