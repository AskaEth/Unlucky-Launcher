//! `instances list`：实例与已安装版本的总览。
//!
//! 安装/登记在 [`super::install`]；这里只读（`profiles.json` + `versions/` 的存在性）。
//! 读 `profiles.json` 的实现委托给 `core::game::install::instance`，避免 CLI 自带一份
//! 解析逻辑（同一份文件两处解析迟早会漂移）。
//!
//! # `--json` 里的 `loader` 字段（M124）
//!
//! 每一行实例带一个 `loader` 对象：`{"kind": <稳定 id>, "version": <版本段>}`。它是
//! **`version_id` 的解析结果**，解析本身不在这里——[`unlucky_core::game::InstanceLoader::parse`]
//! 是全仓唯一的 id → 加载器入口（M122 收敛）。种类 → 稳定 id 的词表也不在本 crate 的代码里：
//! [`unlucky_core::game::LoaderKind::as_str`] 是生产代码里 id 的**源头表**（M126 收敛），这里
//! 直接调它，**连翻译层都没有**——词表与 id 匹配一样，两处各算一遍就会漂（M120 的「刚装的
//! Fabric 显示成原版」）。生产代码里唯一的逐字副本是 `cli::help` 里 `ull help instances` 的
//! 词表行（会打印给用户），由本模块的 `help_vocabulary_tests::help_lists_every_stable_id`
//! 按 `ALL` 遍历守着。
//!
//! ## 约定一：`loader` 恒存在，原版是 `{"kind":"vanilla","version":""}`
//!
//! 原版实例**不会**缺这个键，也**不会**是 `null`——脚本不必先判空，`kind` 恒是七个闭集值
//! 之一（`vanilla` / `fabric` / `quilt` / `forge` / `neoforge` / `optifine` / `liteloader`）。
//! 与 `version` 的关系：原版与「id 里没写加载器版本」的两家第三方（`1.12.2-LiteLoader`）
//! 拿到空串，不是缺失。
//!
//! ## 约定二：`kind` 是**识别**口径，不是**安装**口径
//!
//! 七个值里 `optifine` / `liteloader` 只出现在「本启动器**认得出**但**装不了**」的情形：
//! 它们只能由别的启动器（PCL / HMCL）装好后继承进来，`ull install --loader` 的五个取值
//! （`vanilla` / `fabric` / `quilt` / `neoforge` / `forge`，见 [`crate::cli::model`]）
//! 里没有它们。看到 `loader.kind == "optifine"` 的意思是「这个实例的加载器是 OptiFine」，
//! **不是**「ull 能给你装 OptiFine」。
//!
//! ## 约定三：`vanilla` 的含义是「id 里没有加载器痕迹」
//!
//! [`unlucky_core::game::InstanceLoader::parse`] 只看版本 id 的字符串。把加载器信息只写进
//! 版本 JSON、id 里一点痕迹都没有的实例（整包导入时被重命名成 `我的整合包` 那种）会**诚实地**
//! 返回 `vanilla`——这是「认不出」，不等于「一定没装加载器」。要权威结论得读 `libraries[]`，
//! 不在本命令的能力范围内。
//!
//! ## 向后兼容
//!
//! 这次只**加**一个键：`data.instances[]` 的其余七个键（`id` / `name` / `version_id` /
//! `version_present` / `game_dir` / `java_path` / `last_played_at`）一个没动，人类可读输出
//! 逐字不变（`loader` 只进 JSON）。`--json` 是事实上的对外契约，字段说明同步在
//! `ull help instances`（[`crate::cli::help::topic`]）。

use std::path::PathBuf;

use serde_json::{json, Value};
use unlucky_core::game::install::instance::read_profiles as read_profiles_from_layout;
use unlucky_core::game::{InstanceLoader, ProfileList};

use crate::context::Ctx;
use crate::error::CliError;
use crate::output::Report;

/// `ull instances list`。
///
/// # Errors
///
/// `profiles.json` 读不了或解析失败时返回用户错误。
pub fn list(ctx: &Ctx) -> Result<Report, CliError> {
    let profiles = read_profiles(ctx)?;
    let game = ctx.game();
    let installed = game.installed_versions().unwrap_or_default();

    let mut rows: Vec<Value> = Vec::new();
    let mut body: Vec<String> = Vec::new();
    for profile in &profiles.profiles {
        let present = game.is_version_installed(&profile.version_id);
        body.push(format!(
            "  {}  [{}]  {}  {}",
            profile.name,
            profile.version_id,
            if present { "已安装" } else { "载荷缺失" },
            profile.id.as_str()
        ));
        rows.push(json!({
            "id": profile.id.as_str(),
            "name": profile.name,
            "version_id": profile.version_id,
            "version_present": present,
            "game_dir": option_path(profile.game_dir.as_ref()),
            "java_path": option_path(profile.java_path.as_ref()),
            "last_played_at": profile.last_played_at,
            "loader": loader_json(&profile.version_id),
        }));
    }

    let data = json!({
        "data_root": ctx.data.launcher_home.display().to_string(),
        "game_root": ctx.data.game_home.display().to_string(),
        "instances": rows,
        "installed_versions": installed,
    });

    let mut human = vec![format!(
        "实例 {} 个（数据根 {}）",
        profiles.profiles.len(),
        ctx.data.launcher_home.display()
    )];
    if body.is_empty() {
        human.push("  （空）用 `%prog% install <version>` 登记第一个实例".to_string());
    }
    human.extend(body);
    if installed.is_empty() {
        human.push("已安装版本: （无）".to_string());
    } else {
        human.push(format!("已安装版本: {}", installed.join(", ")));
    }
    Ok(Report::new(data, human))
}

/// 读实例清单（文件不存在 = 空列表，不算错误）。
///
/// # Errors
///
/// 读盘失败或 JSON 非法时返回用户错误。
pub fn read_profiles(ctx: &Ctx) -> Result<ProfileList, CliError> {
    let path = ctx.profiles_config();
    read_profiles_from_layout(&ctx.layout()).map_err(|error| {
        CliError::user(
            "profiles_parse",
            format!("读实例清单 {} 失败：{error}", path.display()),
        )
    })
}

fn option_path(path: Option<&PathBuf>) -> Option<String> {
    path.map(|path| path.display().to_string())
}

/// 版本 id → `loader` 对象（`{"kind": …, "version": …}`）。
///
/// 解析走 [`InstanceLoader::parse`]（全仓唯一入口）；种类 → 稳定 id 走
/// [`unlucky_core::game::LoaderKind::as_str`]（词表的源头，M126）。这里既**不写 id
/// 匹配**，也**不写词表**：词表的源头只在 core（副本见模块文档）。原版同样给出对象
/// （`kind = vanilla`、`version = ""`），不缺键、不为 `null`——见模块文档的「约定一」。
fn loader_json(version_id: &str) -> Value {
    let parsed = InstanceLoader::parse(version_id);
    json!({
        "kind": parsed.kind.as_str(),
        "version": parsed.version,
    })
}

#[cfg(test)]
mod tests;

/// `ull help instances` 的词表行是**对外文档契约**，不许成为第二个说假话的地方（r2）。
///
/// 单列在这里而不是塞进 `instances/tests.rs`：那几个断言钉的是本模块文档承诺的词表，与命令
/// 行为无关（`tests.rs` 的职责是「跑真入口看输出」），而且这样 help 的契约与被承诺的那段
/// 文档就在同一个文件里。
#[cfg(test)]
mod help_vocabulary_tests {
    use unlucky_core::game::LoaderKind;

    /// help 的词表必须覆盖**每一支**稳定 id（含原版与两家只继承、不安装的）。
    ///
    /// 抓得住：help 的词表行被改字或删项（`neoforge` → `neo_forge`）；**新增一家加载器而忘了
    /// 更新 help**——下面按 [`LoaderKind::ALL`] 遍历而不是手抄七项，所以新分支漏写必红。
    /// 抓不住：措辞与排版（只做子串断言）；「kind 是识别口径不是安装口径」那段语义说明是否
    /// 仍然准确，只能靠人读。
    #[test]
    fn help_lists_every_stable_id() {
        let topic = crate::cli::help::topic("instances").expect("help instances 存在");
        for kind in LoaderKind::ALL {
            assert!(
                topic.contains(kind.as_str()),
                "help instances 的词表行漏了 `{}`：\n{topic}",
                kind.as_str()
            );
        }
    }
}
