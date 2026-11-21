//! 失败必须**说真话**（M319）：摘要不符要具名成 `integrity`，失败报告不许同时说「未写入」与
//! 「文件 新13」。
//!
//! 两条用例都对着 M318 真包盘查挖出来的缺陷（`DevDocs/artifacts/mrpack-real/`：
//! `report-fps-badhash-partial.json` 与 `offline-failed-human-lines.txt`），而且都**离线**：
//! 远端只走内存里的 [`ScriptedFetcher`]，一个 socket 都不开。
//!
//! | 用例 | 删掉哪一处就红 |
//! |------|----------------|
//! | [`an_integrity_failure_is_named_integrity_and_keeps_the_digest_text`] | `fetch.rs` 的 `effective_kind` 映射、`with_sources` 的逐源摘要、`phase.rs` 的 integrity 闸 |
//! | [`an_abandoned_import_does_not_report_files_it_deleted`] | `pipeline.rs` 的 `abandoned()` 清账那两行 |
//!
//! 「格式/结构坏 ≠ 摘要不符」（M323）的两条用例在子模块 [`malformed`] 里。
//!
//! # 引擎那一层的叶子成因在哪验
//!
//! 真机上「引擎判 `Integrity` ⇒ 收尾错误带叶子」需要一台「回 200、吐错误字节」的服务器
//! （= 要 socket）。本轮按离线纪律没做：`engine/tests.rs` 只钉「收尾错误一定带叶子」这条形状，
//! 这里的 [`ScriptedFetcher`] 则**照引擎收尾的真实形状**造一条错误，验证上层把它读回了 integrity。

use std::io::Write;
use std::path::{Path, PathBuf};

use super::fetch::{FetchError, FetchFuture, FetchRequest, FetchedFile, PackFetcher};
use super::report::{ImportStatus, MissingReason, MISSING_OFFLINE_DETAIL};
use super::{import_pack, ImportRequest};
use crate::addon::online::OnlineError;
use crate::addon::temp::TempRoot;
use crate::download::blocking::block_on;
use crate::download::verify::digest_bytes;
use crate::download::{DownloadError, DownloadErrorKind, HashAlgo};

/// 夹具里「好」的那一条的地址，「坏」的那一条回引擎收尾错误。
const OK_URL: &str = "stub://mods/ok.jar";
const BAD_URL: &str = "stub://mods/bad.jar";

/// 好文件的字节（不是真 jar，只要能被算摘要）。
const OK_BYTES: &[u8] = b"stub jar bytes: content matches the declared sha1";

fn sha1(bytes: &[u8]) -> String {
    digest_bytes(HashAlgo::Sha1, bytes)
}

/// 128 位十六进制的 `sha512` 形状值：导入链上**没人核它**（只核 sha1；sha512 由
/// `online::download` 那一层核，而这里的 fetcher 是内存件）。清单读入只校验形状。
fn sha512_shape() -> String {
    "a".repeat(128)
}

/// 清单声明的一条 `files[]`（`sha1` 与 `sha512` 都由夹具自己给）。
struct Entry<'a> {
    path: &'a str,
    url: &'a str,
    sha1: &'a str,
    size: usize,
}

/// 手造 mrpack：`index` 的 `files[]` 由 `entries` 决定，`overrides` 由 `overlays` 决定。
fn mrpack(tag: &str, entries: &[Entry<'_>], overlays: &[(&str, &[u8])]) -> (TempRoot, PathBuf) {
    let sha512 = sha512_shape();
    let files: Vec<String> = entries
        .iter()
        .map(|entry| {
            format!(
                r#"{{"path":"{}","hashes":{{"sha1":"{}","sha512":"{sha512}"}},"env":{{"client":"required","server":"unsupported"}},"downloads":["{}"],"fileSize":{}}}"#,
                entry.path, entry.sha1, entry.url, entry.size
            )
        })
        .collect();
    let index = format!(
        r#"{{"game":"minecraft","formatVersion":1,"versionId":"1.0.0","name":"Honest failure","files":[{}],"dependencies":{{"minecraft":"1.20.1"}}}}"#,
        files.join(",")
    );

    let root = TempRoot::new(tag);
    let source = root.join("src");
    write_file(&source.join("modrinth.index.json"), index.as_bytes());
    for (path, bytes) in overlays {
        write_file(&source.join("overrides").join(path), bytes);
    }
    let pack = root.join("pack.mrpack");
    zip_dir(&source, &pack);
    (root, pack)
}

/// 同上，但清单原文由调用方给：用来造「清单根本解析不了」的包（M323 的判据核查）。
fn mrpack_with_index(tag: &str, index: &[u8], overlays: &[(&str, &[u8])]) -> (TempRoot, PathBuf) {
    let root = TempRoot::new(tag);
    let source = root.join("src");
    write_file(&source.join("modrinth.index.json"), index);
    for (path, bytes) in overlays {
        write_file(&source.join("overrides").join(path), bytes);
    }
    let pack = root.join("pack.mrpack");
    zip_dir(&source, &pack);
    (root, pack)
}

fn write_file(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("建目录");
    }
    std::fs::write(path, bytes).expect("写文件");
}

fn zip_dir(source: &Path, target: &Path) {
    let file = std::fs::File::create(target).expect("建 zip");
    let mut writer = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let mut pending = vec![PathBuf::new()];
    while let Some(relative) = pending.pop() {
        let dir = source.join(&relative);
        let mut children: Vec<_> = std::fs::read_dir(&dir)
            .expect("读目录")
            .collect::<Result<Vec<_>, _>>()
            .expect("目录项");
        children.sort_by_key(std::fs::DirEntry::file_name);
        for child in children {
            let relative = relative.join(child.file_name());
            if child.file_type().expect("类型").is_dir() {
                pending.push(relative);
                continue;
            }
            let bytes = std::fs::read(child.path()).expect("读夹具文件");
            let entry = relative.to_string_lossy().replace('\\', "/");
            writer.start_file(entry, options).expect("写 zip 条目");
            writer.write_all(&bytes).expect("写 zip 内容");
        }
    }
    writer.finish().expect("收尾 zip");
}

/// 父目录里有没有遗留的暂存目录（`.<name>.ull-import-<pid>-<seq>`）。
fn stray_staging(parent: &Path) -> Vec<String> {
    std::fs::read_dir(parent)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.file_name().to_string_lossy().to_string())
                .filter(|name| name.contains(".ull-import-"))
                .collect()
        })
        .unwrap_or_default()
}

/// 照**引擎收尾的真实形状**造一条错误：`AllSourcesFailed` + 叶子 `Integrity` + 逐源摘要，
/// 然后走**生产链路**转换：`DownloadError → OnlineError`（`addon/online/transport.rs` 那一张映射，
/// 逐源摘要就接在那里）`→ FetchError`。绕开任何一环，本文件的断言就在证一条用户碰不到的路。
///
/// 用的就是引擎收尾那个构造点（[`DownloadError::all_sources_failed`]，文件与文本两条下载路径
/// 共用）：那里的 `.with_leaf(..)` 一丢（M318 形态），这条错误就退化成 `Network` ⇒ 上面的
/// 用例会以「`partial` + 缺文件」变红。
fn engine_integrity_error(url: &str) -> FetchError {
    let engine = DownloadError::all_sources_failed(
        format!("1 个候选源全部失败：{url}"),
        vec![format!(
            "cdn.modrinth.com: 校验不符：期望 {} 实得 {}",
            "0".repeat(40),
            "f".repeat(40)
        )],
        [DownloadErrorKind::Integrity],
        None,
    );
    FetchError::from(OnlineError::from(engine))
}

/// 内存 fetcher：`OK_URL` 写盘成功，`BAD_URL` 回引擎那种 integrity 收尾错误。
struct ScriptedFetcher;

impl PackFetcher for ScriptedFetcher {
    fn fetch_file<'a>(
        &'a self,
        request: FetchRequest<'a>,
    ) -> FetchFuture<'a, Result<FetchedFile, FetchError>> {
        Box::pin(async move {
            match request.url {
                OK_URL => {
                    if let Some(parent) = request.dest.parent() {
                        std::fs::create_dir_all(parent).expect("建落盘目录");
                    }
                    std::fs::write(request.dest, OK_BYTES).expect("写落盘字节");
                    Ok(FetchedFile {
                        bytes: OK_BYTES.len() as u64,
                    })
                }
                _ => Err(engine_integrity_error(request.url)),
            }
        })
    }
}

/// 摘要不符 = `Failed` + `import_integrity` + 说得出「哪个文件的摘要不符」，**不是**一条
/// 可容忍的「缺文件」。
///
/// M318 之前这里得到的是 `partial` + `missing[0].reason = fetch_failed` + `detail` 里只有一个
/// URL（`crates/core/src/download/engine.rs:286` 漏了 `.with_leaf(leaf_of(kinds))`）。
#[test]
fn an_integrity_failure_is_named_integrity_and_keeps_the_digest_text() {
    let declared = "0".repeat(40);
    let (root, pack) = mrpack(
        "honest-badhash",
        &[Entry {
            path: "mods/bad.jar",
            url: BAD_URL,
            sha1: &declared,
            size: 42,
        }],
        &[],
    );
    let target = root.join("instance");

    let report = block_on(import_pack(
        &ImportRequest::new(&pack, &target).with_fetcher(&ScriptedFetcher),
    ));

    assert_eq!(
        report.status,
        ImportStatus::Failed,
        "内容不可信必须整包失败，不许降级成「缺文件」：missing={:?}",
        report.missing
    );
    let error = report.error.as_ref().expect("具名错误");
    assert_eq!(
        error.code(),
        "import_integrity",
        "成因要具名成 integrity（不是 import_fetch / 网络）：{}",
        error.message()
    );
    let message = error.message();
    assert!(
        message.contains("mods/bad.jar") && message.contains("摘要与清单声明不符"),
        "要指名「哪个文件的摘要不符」：{message}"
    );
    assert!(
        message.contains("校验不符：期望") && message.contains(&declared),
        "引擎那句原文（「校验不符：期望 … 实得 …」）不许被吞掉：{message}"
    );
    assert!(
        report.missing.is_empty(),
        "这条不是「搞不到」，不许同时记缺文件：{:?}",
        report.missing
    );
    assert!(!target.exists(), "失败后目标目录必须零残留");
}

/// 失败报告里**不许**留下「装了多少」的账：那些字节写进的是临时目录，随后连临时目录一起删了。
///
/// M318 的真包盘查里这条报告同时说「失败（未写入：目标目录没被动过）」与「文件 新13 / 4764 字节」。
#[test]
fn an_abandoned_import_does_not_report_files_it_deleted() {
    let ok_sha1 = sha1(OK_BYTES);
    let (root, pack) = mrpack(
        "honest-abandoned",
        &[
            Entry {
                path: "mods/ok.jar",
                url: OK_URL,
                sha1: &ok_sha1,
                size: OK_BYTES.len(),
            },
            // 第二条在**第一条已经落进暂存目录之后**挂掉：正好是"有过账、但一条都没落地"的形状。
            Entry {
                path: "mods/bad.jar",
                url: BAD_URL,
                sha1: &"0".repeat(40),
                size: 42,
            },
        ],
        &[
            ("config/a.toml", b"key = \"a\"\n"),
            ("options.txt", b"fov:0.5\n"),
        ],
    );
    let target = root.join("instance");

    let report = block_on(import_pack(
        &ImportRequest::new(&pack, &target).with_fetcher(&ScriptedFetcher),
    ));

    assert_eq!(report.status, ImportStatus::Failed);
    assert_eq!(
        report.error.as_ref().expect("具名错误").code(),
        "import_integrity"
    );
    assert!(report.rolled_back, "目标目录没被动过 ⇒ 与导入前一致");
    assert_eq!(
        report.installed(),
        0,
        "「失败（未写入）」不许配「文件 新 N」：{:?}",
        report.files
    );
    assert_eq!(report.bytes_written, 0, "写进临时目录的字节不算写出");
    assert!(report.files.is_empty(), "{:?}", report.files);
    let notes: Vec<&str> = report
        .warnings
        .iter()
        .map(|note| note.message.as_str())
        .collect();
    assert!(
        notes
            .iter()
            .any(|line| line.contains("未被写入") && line.contains("3 条文件")),
        "三条（2 条 overrides + 1 条远端）要有一个去处说明：{notes:?}"
    );
    assert!(!target.exists(), "失败后目标目录必须零残留");
    assert!(
        stray_staging(root.path()).is_empty(),
        "临时目录没清掉：{:?}",
        stray_staging(root.path())
    );
}

// ------------------------------------- M325：「离线」那一档的 detail 不许自称「离线」

/// 「没接 fetcher」那一档的 `detail` **不许再出现「离线」**（债务台账 P30 第 2 条）。
///
/// # 为什么钉在这里（而不是只钉那句常量）
///
/// 措辞是在 [`super::phase::missing_for`] 的 `else` 分支上**产出**的。只断言常量（`items.rs`）或者
/// 只断言渲染层（cli 的 m325 用例）都会放过一种真会发生的改法：**分支换回字面量、常量留着不用**
/// ——那时常量测试照样绿，而用户又看到「离线导入：…（离线）…」。本用例走的是生产链路
/// （`import_pack` → `phase::resolve` → `missing_for`），所以无论重复从常量来还是从字面量来都红。
///
/// 夹具要点：条目**带路径但 `downloads` 为空**才走 `missing_for`（`downloads` 有地址、只是没接
/// fetcher 时走的是另一条路 → `FetchFailed` + 引擎那句「本次导入没有 fetcher（离线）」，
/// 那句只有一次「离线」，不在本条判据内）。
///
/// # 变异判据
///
/// - `phase.rs` 的 `else` 分支换回 `"本次导入没有接 fetcher（离线），远端文件一律跳过"`（常量保留）
///   ⇒ 「不许含离线」那条红；
/// - `missing_for` 不再走 `Offline` 这一档（比如退回 `NoSource`）⇒ 成因断言红；
/// - 把 `detail` 整句丢掉（空串）⇒ 与常量相等那条红（去重复 ≠ 去信息）。
#[test]
fn the_offline_missing_detail_does_not_call_itself_offline_again() {
    let index = format!(
        r#"{{"game":"minecraft","formatVersion":1,"versionId":"1.0.0","name":"offline detail","files":[{{"path":"mods/ghost.jar","hashes":{{"sha1":"{}","sha512":"{}"}},"env":{{"client":"required","server":"unsupported"}},"downloads":[],"fileSize":11}}],"dependencies":{{"minecraft":"1.20.1"}}}}"#,
        sha1(b"ghost"),
        sha512_shape()
    );
    let (root, pack) = mrpack_with_index("m325-offline-detail", index.as_bytes(), &[]);
    let target = root.join("instance");
    // **不接 fetcher** + 条目没有可用地址 ⇒ `missing_for(false)`，「离线」那一档。
    let report = block_on(import_pack(&ImportRequest::new(&pack, &target)));

    assert_eq!(
        report.missing.len(),
        1,
        "这个包有一条补不出地址的远端条目、又没接 fetcher，缺文件必须恰好一条：{:?}（错误：{:?}）",
        report.missing,
        report.error
    );
    let missing = &report.missing[0];
    assert_eq!(
        missing.reason,
        MissingReason::Offline,
        "没接 fetcher 的那一档必须记成离线：{missing:?}"
    );
    // 壳层的标签已经是「离线导入」；这句 detail 里再出现一次就是重复（改回原措辞必红）。
    assert!(
        !missing.detail.contains("离线"),
        "离线那一档的 detail 又自称离线了：{}",
        missing.detail
    );
    // 去重复不等于去信息：仍旧要说清为什么跳过，且只有一处真相（与公开常量同一句）。
    assert_eq!(
        missing.detail, MISSING_OFFLINE_DETAIL,
        "离线档的 detail 与常量不是同一句（是不是分支里另写了字面量？）：{}",
        missing.detail
    );
}

mod malformed;
