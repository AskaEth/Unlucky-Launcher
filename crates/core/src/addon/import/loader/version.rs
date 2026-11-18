//! 两道版本一致性闸的**判据**：装回来的与清单声明的是不是**同一个装载器版本**（M332）。
//!
//! # 为什么不能逐字比（P36）
//!
//! 老版 Forge 里**一部分 build** 的 maven 版本串带**双后缀**：`1.7.10-10.13.4.1614-1.7.10`
//! （末尾再写一次原版版本）。清单里写的却是短形（CF 是 `modLoaders[].id = "forge-10.13.4.1614"`，
//! HMCL / PCL 读 CF 清单时都把 `forge-` 后面那段原样当版本号），于是
//! `done.version_id != target.version_id` 会把**同一次发布**判成「装错了版本」。
//!
//! # 判据：落到同一批 maven 坐标上
//!
//! 唯一的事实来源是 [`maven::maven_version_candidates`]（「短形 → 补原版前缀」「Forge 双后缀
//! → 末尾再补一次原版」这两条规则本来就写在 loader 侧，这里**不写第二套**）：两个版本串只要有
//! **一条共同候选**就是同一次发布。
//!
//! ## 哪条候选是承重的（**别按「老版一律双后缀」去简化**）
//!
//! `GET https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml`
//! （2026-11-15 实测，5106 条 `<version>`）：
//!
//! | 原版 | 条数 | 只有双后缀坐标 | 单后缀坐标 |
//! |---|---|---|---|
//! | 1.7.10 | 163 | **68** | 95 |
//! | 1.8.9 | 114 | **3**（11.15.1.2318 / 1902 / 1890） | 111 |
//! | 1.12.2 | 355 | 0 | 355 |
//! | 1.20.1 | 138 | 0 | 138 |
//!
//! 双后缀既不是「老版 Forge 的统一形状」，也不是「越新越双」：1.7.10 的 68 条从 10.13.2.1300
//! 就开始了，而单后缀一直延续到 10.13.3.1401-1710ls；1.8.9 则是最后三个 build 才双。
//! 所以候选表里**第一条（单后缀）的承重面大得多**：它是 699 条真实坐标（1.7.10 的 95 +
//! 1.8.9 的 111 + 1.12.2/1.20.1 的 493）的唯一拼法；第二条（双后缀兜底）只服务 71 条
//! **只有**双后缀的 build —— P36 的 `10.13.4.1614` 正是其中之一（它的单后缀坐标在 maven 上
//! 是 404）。把候选「简化」成只拼双后缀，那 699 条会立刻 404。
//!
//! | 判为同一个 | 共同候选 | 为什么会有两种写法 |
//! |---|---|---|
//! | `10.13.4.1614` ⇔ `10.13.4.1614-1.7.10` | `1.7.10-10.13.4.1614-1.7.10` | 清单写短形，maven 上是双后缀 |
//! | `10.13.4.1614` ⇔ `1.7.10-10.13.4.1614` | `1.7.10-10.13.4.1614` | 一边带原版前缀 |
//! | `47.2.0` ⇔ `1.20.1-47.2.0` | `1.20.1-47.2.0` | CF 的 `id` 与 maven 坐标 |
//! | `21.1.80` ⇔ `21.1.80` | `21.1.80` | NeoForge / Fabric 的版本号没有别的写法 |
//!
//! | 仍然拒 | 为什么 |
//! |---|---|
//! | `10.13.4.1614` ⇎ `10.13.4.1615` | 两条候选互不相交（差一个 build 就是另一次发布） |
//! | `1.7.10-forge-10.13.4.1614` ⇎ `1.8.9-forge-10.13.4.1614` | 版本 id 的原版段不同 |
//! | `1.7.10-forge-…` ⇎ `1.7.10-neoforge-…` | 加载器种类不同 |
//! | `1.20.1-fabric-0.15.11` ⇎ `1.20.1` | 纯原版 id 不是"另一个写法的 Fabric" |
//!
//! # 为什么不把闸放宽成「能归一就放行」
//!
//! 归一只是**换写法**，不是**换版本**：交集为空仍然拒（用例 `a_different_build_is_still_refused`
//! 钉着这条）。版本 id 只在两边都是 `<原版>-<种类>-<装载器版本>` 形状时才拆段比较，形状对不上就
//! **只认逐字相等**——看不懂的字符串不许当成"同一个版本"。

use crate::addon::loader::{maven, LoaderKind, LoaderRelease};
use crate::game::layout::GameLayout;

use super::super::error::ImportError;
use super::super::report::LoaderTarget;
use super::LoaderInstallError;

/// 闸①（解析期）：官方元数据解析回来的**必须**与清单声明的是同一个装载器版本。
///
/// 元数据上找不到"清单要的那个版本"时唯一的正确行为是报错，不是取最近的。
///
/// # Errors
///
/// 版本串或版本 id 归一后仍不是同一个装载器版本时返回 [`LoaderInstallError::Failed`]。
pub(super) fn check_resolved(
    release: &LoaderRelease,
    target: &LoaderTarget,
) -> Result<(), LoaderInstallError> {
    let same = same_loader_version(
        target.kind,
        &target.mc_version,
        &release.loader_version,
        &target.loader_version,
    ) && same_loader_id(
        target.kind,
        &target.mc_version,
        &release.version_id,
        &target.version_id,
    );
    if same {
        return Ok(());
    }
    Err(LoaderInstallError::failed(format!(
        "官方元数据给回来的是 {} {}（版本 id {}），与清单声明的 {} {}（版本 id {}）不是同一个装载器版本",
        release.kind.label(),
        release.loader_version,
        release.version_id,
        target.kind.label(),
        target.loader_version,
        target.version_id
    ))
    .with_hint(
        "装载器拒绝拿别的版本顶替：确认这个整合包要的加载器版本（CF 是 `modLoaders[].id`、\
         Modrinth 是 `dependencies` 的值），或者换一份与它匹配的整合包",
    ))
}

/// 闸②（复核期）：实现方装回来的**必须**与清单声明的是同一个装载器版本。
///
/// 编排层再核一次，是因为"找不到就用最近的"最容易藏在实现方里——core 这一侧宁可整次导入
/// 失败，也不许把另一个版本当成清单要求的版本。
///
/// # Errors
///
/// 装回来的版本 id 归一后仍不是同一个装载器版本时返回 `import_loader`（带"删掉 versions/… 后
/// 重试"的出路提示）。
pub(super) fn check_installed(
    target: &LoaderTarget,
    installed_id: &str,
) -> Result<(), ImportError> {
    if same_loader_id(
        target.kind,
        &target.mc_version,
        installed_id,
        &target.version_id,
    ) {
        return Ok(());
    }
    Err(ImportError::loader(format!(
        "装载器装回来的是 {installed_id}，与清单声明的 {} 不一致（按 maven 版本归一后仍不是同一个装载器版本）：拒绝把别的版本当成本次导入的结果",
        target.version_id
    ))
    .with_hint(format!(
        "删掉 versions/{installed_id} 后重试；若官方 meta 上确实没有这个版本，换一个该整合包支持的加载器版本"
    )))
}

/// **落盘 id 钉到清单声明的那个写法**（闸已判过是同一个装载器版本）。
///
/// 为什么必须钉：实例登记用的是清单的版本 id（`ImportPlan::version_id`），所以
/// `versions/<id>/<id>.json` 必须就落在那个 id 下——否则导入"成功"、点启动读不到版本 JSON，
/// 正是本模块要消灭的假功能。1.20.1 / Fabric / NeoForge 两边写法本来就相同，这里什么都不做。
///
/// 真的换过写法时往 `release.warnings` 里补一条（用户看得见"官方 maven 上叫另一个名字"），
/// 这条告警随后经 loader 计划铺进导入报告。
pub(super) fn adopt_declared_id(release: &mut LoaderRelease, target: &LoaderTarget) {
    if release.version_id == target.version_id && release.loader_version == target.loader_version {
        return;
    }
    let official = std::mem::replace(&mut release.loader_version, target.loader_version.clone());
    release.version_id.clone_from(&target.version_id);
    release.version_json.id = Some(target.version_id.clone());
    release.warnings.push(format!(
        "官方元数据上的 maven 版本串是 {official}（清单声明 {}）：同一次发布，按清单的版本 id {} 落盘",
        target.loader_version, target.version_id
    ));
}

/// 闸③（**落盘判据**）：清单声明的那个 id 下必须**真的有版本 JSON**。
///
/// 为什么单靠闸② 不够：闸② 放行「同一个装载器版本、另一种写法」，而管线登记用的是清单的 id
/// （`ImportPlan::version_id`）。如果实现方把版本 JSON 落在另一种写法下，导入会"成功"、实例却
/// 指向一个读不到清单的 id —— 正是本模块要消灭的假功能。生产实现会把落盘 id 钉成清单写法
/// （[`adopt_declared_id`]，`engine` 在组计划**之前**调用），所以这条只在实现方没钉 id 时红。
///
/// # Errors
///
/// 声明 id 下没有 `versions/<id>/<id>.json` 时返回 `import_loader`（带"删掉 versions/… 后重试"
/// 的出路提示，与闸② 同一出口）。
pub(super) fn check_landed(
    layout: &GameLayout,
    target: &LoaderTarget,
    installed_id: &str,
) -> Result<(), ImportError> {
    if layout.is_version_installed(&target.version_id) {
        return Ok(());
    }
    let spelling = if installed_id == target.version_id {
        ""
    } else {
        "（与清单声明的是同一个装载器版本，只是 maven 写法不同）"
    };
    Err(ImportError::loader(format!(
        "装载器报回来的是 {installed_id}，但 versions/{}/ 里没有版本 JSON{spelling}：实例会被登记成 {}、启动时读不到它",
        target.version_id, target.version_id
    ))
    .with_hint(format!(
        "删掉 versions/{installed_id} 后重试；若反复出现，这是落盘 id 与清单 id 不一致（不是版本不符）"
    )))
}

/// 两个**装载器版本串**是否指同一个装载器版本（同一原版下、同一次 maven 发布）。
///
/// # 拒与不拒的分界线
///
/// 逐字相等恒为「是」；否则看 [`maven::maven_version_candidates`] 的候选集是否有交集
/// （交集规则见模块文档的表）。非 maven 族（Fabric / Quilt / LiteLoader）的版本号没有别的
/// 写法，候选集就是它自己，因此只有逐字相等才放行。
pub(super) fn same_loader_version(
    kind: LoaderKind,
    mc_version: &str,
    left: &str,
    right: &str,
) -> bool {
    let (left, right) = (left.trim(), right.trim());
    if left == right {
        return true;
    }
    let left = maven::maven_version_candidates(kind, mc_version, left);
    let right = maven::maven_version_candidates(kind, mc_version, right);
    left.iter().any(|candidate| right.contains(candidate))
}

/// 两个**版本 id**（`<原版>-<种类>-<装载器版本>`）是否指同一个装载器版本。
///
/// 任一边不是这个形状（纯原版 id、别的加载器的 id、清单里写的怪串）时只认逐字相等：
/// 拆不出装载器版本段就不做归一，闸不放宽到"看不懂就放行"。
pub(super) fn same_loader_id(kind: LoaderKind, mc_version: &str, left: &str, right: &str) -> bool {
    let (left, right) = (left.trim(), right.trim());
    if left == right {
        return true;
    }
    match (
        loader_segment(kind, mc_version, left),
        loader_segment(kind, mc_version, right),
    ) {
        (Some(left), Some(right)) => same_loader_version(kind, mc_version, left, right),
        _ => false,
    }
}

/// `version_id` 里的装载器版本段；不是 `<原版>-<种类>-<非空>` 时给 `None`。
fn loader_segment<'a>(kind: LoaderKind, mc_version: &str, version_id: &'a str) -> Option<&'a str> {
    let rest = version_id.strip_prefix(mc_version)?.strip_prefix('-')?;
    let rest = rest.strip_prefix(kind.as_str())?.strip_prefix('-')?;
    (!rest.is_empty()).then_some(rest)
}
