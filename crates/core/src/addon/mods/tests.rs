//! 列表与启停的用例：**真的建盘上文件**再读回来（不 mock 文件系统）。
//!
//! 每条用例都写在「抓得住什么 / 抓不住什么」的口径里：抓得住的是判据与守卫本身，
//! 抓不住的是 Windows 与 Linux 上 rename 的细节差异（那条靠真实运行，见 handoff）。

use std::path::Path;

use super::suffix::SuffixVariant;
use super::*;
use crate::addon::temp::TempRoot;

/// 建一个运行目录 + `mods/`，并把给定文件写进去。
fn fixture(tag: &str, files: &[(&str, &[u8])]) -> TempRoot {
    let root = TempRoot::under(tag, "run");
    let mods = mods_dir(&root);
    std::fs::create_dir_all(&mods).expect("建 mods");
    for (name, body) in files {
        std::fs::write(mods.join(name), body).expect("写 mod 文件");
    }
    root
}

/// 列目录：四种后缀变体都认、禁用位只看 `.disabled`、目录不进列表。
#[test]
fn scan_lists_every_variant_and_skips_directories() {
    let root = fixture(
        "mods-scan",
        &[
            ("sodium.jar", b"a"),
            ("jei.jar.disabled", b"bb"),
            ("opt.jar.old", b"ccc"),
            ("aether.jar.disabled.old", b"dddd"),
            ("notes.txt", b"whatever"),
        ],
    );
    std::fs::create_dir(root.join("mods/sub")).expect("建子目录");

    let scan = scan(&root).expect("列目录");
    assert!(scan.exists);
    assert_eq!(scan.dir, root.join("mods"));

    let files: Vec<&str> = scan
        .entries
        .iter()
        .map(|entry| entry.file.as_str())
        .collect();
    assert_eq!(
        files,
        [
            "aether.jar.disabled.old",
            "jei.jar.disabled",
            "notes.txt",
            "opt.jar.old",
            "sodium.jar"
        ]
    );

    let entry = |file: &str| {
        scan.entries
            .iter()
            .find(|entry| entry.file == file)
            .unwrap_or_else(|| panic!("{file} 不在列表里"))
    };
    assert!(entry("sodium.jar").enabled);
    assert!(!entry("jei.jar.disabled").enabled);
    assert!(entry("opt.jar.old").enabled, "`.old` 单独出现不是禁用");
    assert!(!entry("aether.jar.disabled.old").enabled);
    // 展示名剥两个后缀（不做 -版本号 猜解）。
    assert_eq!(entry("aether.jar.disabled.old").name, "aether.jar");
    assert_eq!(entry("sodium.jar").variant, SuffixVariant::Plain);
    assert_eq!(entry("jei.jar.disabled").bytes, 2);

    assert_eq!(scan.skipped.len(), 1);
    assert_eq!(scan.skipped[0].file, "sub");
    assert_eq!(scan.skipped[0].reason, SkipReason::NotAFile);
    assert_eq!(scan.enabled_count(), 3);
    assert_eq!(scan.disabled_count(), 2);
}

/// 空实例（没有 `mods/`）不是错误：`exists = false` 的空结果。
#[test]
fn scan_treats_a_missing_mods_dir_as_empty() {
    let root = TempRoot::under("mods-empty", "run");
    let scan = scan(&root).expect("列目录");
    assert!(!scan.exists);
    assert!(scan.entries.is_empty());
    assert!(scan.skipped.is_empty());
}

/// `mods` 是符号链接就拒（不跟随）。Windows 上建目录链接要特权，建不出来就跳过这一条。
#[test]
fn scan_refuses_a_symlinked_mods_dir() {
    let root = TempRoot::under("mods-link", "run");
    let outside = root.join("elsewhere");
    std::fs::create_dir_all(&outside).expect("建外部目录");
    std::fs::write(outside.join("x.jar"), b"x").expect("写外部文件");

    let linked = link_dir(&outside, &mods_dir(&root));
    if !linked {
        eprintln!("[skip] 本机建不出目录符号链接（Windows 需要开发者模式/特权）");
        return;
    }

    let error = scan(&root).expect_err("必须拒绝符号链接");
    assert!(matches!(error, ModError::ModsDirUnusable { .. }), "{error}");
    // 启停同样拒（同一条守卫，不另记一遍）。
    let error = set_enabled(&root, "x.jar", false).expect_err("必须拒绝符号链接");
    assert!(matches!(error, ModError::ModsDirUnusable { .. }), "{error}");
}

/// 启停往返：改名真的发生、读回来状态对、重复点幂等。
#[test]
fn toggling_renames_on_disk_and_is_idempotent() {
    let root = fixture("mods-toggle", &[("sodium.jar", b"jar")]);

    let off = set_enabled(&root, "sodium.jar", false).expect("禁用");
    assert!(off.renamed);
    assert_eq!(off.to, "sodium.jar.disabled");
    assert!(!off.enabled);
    assert!(root.join("mods/sodium.jar.disabled").is_file());
    assert!(!root.join("mods/sodium.jar").exists());

    // 再禁一次：不是错误，也不改名。
    let again = set_enabled(&root, "sodium.jar.disabled", false).expect("幂等");
    assert!(!again.renamed);
    assert_eq!(again.from, again.to);
    assert!(!again.enabled);

    // 启用回来：名字与字节都回到原样。
    let on = set_enabled(&root, "sodium.jar.disabled", true).expect("启用");
    assert!(on.renamed);
    assert_eq!(on.to, "sodium.jar");
    assert!(on.enabled);
    assert_eq!(
        std::fs::read(root.join("mods/sodium.jar")).expect("读回"),
        b"jar"
    );

    let scan = scan(&root).expect("列目录");
    assert_eq!(scan.entries.len(), 1);
    assert!(scan.entries[0].enabled);
}

/// `.disabled` 只能插在 `.old` **之前**，否则读回来丢禁用位（往返用例抓的就是这个）。
#[test]
fn toggling_keeps_the_old_suffix_in_place() {
    let root = fixture("mods-old", &[("jei.jar.old", b"old")]);

    let off = set_enabled(&root, "jei.jar.old", false).expect("禁用");
    assert_eq!(off.to, "jei.jar.disabled.old");
    let listed = scan(&root).expect("列目录");
    assert_eq!(listed.entries[0].variant, SuffixVariant::DisabledOld);
    assert!(!listed.entries[0].enabled, "禁用位被 `.old` 吃掉了");

    let on = set_enabled(&root, "jei.jar.disabled.old", true).expect("启用");
    assert_eq!(on.to, "jei.jar.old");
    assert_eq!(
        scan(&root).expect("列目录").entries[0].variant,
        SuffixVariant::Old
    );
}

/// 目标名已被占用 ⇒ 报冲突，**不覆盖**那个文件。
#[test]
fn toggling_refuses_to_overwrite_an_existing_file() {
    let root = fixture(
        "mods-collision",
        &[("x.jar", b"one"), ("x.jar.disabled", b"two")],
    );

    let error = set_enabled(&root, "x.jar", false).expect_err("必须报冲突");
    assert!(matches!(error, ModError::Collision { .. }), "{error}");
    // 两个文件都还在、内容没被踩。
    assert_eq!(
        std::fs::read(root.join("mods/x.jar")).expect("读 a"),
        b"one"
    );
    assert_eq!(
        std::fs::read(root.join("mods/x.jar.disabled")).expect("读 b"),
        b"two"
    );
}

/// 路径类名字一律拒：启停只接受 `mods/` 下的裸文件名。
#[test]
fn toggling_rejects_anything_that_is_not_a_bare_file_name() {
    let root = fixture("mods-names", &[("x.jar", b"x")]);
    for bad in [
        "",
        "../x.jar",
        "mods/x.jar",
        "sub/x.jar",
        r"mods\x.jar",
        "C:/x.jar",
        ".hidden/../x.jar",
    ] {
        let error = set_enabled(&root, bad, false)
            .err()
            .unwrap_or_else(|| panic!("{bad:?} 必须被拒"));
        assert!(
            matches!(
                error,
                ModError::UnsafeName { .. } | ModError::NotFound { .. }
            ),
            "{bad:?} → {error}"
        );
    }
    // 越界改名一次都没发生。
    assert!(root.join("mods/x.jar").is_file());
}

/// 不存在的文件、目录、符号链接都不是"可启停的 mod"。
#[test]
fn toggling_refuses_missing_and_non_file_targets() {
    let root = fixture("mods-targets", &[("x.jar", b"x")]);
    std::fs::create_dir(root.join("mods/adir")).expect("建目录");

    let missing = set_enabled(&root, "nope.jar", false).expect_err("不存在的必须报");
    assert!(matches!(missing, ModError::NotFound { .. }), "{missing}");

    let dir = set_enabled(&root, "adir", false).expect_err("目录必须报");
    assert!(matches!(dir, ModError::NotARegularFile { .. }), "{dir}");
    assert!(root.join("mods/adir").is_dir(), "目录被改名了");
}

/// 没有 `mods/` 时启停报「目录不可用」（不替用户建目录——建目录是装 mod 的事）。
#[test]
fn toggling_requires_an_existing_mods_dir() {
    let root = TempRoot::under("mods-nodir", "run");
    let error = set_enabled(&root, "x.jar", false).expect_err("没有 mods 目录");
    assert!(matches!(error, ModError::ModsDirUnusable { .. }), "{error}");
    assert!(!root.join("mods").exists(), "启停不该顺手建目录");

    // `ensure_dir` 才是建目录那条路（装 mod 用）。
    ensure_dir(&root).expect("建 mods");
    assert!(root.join("mods").is_dir());
}

/// 建目录链接：Windows 上可能没特权，返回是否建成（用例据此决定跳不跳）。
fn link_dir(target: &Path, link: &Path) -> bool {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link).is_ok()
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_dir(target, link).is_ok()
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (target, link);
        false
    }
}
