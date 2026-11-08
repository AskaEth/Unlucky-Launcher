//! 安装侧词表与 `crate::game` 词表的对接：委派有没有接错、别名有没有漏进产出。
//!
//! 词表本体（七个对外 id）在 `crate::game::loader`，这里只验**安装侧这一半**：
//! [`LoaderKind::as_str`] 委派给 core，[`LoaderKind::parse`] 委派 core 的解析并只多收一个
//! Modrinth 输入别名。为什么放在安装侧的测试目录：这一半的存在理由是「装出来的版本 id 得
//! 能被认回来」，那正好是生成端与解析端的接缝。

use crate::addon::loader::LoaderKind;
use crate::game::{InstanceLoader, LoaderKind as Wide};

/// `as_str` 是 core 词表的委派：四家装得上的一一对应，且绝不产出别名。
///
/// 抓得住：委派接错支（`Fabric` 映到 `Quilt`）；有人把 `as_str` 改回手写字面量并写错；
/// 有人让 `as_str` 产出 `neo-forge`（别名只进不出）；生成端 `<mc>-<kind>-<loader>` 与解析端
/// 脱节（装出来的 id 认不回同一家）。
/// 抓不住：core 词表的字面值（那是 `game::loader` 的往返测试与 presenter 契约测试的职责）。
#[test]
fn install_side_ids_are_delegated_to_the_core_vocabulary() {
    let cases = [
        (LoaderKind::Fabric, Wide::Fabric),
        (LoaderKind::Quilt, Wide::Quilt),
        (LoaderKind::NeoForge, Wide::NeoForge),
        (LoaderKind::Forge, Wide::Forge),
    ];
    for (install, wide) in cases {
        assert_eq!(
            install.as_str(),
            wide.as_str(),
            "{install:?} 的 id 没走 core 词表"
        );
        assert_ne!(install.as_str(), "neo-forge", "别名只进不出");
        assert_eq!(LoaderKind::parse(install.as_str()), Some(install), "反查");

        let id = install.version_id("1.20.4", "0.16.5");
        let parsed = InstanceLoader::parse(&id);
        assert_eq!(parsed.kind.as_str(), install.as_str(), "`{id}` 认错了家");
        assert_eq!(parsed.version, "0.16.5", "`{id}` 的版本段");
    }
}

/// 别名策略：`neo-forge` 只作为**输入**被接受（Modrinth 的拼法），产出永远是 `neoforge`。
///
/// 抓得住：别名被搬进词表（`as_str` 开始产出 `neo-forge`）；别名只认小写；安装侧顺手接受了
/// 本启动器装不了的几家（原版 / OptiFine / LiteLoader）。
/// 抓不住：`neo-forge` 之外 Modrinth 以后可能再发明的拼法——没有样本可写。
#[test]
fn modrinth_alias_is_input_only() {
    for raw in ["neo-forge", "NEO-FORGE", " neo-forge "] {
        assert_eq!(
            LoaderKind::parse(raw),
            Some(LoaderKind::NeoForge),
            "`{raw}`"
        );
    }
    assert_eq!(
        LoaderKind::NeoForge.as_str(),
        "neoforge",
        "产出不许带连字符"
    );
    for raw in ["vanilla", "optifine", "liteloader", "", "rift"] {
        assert_eq!(LoaderKind::parse(raw), None, "`{raw}` 不该被安装侧接受");
    }
}

/// [`LoaderKind::all`] 是安装侧唯一还手写的清单（CF id 前缀匹配、CLI `--loader` 提示用），
/// 它与 [`LoaderKind::from_core`] 认可的集合必须一致——不重抄字面量，直接对映射本身。
///
/// 抓得住：往 core 加一家**能装**的加载器，`from_core` 补了却忘了 `all()`（CLI 提示会漏一家、
/// CF id 不认识它的前缀）；反过来 `all()` 里多一家而 `from_core` 说装不了。
/// 抓不住：`all()` 的顺序（那是 CLI 提示的展示序，没有对外契约意义）。
#[test]
fn installable_list_matches_the_from_core_mapping() {
    let mut derived: Vec<LoaderKind> = Wide::ALL
        .into_iter()
        .filter_map(LoaderKind::from_core)
        .collect();
    derived.sort_unstable();
    let mut listed = LoaderKind::all().to_vec();
    listed.sort_unstable();
    assert_eq!(
        listed, derived,
        "`LoaderKind::all()` 必须正好是 `from_core` 认可的那几家"
    );
}
