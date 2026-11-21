//! 规则库完整性 + 每条规则的样本回归（HMCL `CrashReportAnalyzerTest` 同构）。

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::prelude::*;

#[test]
fn every_rule_has_a_sample_that_hits_itself() {
    let loader = RuleLoader::embedded();
    let rules = loader.rules();
    // 64 = HMCL 59 条全量平移 + 5 条 Vulkan 组（rules.d/graphics.yaml 末尾：
    // vulkan_loader_missing / vulkan_device_version_too_low / vulkan_missing_extension /
    // vulkan_missing_feature / no_graphics_backend）。加规则记得同时加样本并改这个数。
    assert_eq!(rules.len(), 64, "HMCL 59 条 + Vulkan 5 条规则应全量在位");
    for rule in rules {
        let sample = rule
            .rule()
            .sample
            .as_deref()
            .unwrap_or_else(|| panic!("规则 {} 没有 sample 字段", rule.id()));
        let text = read_sample(sample);
        let report = analyze_text(SignalOrigin::Stdout, &text);
        let hit = report
            .findings
            .iter()
            .chain(report.secondary.iter())
            .any(|finding| finding.rule_id == rule.id());
        assert!(
            hit,
            "规则 {} 的样本 {} 没有命中它自己（实际命中：{:?}）",
            rule.id(),
            sample,
            ids(&report)
        );
    }
}

#[test]
fn rule_ids_are_unique_and_namespaced() {
    let loader = RuleLoader::embedded();
    let mut seen = BTreeSet::new();
    for rule in loader.rules() {
        assert!(seen.insert(rule.id()), "规则 id 重复：{}", rule.id());
        let (namespace, name) = rule
            .id()
            .split_once('.')
            .unwrap_or_else(|| panic!("规则 id 必须带命名空间：{}", rule.id()));
        assert!(
            !namespace.is_empty() && !name.is_empty(),
            "规则 id 不合法：{}",
            rule.id()
        );
    }
}

#[test]
fn rules_carry_cause_actions_and_provenance() {
    for rule in RuleLoader::embedded().rules() {
        let rule = rule.rule();
        assert!(
            rule.cause.key.starts_with("diagnose.cause."),
            "规则 {} 的 cause.key 应以 diagnose.cause. 开头",
            rule.id
        );
        assert!(
            !rule.cause.summary.is_empty(),
            "规则 {} 缺结论文案",
            rule.id
        );
        assert!(
            !rule.fix_action.is_empty(),
            "规则 {} 没有绑定六维修复动作（M16 一键修复要靠它）",
            rule.id
        );
        assert!(
            rule.source.is_some(),
            "规则 {} 缺 source（HMCL 出处）",
            rule.id
        );
        assert!(
            (0.0..=1.0).contains(&rule.confidence),
            "规则 {} 的 confidence 超出 0~1",
            rule.id
        );
    }
}

#[test]
fn every_reported_category_has_rules() {
    let mut categories = BTreeSet::new();
    for rule in RuleLoader::embedded().rules() {
        categories.insert(rule.category());
    }
    for category in RuleCategory::all() {
        assert!(
            categories.contains(&category),
            "分类 {category} 一条规则都没有（rules.d 里缺这个文件或为空）"
        );
    }
}

#[test]
fn no_orphan_samples() {
    let samples_root = rules_dir().join("samples");
    let declared = RuleLoader::embedded()
        .rules()
        .iter()
        .filter_map(|rule| rule.rule().sample.clone())
        .collect::<BTreeSet<_>>();
    let mut on_disk = BTreeSet::new();
    for group in std::fs::read_dir(&samples_root).expect("samples 目录") {
        let group = group.expect("目录项").path();
        if !group.is_dir() {
            continue;
        }
        collect(&samples_root, &group, &mut on_disk);
    }
    for path in &on_disk {
        assert!(
            declared.contains(path),
            "样本 {path} 没有规则引用它（孤儿样本，或 sample: 路径写错）"
        );
    }
}

/// 递归收集 `.log` 样本，返回相对 `rules.d/` 的路径。
fn collect(root: &Path, dir: &Path, out: &mut BTreeSet<String>) {
    for entry in std::fs::read_dir(dir).expect("目录项") {
        let path = entry.expect("目录项").path();
        if path.is_dir() {
            collect(root, &path, out);
        } else if path.extension().is_some_and(|ext| ext == "log") {
            let relative = path
                .strip_prefix(root)
                .expect("相对路径")
                .to_string_lossy()
                .replace('\\', "/");
            // hs_err 样本是解析器测试素材，不对应规则。
            if !relative.starts_with("hs_err/") {
                out.insert(format!("samples/{relative}"));
            }
        }
    }
}

#[test]
fn decisive_rule_gives_evidence_message_and_actions() {
    let text = read_sample("samples/integrity/mod_files_decompressed.log");
    let report = analyze_text(SignalOrigin::Stdout, &text);
    let primary = report.primary().expect("应当命中");
    assert_eq!(primary.rule_id, "integrity.mod_files_decompressed");
    assert!(!primary.message.is_empty());
    assert_eq!(primary.severity, Severity::Critical);
    let evidence = primary.evidence.first().expect("证据");
    assert_eq!(evidence.origin, SignalOrigin::Stdout);
    assert!(evidence.render().starts_with("stdout:"));
    assert!(!primary.fix_actions.is_empty());

    // 高置信结论不该再要求 AI 深化
    assert!(
        !report.needs_more_info(),
        "0.95 置信度的结论被判成「需要更多信息」"
    );
    assert_eq!(report.rules_evaluated, 64);
}

/// Vulkan 组：语义是「Vulkan 后端起不来」，所以**成功的** Vulkan 日志不该命中任何一条。
#[test]
fn healthy_vulkan_startup_hits_no_vulkan_rule() {
    let text = "\
[12:00:21] [Render thread/INFO]: Backend library: LWJGL version 3.4.3+13
[12:00:21] [Render thread/INFO]: Using graphics backend VULKAN, using drivers: AMD proprietary driver
[12:00:21] [Render thread/INFO]: OpenGL debug message callback registered
";
    let report = analyze_text(SignalOrigin::Stdout, text);
    let hit = report
        .findings
        .iter()
        .chain(report.secondary.iter())
        .map(|finding| finding.rule_id.as_str())
        .filter(|id| id.starts_with("graphics.vulkan") || *id == "graphics.no_graphics_backend")
        .collect::<Vec<_>>();
    assert!(hit.is_empty(), "正常的 Vulkan 启动日志被判成故障：{hit:?}");
}

/// Vulkan 规则必须落在 `graphics_api` 这个新维度上（否则 agent 侧不知道该改哪一项）。
#[test]
fn vulkan_rules_point_at_the_graphics_api_setting() {
    let loader = RuleLoader::embedded();
    for id in [
        "graphics.vulkan_loader_missing",
        "graphics.vulkan_device_version_too_low",
        "graphics.vulkan_missing_extension",
        "graphics.vulkan_missing_feature",
        "graphics.no_graphics_backend",
    ] {
        let compiled = loader
            .rule_set()
            .get(id)
            .unwrap_or_else(|| panic!("规则 {id} 不存在"));
        let rule = compiled.rule();
        let settings = rule
            .fix_action
            .iter()
            .map(|action| action.materialize(&BTreeMap::new()))
            .filter(|action| {
                action
                    .args
                    .get("setting")
                    .is_some_and(|setting| setting == "graphics_api")
            })
            .count();
        assert!(
            settings > 0,
            "规则 {id} 没有 change_graphics/graphics_api 动作"
        );
        assert!(
            rule.fix_action
                .iter()
                .any(|action| action.kind().as_str() == "change_graphics"),
            "规则 {id} 的修复动作应属于 change_graphics 维度"
        );
    }
}

/// 后端全挂（两个都失败）应当压过单条 vulkan_* 原因，避免同一个故障报五遍。
#[test]
fn no_graphics_backend_suppresses_single_reason_rules() {
    let text = read_sample("samples/graphics/no_graphics_backend.log");
    let report = analyze_text(SignalOrigin::Stdout, &text);
    assert_eq!(
        report.primary().expect("命中").rule_id,
        "graphics.no_graphics_backend"
    );
}

/// SDL 变体必须保持 macOS 特异性：`cocoa` 是 SDL 在 macOS 的视频驱动名，两者同现才算
/// 本规则命中。否则 Windows/Linux 的 SDL_Init 失败会被误归因成「macOS 显示器问题」。
#[test]
fn sdl_display_failure_is_attributed_to_macos_only_with_cocoa() {
    let hit = |text: &str| {
        let report = analyze_text(SignalOrigin::Stdout, text);
        report
            .findings
            .iter()
            .chain(report.secondary.iter())
            .any(|finding| finding.rule_id == "graphics.macos_display_port")
    };

    let macos = "\
[12:00:21] [Render thread/ERROR]: java.lang.IllegalStateException: SDL_Init(SDL_INIT_VIDEO) failed: cocoa: Failed to create window
";
    assert!(hit(macos), "macOS（cocoa）的 SDL 初始化失败应命中原规则");

    for other in [
        // Windows：SDL 的 windows 驱动，文本里没有 cocoa。
        "[12:00:21] [Render thread/ERROR]: SDL_Init(SDL_INIT_VIDEO) failed: No available video device\n",
        // Linux：x11/wayland 驱动。
        "[12:00:21] [Render thread/ERROR]: SDL_Init(SDL_INIT_VIDEO) failed: x11: Could not connect to display\n",
    ] {
        assert!(
            !hit(other),
            "非 macOS 的 SDL 初始化失败不该归因到 macOS 显示器规则：{other}"
        );
    }
}
