//! 探测诊断（M113）：「这个包**看起来像** X，但**缺** Y」。
//!
//! ## 为什么由 host 判，不引 core 改动
//!
//! core 的 `PackRegistry::detect` 只回答「谁认领」，一个都不认领时只给一句
//! `pack_unsupported` + 「试过哪些格式」。而用户手里那个包往往**差一点点**就是某个已知格式
//! （CF 导出漏了 `manifest.json`、mrpack 被浏览器改名成 `.zip` 后解压丢了 index）。
//! 判断「像哪个」要看包内到底有哪些标记文件，这是宿主层拿着 `PackArchive` 才看得见的事。
//!
//! ## 判据只有标记文件的有无（不看文件名）
//!
//! | 格式 | 标记 | 邻居（有它但没标记 ⇒ 「像它」） |
//! |---|---|---|
//! | CurseForge | `manifest.json` | `overrides/` · `mods/` |
//! | Modrinth | `modrinth.index.json` | `overrides/` |
//! | MultiMC / Prism | `mmc-pack.json` | `instance.cfg` · `.minecraft/` |
//!
//! `saw` 只留前 5 条有代表性的标记：长路径往界面里倒会把版面撑破（塔台复审提过）。
//! 一个标记都没认出来时 `saw` 为空，由界面显示「nothing recognisable」而不是一行空白。

use std::path::Path;

use unlucky_core::addon::pack::{PackArchive, PackLimits, PackRegistry};

use crate::dto::{PackFormatView, ProbeView};

/// 诊断一个包（`None` = 有个格式认领它，正常走管线）。
pub(super) fn of(pack: &Path) -> Option<ProbeView> {
    let archive = match PackArchive::open(pack, PackLimits::default()) {
        Ok(archive) => archive,
        Err(error) => {
            // 连 zip 都打不开（选错文件 / 下了一半）：也要说清楚是哪一步失败的
            return Some(ProbeView {
                looks_like: None,
                missing: Vec::new(),
                saw: Vec::new(),
                detail: format!("could not open the archive: {error}"),
            });
        }
    };
    let registry = PackRegistry::builtin();
    if registry.detect(&archive).is_ok() {
        return None;
    }

    // (格式, 标记文件, 邻居标记)
    let rules: [(PackFormatView, &str, &[&str]); 3] = [
        (
            PackFormatView::Curseforge,
            "manifest.json",
            &["overrides/", "mods/"],
        ),
        (
            PackFormatView::Modrinth,
            "modrinth.index.json",
            &["overrides/"],
        ),
        (
            PackFormatView::Multimc,
            "mmc-pack.json",
            &["instance.cfg", ".minecraft/"],
        ),
    ];

    // 目录输入的条目名**没有尾斜杠**（`overrides` 而不是 `overrides/`），zip 里目录条目也可能缺；
    // 所以匹配同时认「精确 / 前缀 / 去掉尾斜杠」三种写法，判据仍然只有标记文件的有无。
    let names: Vec<&str> = archive.entries().iter().map(|entry| entry.name()).collect();
    let has = |needle: &str| {
        names.iter().any(|name| {
            *name == needle || name.starts_with(needle) || *name == needle.trim_end_matches('/')
        })
    };

    let mut saw: Vec<String> = Vec::new();
    let mut best: Option<(PackFormatView, String)> = None;
    for (format, marker, neighbours) in rules {
        let hits: Vec<&str> = neighbours
            .iter()
            .copied()
            .filter(|entry| has(entry))
            .collect();
        if hits.is_empty() {
            continue;
        }
        if !has(marker) {
            // 有邻居、没标记 ⇒ 「像它，但缺那个标记」
            saw.extend(hits.iter().map(|entry| (*entry).to_string()));
            if best.is_none() {
                best = Some((format, marker.to_string()));
            }
        }
    }
    saw.sort();
    saw.dedup();
    saw.truncate(5);

    let detail = format!("tried: {}", registry.ids().join(" / "));
    Some(ProbeView {
        looks_like: best.as_ref().map(|(format, _)| *format),
        missing: best.map(|(_, marker)| vec![marker]).unwrap_or_default(),
        saw,
        detail,
    })
}
