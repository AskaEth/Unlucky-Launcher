//! 整合包导入的**落点派生**（M216 ②）：`<data>/instances/<实例目录名>` + 名字归一化。
//!
//! 为什么在 core：`ull pack import` 与 GUI 导入页必须落到**同一个**目录、用**同一份**名字
//! 归一化。M215 survey 的债②就是两边不同源——CLI 缺省指向共享 `.minecraft` 根
//! （`cli/commands/pack.rs`），GUI 指向 `<data>/instances/<dir_name(包名)>`
//! （`presenter/host/addon/pack/import.rs::target_of`）。归一化留在 presenter 就只有 GUI 能看，
//! CLI 复制一份就是第三处真相源；放这里，两侧各一行委托。
//!
//! 归 GUI 侧独有的部分**不在**这里：选中的实例 → 其运行目录（要读隔离档与实例设置，
//! `pack.rs::instance_root`），那是「用已有实例」而不是「建新实例目录」。

use std::path::{Path, PathBuf};

use super::instance::{self, InstanceConfig, Isolation};
use crate::game::error::GameError;
use crate::platform::layout::LauncherLayout;

/// 包文件名 → 实例候选名（`Better MC.mrpack` → `Better MC`）。
///
/// 取不到 `file_stem`（路径以 `..` 结尾之类）时给 `imported-pack`：宁可名字难看，
/// 也不要一个空目录名。
#[must_use]
pub fn pack_stem(pack: &Path) -> String {
    pack.file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_else(|| "imported-pack".to_string())
}

/// **实例目录名**：ASCII 化 + 必要时补一段内容哈希。
///
/// 判据：名字里**只含** ASCII 字母数字、空格、`-`、`_` 时保持原样（空格折成 `-`，
/// `Better MC` → `Better-MC`）；**一旦折过别的字符**（中文、`.`、括号……）就补 `-<hash8>`，
/// 用原名的哈希保住区分度——否则「我的整合包」「另一个整合包」都会得到 `pack`，
/// 第二个中文名导入直接撞「目录已存在」。
#[must_use]
pub fn instance_dir_name(name: &str) -> String {
    let base = slug(name);
    let faithful = !name.is_empty()
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == ' ');
    if faithful {
        base
    } else {
        format!("{base}-{}", hash8(name))
    }
}

/// 新实例的内容目录：`<data>/instances/<实例目录名>`（**只算路径，不建目录**）。
///
/// 调用方自己决定要不要建（CLI 的预检与 GUI 的 `prepare_target` 都要先看路径）。
#[must_use]
pub fn instance_dir(layout: &LauncherLayout, name: &str) -> PathBuf {
    layout
        .data()
        .join("instances")
        .join(instance_dir_name(name))
}

/// 导入一次的**落点与登记名**：显式目标优先，否则 `<data>/instances/<实例目录名>`。
///
/// 名字 = `name`（去空格，空串不算）或包文件名；**目录名与登记名用同一个值**（GUI 同）。
/// `Err` = 缺省目录已存在且非空的人读原因：空目录（上次失败的残留）可以复用，否则一次
/// 失败之后永远重试不了；非空也不猜「你想合并进已有实例」，让用户显式给目标。
///
/// GUI 导入页（`import.rs::target_of`）是同一判据的另一份实现，外加一条「用已有实例」分支
/// （要读隔离档与实例设置）——那条分支只有 GUI 有，所以落点判定留了这道公共函数给 CLI。
///
/// # Errors
///
/// 缺省目录非空时返回原因字符串。
pub fn import_target(
    layout: &LauncherLayout,
    pack: &Path,
    target: Option<&Path>,
    name: Option<&str>,
) -> Result<(PathBuf, String), String> {
    let name = name
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| pack_stem(pack));
    let Some(target) = target else {
        let dir = instance_dir(layout, &name);
        let occupied = std::fs::read_dir(&dir).is_ok_and(|mut entries| entries.next().is_some());
        if occupied {
            return Err(format!("目标实例目录已存在且非空：{}", dir.display()));
        }
        return Ok((dir, name));
    };
    Ok((target.to_path_buf(), name))
}

/// 把「导入建出来的实例」绑到它的内容目录（`isolation=custom` + `game_dir`）。
///
/// # 为什么必须绑
///
/// 运行目录不是「导入目录」，而是从 `config/instances/<id>.json` 的 `isolation` 派生的
/// （[`derive_game_dir`](crate::game::isolation::derive_game_dir)）：缺省档
/// `instances.mode=isolated` 给成 `<game>/versions/<versionId>`，`shared` 给成游戏根。
/// 导入目标 `<data>/instances/<名>` **不在这条链上** ⇒ 不绑的话「导入成功」而那批
/// mods/config 永远不会被那条实例的启动用到（M113 起 GUI 就有这个洞）。
///
/// # 绑成什么、不绑成什么
///
/// - 运行目录 = `dir`：`saves/` `mods/` `logs/` `config/` 落在这里（与别的启动器隔离）；
/// - **`versions/` `libraries/` `assets/` 仍按 D20 恒共享**在游戏根 —— 不是「完全隔离」，
///   版本本体只装一份，别人 `.minecraft` 里已下好的库也照旧复用。
///
/// # 回滚
///
/// 把 `config/instances/<id>.json` 的 `isolation` 删掉（或写成 `"inherit"`）即回到「跟随全局
/// 档」；`gameDir` 随之失效。也可以整份删掉这一份实例设置文件（`register` 会重新补最小那份）。
/// **不会**搬动任何已落盘的内容。
///
/// # Errors
///
/// 读/写实例设置失败（JSON 坏、id 不合法、原子写失败）。
pub fn bind_instance_dir(
    layout: &LauncherLayout,
    instance_id: &str,
    version_id: &str,
    dir: &Path,
) -> Result<(), GameError> {
    let mut config = instance::read_instance_config(layout, instance_id)?
        .unwrap_or_else(|| InstanceConfig::new(instance_id, version_id));
    config.isolation = Some(Isolation::Custom);
    config.game_dir = Some(dir.to_path_buf());
    instance::write_instance_config(layout, &config)?;
    Ok(())
}

/// 导入收尾：把「我们新建出来的实例」绑到它的内容目录，返回一句给人读的结论。
///
/// 返回 `None` = 不需要绑；`Some(Ok(说明))` = 已绑；`Some(Err(说明))` = 绑失败（档案在、内容在，
/// 只是运行目录没指过来——调用方**别**把它当整次导入失败，但要让人看见）。
///
/// 判据（CLI 与 GUI 共用一份，别自己再写一遍）：
/// - `auto_bound == false` ⇒ 不绑（用户显式给了目标目录 / `--no-register`：那目录可能是别人的
///   运行目录，也可能被用户自己指过）；
/// - 没登记、或**复用了已有档案**（`registered.created == false`）⇒ 不绑；
/// - 其余 ⇒ [`bind_instance_dir`]。
///
/// 参数直接吃 `addon::import::ImportReport`：`addon` 早就在调 `game::install::register`，
/// 两者同属 core 内部；再投影一层 `struct` 只会让两侧各多写六行构造。
#[must_use]
pub fn bind_created_instance(
    layout: &LauncherLayout,
    target: &Path,
    auto_bound: bool,
    report: &crate::addon::import::ImportReport,
) -> Option<Result<String, String>> {
    if !auto_bound {
        return None;
    }
    let registered = report.registered.as_ref().filter(|item| item.created)?;
    Some(
        match bind_instance_dir(layout, &registered.id, &registered.version_id, target) {
            Ok(()) => Ok(format!(
                "运行目录已指向 {}（实例 {}）",
                target.display(),
                registered.name
            )),
            // 内容与档案都已就位，只是目录没指过来 ⇒ 不把整次导入判失败，但必须说出来。
            Err(error) => Err(format!(
                "实例已登记但没能绑到导入目录（{error}）：启动前请把该实例的运行目录指到 {}",
                target.display()
            )),
        },
    )
}

/// 目录/文件名用的 slug（只留 ASCII 字母数字与 `-` / `_`，其余折成 `-`）。
#[must_use]
pub fn slug(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "pack".to_string()
    } else {
        trimmed
    }
}

/// FNV-1a 64 → 8 位十六进制（只要稳定、短、不需要密码学强度）。
fn hash8(text: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{:08x}", hash as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 抓得住：目录名归一化的四条既有判据（ASCII 保真、空格折 `-`、中文补哈希、点号补哈希），
    /// 以及 `<data>/instances/<名>` 这个落点本身。
    /// 抓不住：GUI 是否真的改调这个函数（那条由 presenter 侧的既有用例继续钉着）。
    #[test]
    fn the_instance_dir_is_derived_the_same_way_the_gui_used_to() {
        assert_eq!(
            pack_stem(Path::new("C:/packs/Better MC.mrpack")),
            "Better MC"
        );
        assert_eq!(pack_stem(Path::new("packs/unnamed")), "unnamed");
        assert_eq!(instance_dir_name("Better MC"), "Better-MC");
        assert_eq!(instance_dir_name("Better-MC"), "Better-MC");
        assert_eq!(instance_dir_name("pack"), "pack");
        // 折过字符 ⇒ 补哈希保住区分度（两个中文名不许塌成同一个目录）。
        let a = instance_dir_name("我的整合包");
        let b = instance_dir_name("另一个整合包");
        assert_ne!(a, b);
        assert!(
            a.starts_with("pack-") && a.len() == "pack-".len() + 8,
            "{a}"
        );
        assert!(instance_dir_name("Survival 1.20.1").starts_with("Survival-1-20-1-"));
        // 落点：`<data>/instances/<目录名>`，不是共享的 `.minecraft` 根。
        let layout = LauncherLayout::new(PathBuf::from("/data/.ULL"));
        assert_eq!(
            instance_dir(&layout, "Better MC"),
            PathBuf::from("/data/.ULL/data/instances/Better-MC")
        );
    }

    /// **M216 ②**：缺省落点与 GUI 同源（`<data>/instances/<名>`），**不是共享的 `.minecraft` 根**；
    /// 显式目标照给、空目录可复用、非空才报错。
    ///
    /// 抓得住：缺省改回 `game_home` 根（第一条断言会变成 `…/ .minecraft`）、目录名不归一化、
    /// 非空目录不复检、登记名与目录名分叉。抓不住：CLI/GUI 是否真调用本函数（编译期无约束，
    /// 靠两侧各自的调用点用例），以及真实 `Ctx::layout()` / `HostEnv` 的取值。
    #[test]
    fn the_default_target_is_an_instance_dir_not_the_shared_minecraft_root() {
        let root = std::env::temp_dir().join(format!("ull-m216-pack-{}", std::process::id()));
        let layout = LauncherLayout::new(root.join(".ULL"));
        let pack = Path::new("C:/packs/Better MC.mrpack");
        let target =
            |target: Option<&Path>, name: Option<&str>| import_target(&layout, pack, target, name);

        let (dir, name) = target(None, None).expect("缺省可用");
        assert_eq!(dir, root.join(".ULL/data/instances/Better-MC"));
        assert!(
            !dir.starts_with(root.join(".minecraft")),
            "不许落到共享游戏根"
        );
        assert_eq!(name, "Better MC", "登记名与目录名同源");

        std::fs::create_dir_all(&dir).expect("建目录");
        assert_eq!(
            target(None, None).expect("空目录可复用").0,
            dir,
            "空目录可复用"
        );
        std::fs::write(dir.join("mmc-pack.json"), b"{}").expect("写文件");
        let reason = target(None, None).expect_err("非空要报错");
        assert!(reason.contains("已存在且非空"), "{reason}");

        let explicit = root.join("my-target");
        let (dir, name) = target(Some(&explicit), Some("x")).expect("显式目标照给");
        assert_eq!((dir, name.as_str()), (explicit, "x"));
        std::fs::remove_dir_all(&root).ok();
    }

    /// **§5 正向**：导入建出来的实例必须绑到内容目录，否则「导入成功」而内容永远不会被启动用到。
    ///
    /// 抓得住：`bind_instance_dir` 没写 `isolation=custom` / `game_dir` 落空 / 写错了实例（读回来
    /// 仍是 `None`）。抓不住：`derive_game_dir` 真的把运行目录算成那个目录——那条由
    /// `isolation.rs` 的既有用例钉住，这里不重算（tower 要求不动 `derive_game_dir`）。
    #[test]
    fn binding_an_imported_instance_points_it_at_the_import_dir() {
        let root = std::env::temp_dir().join(format!("ull216-bind-{}", std::process::id()));
        let layout = LauncherLayout::new(root.join(".ULL"));
        let registration =
            crate::game::install::instance::register(&layout, "pack", "1.20.1", None)
                .expect("登记");
        let id = registration.profile.id.as_str().to_string();
        let dir = instance_dir(&layout, "pack");

        bind_instance_dir(&layout, &id, "1.20.1", &dir).expect("绑定");
        let config = crate::game::install::instance::read_instance_config(&layout, &id)
            .expect("读设置")
            .expect("有设置文件");
        assert_eq!(config.isolation, Some(Isolation::Custom));
        assert_eq!(config.game_dir.as_deref(), Some(dir.as_path()));
        std::fs::remove_dir_all(&root).ok();
    }

    /// **§5 反向**：没走绑定那条路的实例不许自己冒出 `custom`（否则上一条抓不住东西）。
    #[test]
    fn a_freshly_registered_instance_still_inherits_the_global_mode() {
        let root = std::env::temp_dir().join(format!("ull216-unbound-{}", std::process::id()));
        let layout = LauncherLayout::new(root.join(".ULL"));
        let registration =
            crate::game::install::instance::register(&layout, "plain", "1.20.1", None)
                .expect("登记");
        let config = crate::game::install::instance::read_instance_config(
            &layout,
            registration.profile.id.as_str(),
        )
        .expect("读设置")
        .expect("有设置文件");
        assert_eq!(config.isolation, None, "缺省必须跟随全局档");
        assert_eq!(config.game_dir, None);
        std::fs::remove_dir_all(&root).ok();
    }
}
