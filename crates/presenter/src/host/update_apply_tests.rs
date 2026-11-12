//! 应用（下载 → 双检 → 替换）的用例：校验失败不许碰目标、成功要留备份。
//!
//! 夹具（`TempDir` / `factory` / `job` / `manifest_for`）在父模块，本文件 `use super::*`。

use super::*;

// ------------------------------------------------------------------ 应用

/// 摘要不符 ⇒ 拒绝应用 + 暂存文件不残留 + 目标文件没被换掉。
#[test]
fn sha256_mismatch_refuses_the_apply_and_leaves_no_temp_file() {
    let tmp = TempDir::new("digest");
    let good = b"new-binary-v2";
    let evil = b"evil-binary-x"; // 同样长度：先过长度判据，才能测到摘要这一关
    assert_eq!(good.len(), evil.len());
    // 清单声明的是 good 的摘要，盘上给的却是 evil。
    dir_source(&tmp, &manifest_for("0.9.0", good, ""), evil);

    let dest = tmp.join("bin/unlucky-launcher.exe");
    std::fs::create_dir_all(dest.parent().expect("父目录")).expect("bin");
    std::fs::write(&dest, b"old-binary-v1").expect("旧版本");

    let report = Report::applied(job(&tmp).apply(&version("0.2.2"), Channel::Stable));
    match report {
        Report::Applied(ApplyReport::Failed(failure)) => {
            assert_eq!(failure.kind, "digest_mismatch", "失败分类要能区分摘要不符");
            assert!(failure.retryable, "摘要不符是可重试的（换源/重下可能变好）");
        }
        other => panic!("摘要不符必须拒绝应用，实际 {other:?}"),
    }

    // 目标文件一个字节都没动
    assert_eq!(
        std::fs::read(&dest).expect("读旧版本"),
        b"old-binary-v1",
        "校验失败不许碰目标文件"
    );
    // 暂存目录里不许留 .ullupd 中间文件
    let staging = tmp.join("staging");
    let leftovers: Vec<String> = std::fs::read_dir(&staging)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    assert!(
        leftovers.is_empty(),
        "校验失败后暂存目录里还有文件：{leftovers:?}"
    );
}

/// 正常应用：新字节就位、旧字节进备份、回报是「已就位」。
#[test]
fn a_successful_apply_leaves_the_new_file_and_a_backup() {
    let tmp = TempDir::new("apply");
    let payload = b"new-binary-v2";
    dir_source(
        &tmp,
        &manifest_for("0.9.0", payload, "修掉启动崩溃"),
        payload,
    );

    let dest = tmp.join("bin/unlucky-launcher.exe");
    std::fs::create_dir_all(dest.parent().expect("父目录")).expect("bin");
    std::fs::write(&dest, b"old-binary-v1").expect("旧版本");

    let applied = job(&tmp)
        .apply(&version("0.2.2"), Channel::Stable)
        .expect("应用成功");
    let Applied::Installed { receipt, version } = applied else {
        panic!("应当装上");
    };
    assert_eq!(version, "0.9.0");
    assert_eq!(std::fs::read(&dest).expect("读新版本"), payload);
    let backup = receipt.backup.clone().expect("旧版本应留备份");
    assert_eq!(
        std::fs::read(&backup).expect("读备份"),
        b"old-binary-v1",
        "回滚用的旧版本要在备份里"
    );

    // 回报 → 快照：「就位」状态让提示条变成「重启后生效」
    let mut runtime = Runtime::with_factory(&HostEnv::at_root(tmp.join("root")), factory(&tmp));
    runtime.adopt(Report::applied(Ok(Applied::Installed { receipt, version })));
    let view = runtime.snapshot();
    assert!(view.ready.is_some(), "就位状态要进快照");
    assert!(matches!(view.banner(), Some(UpdateBanner::Ready(_))));
    assert!(!view.applying);
}
