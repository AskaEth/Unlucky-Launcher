//! M325（债务台账 P30 收尾）：两条「用户看得到的话里出现内部标识 / 重复」。
//!
//! | 用例 | 抓得住什么（变异） |
//! |------|--------------------|
//! | [`the_processor_line_names_the_java_source_in_plain_words`] | `processor_lines` 改回印 `java_origin.as_str()` ⇒ 五个 slug 露进人读行；`java_origin_key` 指错 key（两档并一句）⇒ 逐档对账红；删掉某个臂 ⇒ 编译不过（穷尽匹配） |
//! | [`the_offline_missing_line_says_offline_only_once`] | core 那句 `detail` 改回自称「离线」⇒ 同一行里「离线」数变 2；把 `detail` 整句丢掉 ⇒ 「去重复不等于去信息」那条红 |
//!
//! **为什么挂在本文件而不是 `tests.rs`**：那个文件已是 399 行（400 行红线），再加一条 `#[path]`
//! 挂载就超限；`probe/tests.rs` 挂 `tests_guards.rs` 是同一种做法（子测试文件由同级测试文件挂）。
//!
//! 两条都**离线**：报告条目是手搓的，不碰网络、不碰真包。

use std::path::PathBuf;

use unlucky_core::addon::import::{
    ImportReport, MissingFile, MissingReason, ProcessorStep, ProcessorSummary,
    MISSING_OFFLINE_DETAIL,
};
use unlucky_core::game::java::JavaOrigin;
use unlucky_i18n::{Catalog, Key, Lang};

use super::super::outcome;
use super::{ctx_at, TempDir};

use crate::cli::PackImportArgs;

fn args() -> PackImportArgs {
    PackImportArgs {
        path: PathBuf::from("pack.mrpack"),
        target: None,
        name: None,
        overwrite: false,
        no_register: false,
    }
}

/// 一份只差「来源」的处理器汇总（其余字段照真实现填，形状同 `ProcessorSummary::from_run`）。
fn summary_with(origin: JavaOrigin) -> ProcessorSummary {
    ProcessorSummary {
        java: "C:/fake/java/bin/java.exe".to_string(),
        java_origin: origin,
        java_major: Some(17),
        executed: 1,
        skipped: 0,
        steps: vec![ProcessorStep {
            index: 1,
            label: "net.minecraftforge:installertools:1.3.0".to_string(),
            status: "executed",
            outputs: 1,
            elapsed_ms: 12,
        }],
        warnings: Vec::new(),
    }
}

/// 走真的 `outcome()` 渲染，取出「处理器：…」那一行（人读出口，不是自己拼的字符串）。
fn processor_line_with(origin: JavaOrigin) -> String {
    let dir = TempDir::new("m325-java");
    let ctx = ctx_at(dir.path());
    let mut report = ImportReport::empty(PathBuf::from("C:/tmp/instance"));
    report.processors = Some(summary_with(origin));
    let heard = outcome(
        &ctx,
        &report,
        &PathBuf::from("C:/tmp/instance"),
        &[],
        &args(),
        None,
    );
    let human = &heard.report.human;
    human
        .iter()
        .find(|line| line.starts_with("处理器："))
        .unwrap_or_else(|| panic!("没有处理器汇总行：{human:?}"))
        .clone()
}

/// 五档来源各印**目录里那句人话**，一个 slug 都不许露出来。
///
/// 断言三件事，对应三种真会发生的坏法：
/// 1. **没有 slug**（`instance` / `managed` / `JAVA_HOME` / `PATH` / `system`）——改回
///    `java_origin.as_str()` 那一行就红；
/// 2. **来源信息还在**（行里仍有「来源」二字）——为了去 slug 把来源整段删掉也红；
/// 3. **逐档对账目录文案**——`java_origin_key` 里两档指到同一个 key（或 `System` 指成
///    `FromPath`）⇒ 该档那句对不上，红。删掉某个臂则**编译不过**（穷尽匹配，比字符串针更硬）。
#[test]
fn the_processor_line_names_the_java_source_in_plain_words() {
    let catalog = Catalog::of(Lang::ZhCn);
    let expected = [
        (JavaOrigin::Instance, Key::PackProcessorJavaFromInstance),
        (JavaOrigin::Managed, Key::PackProcessorJavaFromLauncher),
        (JavaOrigin::JavaHome, Key::PackProcessorJavaFromEnvVar),
        (JavaOrigin::Path, Key::PackProcessorJavaFromPath),
        (JavaOrigin::System, Key::PackProcessorJavaFromSystem),
    ];
    let slugs = ["instance", "managed", "JAVA_HOME", "PATH", "system"];
    let mut lines = Vec::new();

    for (origin, key) in expected {
        let line = processor_line_with(origin);
        assert!(
            line.contains("来源"),
            "{origin:?} 的来源信息被删掉了（去 slug 不等于去事实）：{line}"
        );
        assert!(
            line.contains(catalog.text(key)),
            "{origin:?} 那一档没取到目录文案「{}」：{line}",
            catalog.text(key)
        );
        for slug in slugs {
            assert!(
                !line.contains(slug),
                "{origin:?} 那一行把内部 slug `{slug}` 印给了用户：{line}"
            );
        }
        assert!(
            !lines.contains(&line),
            "两档来源印成了同一句（`java_origin_key` 指错 key）：{line}"
        );
        lines.push(line);
    }
    assert_eq!(lines.len(), 5, "{lines:#?}");
}

/// 「离线」那一档的缺文件行：`离线` 只出现一次，而且仍旧说清了为什么跳过。
///
/// 修的是 **core 那句 `detail` 自己**（[`MISSING_OFFLINE_DETAIL`]）：壳层的标签已经是「离线导入」，
/// 句子里再写一次就成了「离线导入：…（离线）…」。断言：
///
/// - `离线` 恰好 1 次 ⇒ core 那句改回自称「离线」必红；
/// - 行里仍有那句 `detail`（「没有接 fetcher，远端文件一律跳过」）⇒ 把 detail 整句删掉也红
///   （去重复不等于去信息）。
#[test]
fn the_offline_missing_line_says_offline_only_once() {
    let dir = TempDir::new("m325-offline");
    let ctx = ctx_at(dir.path());
    let mut report = ImportReport::empty(PathBuf::from("C:/tmp/instance"));
    report.missing = vec![MissingFile {
        path: Some("mods/offline.jar".to_string()),
        display: "mods/offline.jar".to_string(),
        project_id: None,
        file_id: None,
        reason: MissingReason::Offline,
        message_key: MissingReason::Offline.message_key(),
        detail: MISSING_OFFLINE_DETAIL.to_string(),
    }];
    let heard = outcome(
        &ctx,
        &report,
        &PathBuf::from("C:/tmp/instance"),
        &[],
        &args(),
        None,
    );
    let human = &heard.report.human;
    let line = human
        .iter()
        .find(|line| line.starts_with("缺文件："))
        .unwrap_or_else(|| panic!("没有缺文件那一行：{human:?}"));

    assert_eq!(
        line.matches("离线").count(),
        1,
        "同一行里「离线」说了不止一次：{line}"
    );
    assert!(line.contains("离线导入"), "标签还在：{line}");
    assert!(
        line.contains(MISSING_OFFLINE_DETAIL),
        "去重复不等于去信息：core 那句 detail 不能丢：{line}"
    );
    // `--json` 那一位仍是同一句原文（脚本读的 `missing[].detail` 形状没变）。
    assert_eq!(
        heard.report.data["missing"][0]["detail"], MISSING_OFFLINE_DETAIL,
        "机器位的 detail 被改了形状"
    );
}
