//! 「源选择」两个键的行为守卫（M294）。
//!
//! 判据一律断**具体的源名序列**，不断「非空」：今天不改设置也能选到镜像，「非空」抓不住
//! 「设置没被读」——本任务要证明的恰好是「改了真的变、不改逐值不变」。

use std::path::{Path, PathBuf};

use super::{apply, MirrorPreference, RegionPreference};
use crate::download::cache::now_ms;
use crate::download::config::DownloadTuning;
use crate::download::mirrors::MirrorTable;
use crate::download::region::{store_cached, RegionMode, RegionProfile, RegionTrigger};
use crate::download::shared::{engine_for, set_tuning};
use crate::download::types::{DownloadRequest, ResourceKind};

const LIBRARIES_URL: &str =
    "https://libraries.minecraft.net/com/google/guava/guava/32.0.0/guava-32.0.0.jar";

/// 一个 URL 展开后的**源名序列**（`expand()` 就是「按顺序候选源」那一步）。
fn sources(profile: &RegionProfile) -> Vec<String> {
    MirrorTable::builtin()
        .expand(LIBRARIES_URL, ResourceKind::Library, profile)
        .into_iter()
        .map(|candidate| candidate.source)
        .collect()
}

/// 一个独立数据根（共享引擎登记处是进程级、按根存，键撞了会互相干扰）。
fn root(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ull-pref-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("建缓存根");
    dir
}

/// **默认判据**：两个键都不设 / 写 `auto` / 写不认识的值 ⇒ 候选序列与不做覆盖**逐项相同**。
///
/// 这里的期望值不是「非空」而是 M294 之前 `expand()` 对同一 URL 的真实结论（国内档案
/// `["bmclapi", "official"]`、国外档案 `["official", "bmclapi"]`），所以覆盖层一旦偷偷改了顺序，
/// 断言立刻红。变异证明：把 `RegionPreference::Auto` 分支改成 `Global` ⇒ domestic 那两行红。
#[test]
fn not_set_changes_nothing() {
    let domestic = RegionProfile::mirror_first(0);
    let abroad = RegionProfile::direct(0);
    assert_eq!(sources(&domestic), ["bmclapi", "official"]);
    assert_eq!(sources(&abroad), ["official", "bmclapi"]);

    for (region, mirror) in [
        (None, None),
        (Some(""), Some("")),
        (Some("auto"), Some("auto")),
        (Some("AUTO"), Some(" Auto ")),
        (Some("mars"), Some("prefer-mirror")),
    ] {
        let tuned = DownloadTuning::default()
            .with_source_selection(region, mirror)
            .apply_source_selection(domestic.clone());
        assert_eq!(
            tuned, domestic,
            "region={region:?} mirror={mirror:?} 不该动档案"
        );
        assert_eq!(
            sources(&tuned),
            ["bmclapi", "official"],
            "region={region:?} mirror={mirror:?} 不该动候选顺序"
        );
    }
}

/// **主判据（mirror_preference）**：设成 `official` / `mirror` ⇒ 候选链第一条真的跟着换。
///
/// 与探测结论**故意取反**（国内档案 + official、国外档案 + mirror），这样「覆盖没生效」时
/// 结论仍是探测值、两条断言各自红。变异证明：`MirrorPreference::apply` 的 `Official` 分支
/// 返回原样 ⇒ 第一条还是 `bmclapi`，红。
#[test]
fn mirror_preference_decides_who_comes_first() {
    let domestic = RegionProfile::mirror_first(0);
    let abroad = RegionProfile::direct(0);

    let official = DownloadTuning::default()
        .with_source_selection(None, Some("official"))
        .apply_source_selection(domestic.clone());
    assert_eq!(official.mode, RegionMode::Direct);
    assert_eq!(
        sources(&official),
        ["official", "bmclapi"],
        "官方必须第一条"
    );
    assert_eq!(
        official.greater_china, domestic.greater_china,
        "镜像偏好只定「谁在前」，地区判定不归它管"
    );

    let mirror = DownloadTuning::default()
        .with_source_selection(None, Some("mirror"))
        .apply_source_selection(abroad.clone());
    assert_eq!(mirror.mode, RegionMode::MirrorFirst);
    assert_eq!(sources(&mirror), ["bmclapi", "official"], "镜像必须第一条");
    assert_eq!(
        mirror.gh_proxy.as_deref(),
        Some(crate::download::region::DEFAULT_GH_PROXY),
        "「镜像优先」被显式要求时补齐内置 gh-proxy 前缀（否则 GitHub 资源等于没加速）"
    );
    assert_eq!(mirror.trigger, RegionTrigger::UserPinned);
}

/// **主判据（region）**：`cn` / `global` 在探测结论之上叠一层，并改到「直连还是镜像」。
///
/// `greater_china` 也一起变（这是 `is_domestic()` 的输入 → JDK 发行版默认序），所以两个键
/// 的分工在这里可见：region 定地区，mirror_preference 定顺序。
#[test]
fn region_decides_direct_or_mirror_first() {
    let cn = DownloadTuning::default()
        .with_source_selection(Some("cn"), None)
        .apply_source_selection(RegionProfile::direct(0));
    assert_eq!(cn.mode, RegionMode::MirrorFirst);
    assert!(cn.greater_china);
    assert!(cn.is_domestic());
    assert_eq!(sources(&cn), ["bmclapi", "official"]);

    let global = DownloadTuning::default()
        .with_source_selection(Some("global"), None)
        .apply_source_selection(RegionProfile::mirror_first(0));
    assert_eq!(global.mode, RegionMode::Direct);
    assert!(!global.greater_china);
    assert!(!global.is_domestic());
    assert_eq!(global.gh_proxy, None, "直连不需要前缀");
    assert_eq!(sources(&global), ["official", "bmclapi"]);
}

/// **优先级**：两个键都显式写时，更具体的 `mirror_preference` 胜。
///
/// 模块头那条规则的守卫。变异证明：`apply()` 里把两层顺序对调 ⇒ 第一条变回 `bmclapi`，红。
#[test]
fn an_explicit_mirror_preference_wins_over_an_explicit_region() {
    let tuned = apply(
        RegionProfile::direct(0),
        RegionPreference::GreaterChina,
        MirrorPreference::Official,
    );
    assert_eq!(tuned.mode, RegionMode::Direct, "mirror_preference 优先");
    assert!(tuned.greater_china, "region 的「我在国内」仍然有效");
    assert_eq!(sources(&tuned), ["official", "bmclapi"]);
}

/// **端到端**：设置原文 → `DownloadTuning` → 进程级共享引擎 → `plan()` 的候选顺序。
///
/// 这一条才是「改了设置下次下载真的变」的判据：`plan()` 是引擎选源的决策点，`set_tuning` 是宿主
/// （presenter 的 `apply_tuning` / CLI 的 `engine_with_tuning`）唯一的灌入口。
/// 变异证明：`shared::build` 里换回 `apply_gh_proxy(region)` ⇒ 第一条仍是探测值 `bmclapi`，红。
#[test]
fn the_settings_reach_the_engines_candidate_plan() {
    let dir = root("plan");
    store_cached(
        &dir.join("region.json"),
        &RegionProfile::mirror_first(now_ms()),
    )
    .expect("写地区缓存");
    let request = DownloadRequest::new(
        ResourceKind::Library,
        LIBRARIES_URL.to_string(),
        dir.join("guava.jar"),
    );
    let plan = |dir: &Path| -> Vec<String> {
        engine_for(dir)
            .plan(&request, 0)
            .into_iter()
            .map(|candidate| candidate.source)
            .collect()
    };

    // 探测结论 = 镜像优先（不改设置时计划的第一条就是它）。
    assert_eq!(plan(&dir), ["bmclapi", "official"]);

    assert!(
        set_tuning(
            &dir,
            DownloadTuning::default().with_source_selection(None, Some("official"))
        ),
        "改了源选择必须报变更（否则不会重建引擎）"
    );
    assert_eq!(plan(&dir), ["official", "bmclapi"]);

    set_tuning(
        &dir,
        DownloadTuning::default().with_source_selection(Some("cn"), Some("mirror")),
    );
    assert_eq!(plan(&dir), ["bmclapi", "official"]);

    // 回到「不设」⇒ 回到探测结论（不是停在最后一次覆盖上）。
    set_tuning(&dir, DownloadTuning::default());
    assert_eq!(plan(&dir), ["bmclapi", "official"]);

    std::fs::remove_dir_all(&dir).ok();
}

/// **叠加次序**（r1 复审 P3-2）：`gh_proxy` 三态在源选择**之后**叠，所以
/// 「`region=cn`（⇒ 镜像优先）+ `gh_proxy=off`」⇒ 一个 gh-proxy 候选都不生成。
///
/// 这条此前只靠读 `apply_source_selection` 的实现，没有断言。变异证明：把
/// `apply_source_selection` 写成 `preference::apply(self.apply_gh_proxy(p), …)`（先 gh_proxy 后
/// 源选择）⇒ 前缀被重判成内置默认，`off` 那一段红。
#[test]
fn an_explicit_gh_proxy_off_wins_over_a_mirror_first_region() {
    use crate::download::config::GhProxyOverride;

    let url = "https://github.com/o/r/releases/download/v1/a.jar";
    let sources = |tune: &DownloadTuning| -> Vec<String> {
        let profile = tune.apply_source_selection(RegionProfile::direct(0));
        MirrorTable::builtin()
            .expand(url, ResourceKind::GitHubRelease, &profile)
            .into_iter()
            .map(|candidate| candidate.source)
            .collect()
    };

    // 基线：`region=cn` 让档案变镜像优先，GitHub 资源因此套上内置前缀。
    let mut tune = DownloadTuning::default().with_source_selection(Some("cn"), None);
    assert_eq!(sources(&tune), ["gh_proxy", "official"]);

    // 同一次设置里再写 `off`：它最后叠 ⇒ 前缀被清掉，候选链只剩官方。
    tune.gh_proxy = GhProxyOverride::Off;
    let off = tune.apply_source_selection(RegionProfile::direct(0));
    assert_eq!(off.mode, RegionMode::MirrorFirst, "off 不该改「谁在前」");
    assert_eq!(off.gh_proxy, None, "off 必须清掉前缀");
    assert_eq!(sources(&tune), ["official"]);
}
