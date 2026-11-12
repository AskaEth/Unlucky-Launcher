//! 远端条目补全（M162）：**有 `path` 但缺 `hashes.sha1` 的 mrpack 条目，按 sha512 反查 sha1**。
//!
//! # 补什么、为什么补
//!
//! M160 的预检用 SHA-1 判「会不会覆盖用户文件」；mrpack 规范里 `sha1` 是**可选**的，只有
//! `sha512` 必填。于是「只有 path+downloads+hashes(sha512)」的条目在预检里判不了，只能记进
//! `ImportScan::unknown_remote`。现在拿 sha512 去 Modrinth 反查：站点认得这个文件就把**它自己
//! 记的 sha1** 填回来（同一个文件在同一站点上的官方摘要），补不出**保持缺**、照旧计数。
//!
//! # 三条边界
//!
//! 1. **只补 sha1 / 字节数 / 缺的直链**，绝不改写清单里已有的值（清单说多大就是多大）；
//! 2. **CurseForge 条目不走这里**：它们连路径都没有，补全要 `projectID/fileID` 端点（需要 key），
//!    那条路仍是 `PackFetcher::curse_file` 的活；
//! 3. **只有「没联网」才立刻停**：第一条 [`FetchErrorKind::Offline`] 之后剩下的条目一律算
//!    「补不出」，不拿 N 次超时惩罚用户；**404 / 5xx 不停**——真实 mrpack 的文件大多托管在
//!    CurseForge CDN，Modrinth 的 `version_file` 对它们必然 404，把「补不出」当「没联网」
//!    会让「能补就补」只在第一条恰好托管在 Modrinth 时成立（r1 P1-1 后果 B）。
//!
//! # 为什么要改 `plan.remote` 而不是 `manifest.files`
//!
//! 导入与预检都只读 `plan.remote`（`manifest` 保留原样作诊断），改一处即可两条路都生效，
//! 也不会让人误以为「清单被回写了」。

use super::fetch::FetchErrorKind;
use super::plan::ImportPlan;
use super::report::{keys, ImportIssue, ImportReport};
use super::ImportRequest;

/// 一次补全的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Completion {
    /// 补出了 sha1 的条目数
    pub(crate) filled: usize,
    /// 仍然补不出的条目数（缺 sha512 / 站点查不到 / 没接源）
    pub(crate) unfilled: usize,
    /// 是否因为「没联网」提前收工
    pub(crate) offline: bool,
}

impl Completion {
    /// 补全结果 → 报告注记（`None` = 既没补到、也没因断网停下，不必占一行）。
    ///
    /// `lead` 是场景前缀（导入用空串、预检用「预检前」）：**文案只此一处**，两个入口不许
    /// 各写一句——r1 抓到的「404 却写因未联网提前停止」正是两处文案各自演进的后果。
    /// 现在 `offline` 只由真 `Offline` 置位，这句话只会在真断网时出现。
    pub(crate) fn note(&self, lead: &str) -> Option<ImportIssue> {
        if self.filled == 0 && !self.offline {
            return None;
        }
        let mut message = if self.filled == 0 {
            // 一条也没补到（只可能是断网）：说「补全了 0 条」读起来像成功。
            format!(
                "{lead}按 sha512 从 Modrinth 补全 sha1：一条也没补到（补不出 {} 条）",
                self.unfilled
            )
        } else {
            format!(
                "{lead}按 sha512 从 Modrinth 补全了 {} 条远端条目的 sha1（补不出 {} 条）",
                self.filled, self.unfilled
            )
        };
        if self.offline {
            message.push_str(if self.filled == 0 {
                "；补全因未联网提前停止"
            } else {
                "；补全因未联网提前停止，其余条目照旧按「缺省保留」处置"
            });
        }
        Some(ImportIssue::new(keys::NOTE_REMOTE_COMPLETED, message))
    }
}

/// 补全 + 把注记写进报告（导入管线唯一的入口：一行调用，文案与计数都在本模块）。
pub(crate) async fn complete_and_note(
    request: &ImportRequest<'_>,
    plan: &mut ImportPlan,
    report: &mut ImportReport,
) {
    let completion = complete(request, plan).await;
    // 取消时这条注记会撒谎（剩下的条目**没试过**，不是"补不出"）：取消收尾的注记已经说清
    // 停在哪一步，这里不再补一句计数。
    if request.cancel.is_cancelled() {
        return;
    }
    if let Some(note) = completion.note("") {
        report.warnings.push(note);
    }
}

/// 跑一次补全（就地改 `plan.remote`）。
pub(crate) async fn complete(request: &ImportRequest<'_>, plan: &mut ImportPlan) -> Completion {
    let mut state = Completion::default();
    let Some(fetcher) = request.fetcher else {
        state.unfilled = plan
            .remote
            .iter()
            .filter(|entry| entry.path.is_some() && entry.file.sha1().is_none())
            .count();
        return state;
    };
    for entry in plan.remote.iter_mut() {
        // 取消检查点：补全是**逐条网络请求**，条目之间是天然的落点；已补的只改内存，不写盘。
        if request.cancel.is_cancelled() {
            break;
        }
        if entry.file.sha1().is_some() {
            continue;
        }
        // CF 条目（没有路径）不归这里：那是 `curse_file` 的活。
        if entry.path.is_none() {
            continue;
        }
        let Some(sha512) = entry.file.sha512().map(str::to_string) else {
            state.unfilled += 1;
            continue;
        };
        if state.offline {
            state.unfilled += 1;
            continue;
        }
        match fetcher.modrinth_file_by_sha512(&sha512).await {
            Ok(meta) => {
                // 站点给的 sha1 也要过形状检查：长度不对的「摘要」比没有更危险。
                match meta.sha1.filter(|hex| is_sha1(hex)) {
                    Some(sha1) => {
                        entry.file.hashes.insert("sha1".to_string(), sha1);
                        if entry.file.size.is_none() {
                            entry.file.size = meta.size;
                        }
                        if entry.file.downloads.iter().all(|url| url.trim().is_empty()) {
                            if let Some(url) = meta.url {
                                entry.file.downloads.push(url);
                            }
                        }
                        state.filled += 1;
                    }
                    None => state.unfilled += 1,
                }
            }
            Err(error) => {
                // 只有**真正的「没联网」**才提前收工：404（这条不在 Modrinth 上，或托管在
                // CF CDN）与 5xx 都只影响这一条，后面那些仍然可能补得出来。
                if error.kind() == FetchErrorKind::Offline {
                    state.offline = true;
                }
                state.unfilled += 1;
            }
        }
    }
    state
}

/// 40 位十六进制。
fn is_sha1(hex: &str) -> bool {
    hex.len() == 40 && hex.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    //! 覆盖 **pack 用例走不到**的两条分支（r3 复审指出的缺口）：
    //! `sha512` 缺失臂与「已有 downloads 不许追加第二个直链」。
    //!
    //! mrpack 的读者要求 `hashes.sha512` 必填，所以「有 path 但没有 sha512」在真实文件里进不来
    //! ——它是**防御分支**。直接构造 `ImportPlan` 是唯一能让它变红的办法（不是放宽断言）。

    use std::path::Path;
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::{complete, Completion};
    use crate::addon::import::fetch::{
        FetchError, FetchFuture, FetchRequest, FetchedFile, ModrinthFileMeta, PackFetcher,
    };
    use crate::addon::import::plan::{ImportPlan, PlanRemote};
    use crate::addon::import::report::FileSource;
    use crate::addon::import::ImportRequest;
    use crate::addon::pack::{PackFile, PackManifest};
    use crate::download::blocking::block_on;

    /// 只回答 `modrinth_file_by_sha512` 的假 fetcher（并记下被问了几次）。
    struct Lookup {
        meta: Result<ModrinthFileMeta, FetchError>,
        asked: AtomicU32,
    }

    impl PackFetcher for Lookup {
        fn modrinth_file_by_sha512<'a>(
            &'a self,
            _sha512: &'a str,
        ) -> FetchFuture<'a, Result<ModrinthFileMeta, FetchError>> {
            Box::pin(async move {
                self.asked.fetch_add(1, Ordering::SeqCst);
                match &self.meta {
                    Ok(meta) => Ok(meta.clone()),
                    Err(error) => Err(error.clone()),
                }
            })
        }

        fn fetch_file<'a>(
            &'a self,
            _request: FetchRequest<'a>,
        ) -> FetchFuture<'a, Result<FetchedFile, FetchError>> {
            Box::pin(async move { Err(FetchError::unsupported("本用例不下载")) })
        }
    }

    fn plan_with(file: PackFile) -> ImportPlan {
        ImportPlan {
            format: "modrinth".to_string(),
            manifest: PackManifest::default(),
            mc_version: "1.20.1".to_string(),
            loader: None,
            version_id: "1.20.1".to_string(),
            rejected: Vec::new(),
            notes: Vec::new(),
            overrides: Vec::new(),
            remote: vec![PlanRemote {
                file,
                path: Some("mods/no-sha1.jar".to_string()),
                source: FileSource::Modrinth,
                disabled: false,
            }],
            max_memory_mb: None,
        }
    }

    fn entry(sha512: Option<&str>, downloads: &[&str]) -> PackFile {
        let mut file = PackFile::with_path("mods/no-sha1.jar");
        file.downloads = downloads.iter().map(|url| (*url).to_string()).collect();
        if let Some(sha512) = sha512 {
            file.hashes.insert("sha512".to_string(), sha512.to_string());
        }
        file
    }

    fn meta(sha1: &str, url: &str) -> ModrinthFileMeta {
        ModrinthFileMeta {
            sha1: Some(sha1.to_string()),
            size: Some(16),
            url: Some(url.to_string()),
            file_name: Some("no-sha1.jar".to_string()),
        }
    }

    #[test]
    fn an_entry_without_sha512_is_counted_not_guessed() {
        // 防御分支：有 path、有 downloads、**没有 sha512** ⇒ 无从反查，如实计数、不发请求。
        let lookup = Lookup {
            meta: Ok(meta(&"a".repeat(40), "stub://from-source")),
            asked: AtomicU32::new(0),
        };
        let mut plan = plan_with(entry(None, &["stub://original"]));
        let request = ImportRequest::new(Path::new("pack.mrpack"), Path::new("instance"))
            .with_fetcher(&lookup);
        let summary = block_on(complete(&request, &mut plan));
        assert_eq!(summary.filled, 0);
        assert_eq!(summary.unfilled, 1, "缺 sha512 要计入「补不出」");
        assert_eq!(
            lookup.asked.load(Ordering::SeqCst),
            0,
            "没有摘要就不许发请求"
        );
        assert_eq!(plan.remote[0].file.sha1(), None, "补不出就是补不出，不许猜");
    }

    #[test]
    fn an_existing_download_list_is_never_extended() {
        // 已有直链 ⇒ 站点给的直链**不许**再追加一条（列表必须逐字相同）。
        let lookup = Lookup {
            meta: Ok(meta(&"a".repeat(40), "stub://from-source")),
            asked: AtomicU32::new(0),
        };
        let mut plan = plan_with(entry(Some(&"c".repeat(128)), &["stub://original"]));
        let request = ImportRequest::new(Path::new("pack.mrpack"), Path::new("instance"))
            .with_fetcher(&lookup);
        let summary = block_on(complete(&request, &mut plan));
        assert_eq!(summary.filled, 1);
        assert_eq!(plan.remote[0].file.sha1(), Some("a".repeat(40).as_str()));
        assert_eq!(
            plan.remote[0].file.downloads,
            vec!["stub://original".to_string()],
            "已有直链时不许追加第二个（长度与内容都要逐字相同）"
        );
        assert_eq!(lookup.asked.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn offline_with_nothing_filled_does_not_say_zero_items_were_filled() {
        let note = Completion {
            filled: 0,
            unfilled: 3,
            offline: true,
        }
        .note("预检前")
        .expect("断网要留一条注记");
        assert!(
            note.message.contains("一条也没补到"),
            "断网且一条没补到时不许说「补全了 0 条」：{}",
            note.message
        );
        assert!(
            !note.message.contains("补全了 0 条"),
            "文案读起来像成功：{}",
            note.message
        );
        assert!(
            !note.message.contains("缺省保留"),
            "预检场景没有「缺省保留」这回事：{}",
            note.message
        );

        // 真的补到过东西时，仍是原来那句（并带上「其余条目按缺省保留」）。
        let note = Completion {
            filled: 2,
            unfilled: 1,
            offline: true,
        }
        .note("")
        .expect("补到过就要留注记");
        assert!(note.message.contains("补全了 2 条"));
        assert!(note.message.contains("缺省保留"));

        // 没补到、也没断网（例如所有摘要站点都不认得）⇒ 不占一行。
        assert!(Completion {
            filled: 0,
            unfilled: 2,
            offline: false
        }
        .note("")
        .is_none());
    }
}
