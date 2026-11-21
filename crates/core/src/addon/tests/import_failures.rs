//! **必须先失败**的样本：四条恶意/坏包 + 两条"证明双检真的在导入链上跑"的用例。
//!
//! # 复审要的那张表
//!
//! | 样本 | 期望结果 | 断言在哪 |
//! |------|----------|----------|
//! | 路径穿越条目（`../evil.jar`） | `Failed` + `pack_traversal` + 目标目录零残留 | [`traversal_entry_rolls_back_with_zero_residue`] |
//! | 符号链接条目 | `Failed` + `pack_symlink` + 零残留 | [`symlink_entry_rolls_back_with_zero_residue`] |
//! | 损坏清单（非法 JSON） | `Failed` + `pack_metadata` + 零残留 | [`corrupt_manifest_rolls_back_with_zero_residue`] |
//! | 声明与实际不符（sha1 不符） | `Failed` + `import_integrity` + 零残留 | [`declared_hash_mismatch_rolls_back_everything`] |
//!
//! # 零残留的验证方式
//!
//! [`assert_clean`]：①目标目录不存在；②目标目录的父目录里没有 `.ull-import-*` 临时目录
//! （临时目录是"事务没收拾干净"的唯一痕迹）；③合并态用例另外断言目标目录内容与导入前
//! **逐项相同**。三条合起来就是"失败后既没有半个实例，也没有残留垃圾"。
//!
//! # 两条双检证明
//!
//! 路径穿越是**两条独立检查**（M110 `pack::path::normalize_file` + `zip` 的
//! `enclosed_name()`）。要证明两条都真的接在导入链上，得各给一条**只有其中一条能拦**的样本：
//!
//! - 绝对路径 `/etc/passwd`：`enclosed_name()` 在深度 0 处**放行**（它允许根目录），只有第一检
//!   能拦 ⇒ 它失败就说明第一检活着；
//! - Windows 设备名 `NUL`：第一检七条规则全放行，只有第二检（按 Win32 命名空间解析）能拦
//!   ⇒ 它失败就说明第二检活着（该用例仅在 Windows 宿主上有效，与 M110 的用例同构）。

use std::path::Path;

use crate::addon::import::{import_pack, ImportRequest, ImportStatus};
use crate::download::blocking::block_on;

use super::support::{StubFetcher, JEI_BYTES, JEI_PLACEHOLDER_URL, OPTIONAL_PLACEHOLDER_URL};
use super::{
    fixtures, list_tree, read_text, write_file, write_zip, write_zip_with_symlink, TempRoot,
};

/// 一份形状合法的最小 CF 清单（恶意包要能被"探测成功"，失败必须发生在它该发生的阶段）。
const CF_MANIFEST: &[u8] = br#"{"manifestType": "minecraftModpack", "manifestVersion": 1,
  "name": "evil", "version": "1.0.0", "overrides": "overrides",
  "minecraft": {"version": "1.20.1", "modLoaders": []}, "files": []}"#;

/// 断言"目标目录零残留"：目录不存在、父目录里没有临时目录。
fn assert_clean(target: &Path) {
    assert!(
        !target.exists(),
        "目标目录必须不存在（零残留），实际：{:?}",
        list_tree(target)
    );
    let parent = target.parent().expect("target 有父目录");
    if let Ok(entries) = std::fs::read_dir(parent) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            assert!(!name.contains(".ull-import-"), "临时目录没清掉：{name}");
        }
    }
}

/// 恶意样本包 + 目标目录。返回的 `TempRoot` **`Deref` 到那个 zip**，同时持有临时根：
/// 用它喂 `ImportRequest::new(&pack, ..)` 之后包文件还在（不带守卫的话函数一返回包就被删了，
/// 导入会以 `pack_io` 失败——那是"测错了东西还看不出来"的典型）。
fn failing(tag: &str, entries: &[(&str, &[u8])]) -> (TempRoot, std::path::PathBuf) {
    let pack = TempRoot::at(tag, "evil.zip");
    write_zip(&pack, entries);
    let target = pack.parent().expect("临时根").join("instance");
    (pack, target)
}

#[test]
fn traversal_entry_rolls_back_with_zero_residue() {
    let (pack, target) = failing(
        "fail-traversal",
        &[("manifest.json", CF_MANIFEST), ("../evil.jar", b"payload")],
    );
    let report = block_on(import_pack(&ImportRequest::new(&pack, &target)));

    assert_eq!(report.status, ImportStatus::Failed);
    assert_eq!(
        report.error.as_ref().expect("具名错误").code(),
        "pack_traversal"
    );
    assert!(report.rolled_back);
    assert_clean(&target);
    assert_eq!(report.status.exit_code(), 1);
}

#[test]
fn absolute_entry_proves_the_first_check_is_live() {
    // `enclosed_name()` 会放行深度 0 的绝对路径，只有 `normalize_file` 能拦。
    let (pack, target) = failing(
        "fail-absolute",
        &[
            ("manifest.json", CF_MANIFEST),
            ("/etc/passwd", b"root::0:0"),
        ],
    );
    let report = block_on(import_pack(&ImportRequest::new(&pack, &target)));

    assert_eq!(report.status, ImportStatus::Failed);
    assert_eq!(
        report.error.as_ref().expect("具名错误").code(),
        "pack_traversal"
    );
    assert_clean(&target);
}

#[cfg(windows)]
#[test]
fn windows_device_name_proves_the_second_check_is_live() {
    // `NUL` 过得了第一检的七条规则，只有 `zip` 的 `enclosed_name()`（Win32 命名空间）会拦。
    let (pack, target) = failing(
        "fail-device",
        &[("manifest.json", CF_MANIFEST), ("mods/NUL", b"x")],
    );
    let report = block_on(import_pack(&ImportRequest::new(&pack, &target)));

    assert_eq!(report.status, ImportStatus::Failed);
    assert_eq!(
        report.error.as_ref().expect("具名错误").code(),
        "pack_traversal"
    );
    assert_clean(&target);
}

#[test]
fn symlink_entry_rolls_back_with_zero_residue() {
    let root = TempRoot::new("fail-symlink");
    let pack = root.join("evil.zip");
    let entries: [(&str, &[u8]); 1] = [("manifest.json", CF_MANIFEST)];
    write_zip_with_symlink(&pack, &entries, "mods/evil.jar", "../../../etc/passwd");
    let target = root.join("instance");

    let report = block_on(import_pack(&ImportRequest::new(&pack, &target)));
    assert_eq!(report.status, ImportStatus::Failed);
    assert_eq!(
        report.error.as_ref().expect("具名错误").code(),
        "pack_symlink"
    );
    assert_clean(&target);
}

#[test]
fn corrupt_manifest_rolls_back_with_zero_residue() {
    let (pack, target) = failing(
        "fail-json",
        &[
            ("manifest.json", b"{\"manifestType\": \"minecraftModpack\","),
            ("overrides/config/a.toml", b"k = 1\n"),
        ],
    );
    let report = block_on(import_pack(&ImportRequest::new(&pack, &target)));

    assert_eq!(report.status, ImportStatus::Failed);
    let code = report.error.as_ref().expect("具名错误").code();
    assert!(
        code == "pack_metadata" || code == "pack_unsupported",
        "坏 JSON 要报成因，实际 {code}"
    );
    assert_clean(&target);
}

#[test]
fn declared_hash_mismatch_rolls_back_everything() {
    // 原始 M110 夹具：`hashes.sha1` 是形状正确的**占位值**，必然不等于 stub 给的字节。
    // 这是"声明与实际不符"——它**不是**"暂时搞不到"，必须整包回滚。
    let fixture = fixtures::modrinth("fail-integrity");
    let root = TempRoot::new("fail-integrity-out");
    let target = root.join("instance");
    let fetcher = StubFetcher::new()
        .with_file(JEI_PLACEHOLDER_URL, JEI_BYTES)
        .with_file(OPTIONAL_PLACEHOLDER_URL, b"stub optional");
    let report = block_on(import_pack(
        &ImportRequest::new(&fixture.zip, &target).with_fetcher(&fetcher),
    ));

    assert_eq!(
        report.status,
        ImportStatus::Failed,
        "缺文件 {:?}",
        report.missing
    );
    assert_eq!(
        report.error.as_ref().expect("具名错误").code(),
        "import_integrity"
    );
    assert!(report.rolled_back);
    assert_clean(&target);
}

#[test]
fn fatal_error_in_merge_mode_leaves_the_target_untouched() {
    // 目标目录已有内容（升级/重导）：致命错误时**只**清临时目录，不动目标目录。
    let (pack, target) = failing(
        "fail-merge",
        &[("manifest.json", CF_MANIFEST), ("../evil.jar", b"payload")],
    );
    write_file(&target.join("keep.txt"), b"untouched");
    write_file(&target.join("mods/mine.jar"), b"mine");
    let before = list_tree(&target);

    let report = block_on(import_pack(&ImportRequest::new(&pack, &target)));

    assert_eq!(report.status, ImportStatus::Failed);
    assert!(report.rolled_back);
    assert_eq!(list_tree(&target), before, "目标目录必须逐项不变");
    assert_eq!(
        std::fs::read(target.join("keep.txt")).expect("keep.txt"),
        b"untouched"
    );
}

#[test]
fn commit_failure_rolls_back_partially_applied_files() {
    // 让合并态的**提交阶段**在中途失败：目标文件要被替换时必须先备份成
    // `<dest>.ullbak-<pid>`，而那个位置已被一个**非空目录**占住 ⇒ `rename(dest → backup)`
    // 必然失败（文件改不成非空目录，Windows / Unix 一致）。
    // 这覆盖 journal 回滚 + `Drop` 兜底：前半截已经搬进去的文件（基线排最前）必须被撤回。
    let root = TempRoot::new("fail-commit");
    let source = fixtures::multimc("fail-commit-src");
    let target = root.join("instance");
    let first = block_on(import_pack(&ImportRequest::new(&source, &target)));
    assert_eq!(first.status, ImportStatus::Success, "{:?}", first.error);

    // 新版本包内容变了（用户没动过 ⇒ 判据是 Install，会被暂存并尝试替换）。
    write_file(
        &source.join(".minecraft/config/known.toml"),
        b"key = \"v2\"\n",
    );
    // 备份位置被非空目录占住。
    let blocker = target.join(format!("config/known.toml.ullbak-{}", std::process::id()));
    write_file(&blocker.join("blocker.txt"), b"block");
    let before = list_tree(&target);

    let second = block_on(import_pack(&ImportRequest::new(&source, &target)));

    assert_eq!(
        second.status,
        ImportStatus::Failed,
        "提交阶段失败必须整包回滚"
    );
    assert_eq!(
        second.error.as_ref().expect("具名错误").code(),
        "pack_io",
        "{}",
        second.error.as_ref().expect("具名错误").message()
    );
    assert!(second.rolled_back);
    assert_eq!(list_tree(&target), before, "目标目录必须逐项回到导入前");
    // 提交阶段排在最前面的基线被成功搬入又被撤回：内容仍是第一次导入的那份。
    assert!(target.join(super::baseline_rel()).is_file());
    assert_eq!(
        read_text(&target.join("config/known.toml")),
        "key = \"value\"\n",
        "旧内容必须还在（新内容没落地）"
    );
}
