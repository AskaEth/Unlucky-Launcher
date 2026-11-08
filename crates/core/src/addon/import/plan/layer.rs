//! 覆盖目录的**叠层顺序**与**同路径遮蔽**：导入计划里"逐文件"那一半的判据（纯计算，无 IO）。
//!
//! 拆成单独一个文件有两个原因：①`plan.rs` 要守 400 行；②这一层的两条规则（先按侧排、
//! 再按路径结算胜负）是导入器里唯一"我们自己定的确定性规则"，值得放在一处被单独读完。

use std::collections::HashMap;

use super::super::report::{keys, FileSource, ImportIssue};
use super::PlanOverride;
use crate::addon::pack::{PackManifest, PackOverrides, PackSide};

/// 「这条覆盖条目被同一份包里的另一条声明遮蔽」。
///
/// 为什么不是新的 `FileAction` 变体：`FileAction` / `ConflictReason` / `ConflictChoice` 三个
/// 枚举被 `crates/presenter/src/host/addon/pack/convert.rs` **穷尽 match**（那个 crate 不在
/// `addon` 的作用域里）。所以"遮蔽"这件事走 [`ImportIssue`] 的 note 通道，与
/// `server-overrides/ 被跳过` 完全同一条通道。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shadowed {
    /// 胜出那条的来源侧。
    pub winner: FileSource,
    /// 胜出那条的包内目录名（人读用，例如 `client-overrides`）。
    pub winner_dir: String,
    /// 两条声明的字节是否完全相同（`true` = 遮蔽不丢内容，只是重复了一份）。
    pub same_bytes: bool,
}

/// 覆盖目录按侧排序：`Both`（`overrides/`）先、`Client`（`client-overrides/`）后 = 后者胜。
pub(super) fn ordered(manifest: &PackManifest) -> Vec<&PackOverrides> {
    let mut groups: Vec<&PackOverrides> = manifest.overrides.iter().collect();
    groups.sort_by_key(|group| match group.side {
        PackSide::Both => 0_u8,
        PackSide::Client => 1,
        PackSide::Server => 2,
    });
    groups
}

/// 同一实例路径被本包多份声明命中时：**叠层顺序里最后一条胜**，其余标记为遮蔽并逐条注记。
///
/// # 这是"逐文件"而不是"整包一个策略"的那一步
///
/// research/18 §3.3 的 A 级结论：mrpack 的 `overrides/` 与 `client-overrides/` 之间，
/// **规范层面只有极少数固定路径有优先级**，其余按 zip 条目顺序落盘；HMCL 连顺序都相反。
/// 上一版导入器对此的处理是"按侧排序后逐条写"——结果是对的，但**报告是错的**：同一路径
/// 会出现两条 `Installed`，其中前一条的哈希根本没落地，也没人说得清谁遮蔽了谁。
///
/// 现在把这层算清楚，逐条给出两个事实：
///
/// 1. **赢了的是哪一侧**（叠层顺序 = [`ordered`] 的 `Both → Client`，同组内按路径排序，
///    所以与 zip 条目顺序无关——那是我们自己定死的确定性规则，不是规范）；
/// 2. **两份内容是否逐字节相同**（相同 = 重复声明，遮蔽不丢内容；不同 = 真的换了一份）。
///
/// 判据只看模型里的 `sha1`：M110 扫包时是**现算**的真实内容哈希；手工构造的模型可能撒谎，
/// 但那最多让 `same_bytes` 这句注记不准，**谁落地不受影响**（胜者永远按叠层顺序定）。
pub(super) fn resolve(overrides: &mut [PlanOverride], notes: &mut Vec<ImportIssue>) {
    let mut last: HashMap<String, usize> = HashMap::new();
    for (index, entry) in overrides.iter().enumerate() {
        last.insert(entry.path.clone(), index);
    }
    for index in 0..overrides.len() {
        let path = overrides[index].path.clone();
        let Some(&winner_index) = last.get(&path) else {
            continue;
        };
        if winner_index == index {
            continue;
        }
        let winner_dir = overrides[winner_index].dir.clone();
        let loser_dir = overrides[index].dir.clone();
        let same_bytes = overrides[winner_index].sha1 == overrides[index].sha1;
        let conclusion = if same_bytes {
            "两份内容逐字节相同，只落地一次"
        } else {
            "两份内容不同，以这一条为准"
        };
        notes.push(
            ImportIssue::new(
                keys::NOTE_OVERRIDE_SHADOWED,
                format!(
                    "{loser_dir}/{path} 与 {winner_dir}/{path} 声明同一实例路径：叠层顺序 \
                     overrides/ 在前、client-overrides/ 在后（Modrinth 官方客户端语义），\
                     {conclusion}"
                ),
            )
            .with_path(path),
        );
        overrides[index].shadowed = Some(Shadowed {
            winner: overrides[winner_index].source,
            winner_dir,
            same_bytes,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(dir: &str, path: &str, source: FileSource, sha1: &str) -> PlanOverride {
        PlanOverride {
            dir: dir.to_string(),
            path: path.to_string(),
            source,
            sha1: sha1.to_string(),
            size: 1,
            disabled: false,
            note: None,
            shadowed: None,
        }
    }

    #[test]
    fn the_later_layer_wins_and_the_earlier_one_is_shadowed() {
        // 叠层顺序：overrides/（Both）在前、client-overrides/（Client）在后 ⇒ 后者胜。
        let mut overrides = vec![
            entry(
                "overrides",
                "config/dup.toml",
                FileSource::Overrides,
                "aaaa",
            ),
            entry(
                "client-overrides",
                "config/dup.toml",
                FileSource::ClientOverrides,
                "bbbb",
            ),
        ];
        let mut notes = Vec::new();
        resolve(&mut overrides, &mut notes);

        let shadowed = overrides[0].shadowed.as_ref().expect("前一条要被标记");
        assert_eq!(shadowed.winner, FileSource::ClientOverrides);
        assert_eq!(shadowed.winner_dir, "client-overrides");
        assert!(!shadowed.same_bytes, "同路径不同哈希");
        assert!(overrides[1].shadowed.is_none(), "胜者不标遮蔽");
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].message_key, keys::NOTE_OVERRIDE_SHADOWED);
        assert_eq!(notes[0].path.as_deref(), Some("config/dup.toml"));
        let message = &notes[0].message;
        for expected in [
            "overrides/config/dup.toml",
            "client-overrides/config/dup.toml",
            "内容不同",
        ] {
            assert!(message.contains(expected), "缺 {expected}：{message}");
        }
    }

    #[test]
    fn identical_bytes_are_reported_as_same_bytes() {
        let mut overrides = vec![
            entry(
                "overrides",
                "config/same.toml",
                FileSource::Overrides,
                "aaaa",
            ),
            entry(
                "client-overrides",
                "config/same.toml",
                FileSource::ClientOverrides,
                "aaaa",
            ),
        ];
        let mut notes = Vec::new();
        resolve(&mut overrides, &mut notes);

        assert!(overrides[0].shadowed.as_ref().expect("标记").same_bytes);
        assert!(
            notes[0].message.contains("逐字节相同"),
            "内容一致要说清楚，别让人以为丢了东西：{}",
            notes[0].message
        );
    }

    #[test]
    fn different_paths_with_the_same_bytes_are_never_shadowed() {
        // 同哈希不同路径 = 两份文件，不是重复声明。按哈希去重会**静默丢一个文件**。
        let mut overrides = vec![
            entry("overrides", "mods/a.jar", FileSource::Overrides, "aaaa"),
            entry("overrides", "mods/b.jar", FileSource::Overrides, "aaaa"),
        ];
        let mut notes = Vec::new();
        resolve(&mut overrides, &mut notes);

        assert!(overrides.iter().all(|entry| entry.shadowed.is_none()));
        assert!(notes.is_empty());
    }

    #[test]
    fn the_same_path_twice_inside_one_group_is_shadowed_too() {
        // 同侧重复（模型被合并 / 手造时才会出现）：照样只落地一次，且注记两份来自同一目录。
        let mut overrides = vec![
            entry("overrides", "mods/x.jar", FileSource::Overrides, "aaaa"),
            entry("overrides", "mods/x.jar", FileSource::Overrides, "bbbb"),
        ];
        let mut notes = Vec::new();
        resolve(&mut overrides, &mut notes);

        assert_eq!(
            overrides[0].shadowed.as_ref().expect("标记").winner,
            FileSource::Overrides
        );
        assert_eq!(notes.len(), 1);
    }
}
