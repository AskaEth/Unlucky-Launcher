//! 实例相关：磁贴外观（相位/进度/徽标/占位色）、运行中列表。

use slint::Color;

use crate::{LoaderKind, RuntimePhase, UiTile};
use unlucky_presenter::dto::{self, InstanceRuntime};
use unlucky_presenter::state::{AppState, TileCell};

/// 头像/图标占位的底色池（Metro 强调色系，按 id 哈希稳定取色）。
const HUES: [u32; 8] = [
    0x0078D4, 0x8764B0, 0xD13438, 0xCA5010, 0x498205, 0x038387, 0x5C2D91, 0x0050EF,
];

/// 加载器枚举。
pub fn loader(kind: dto::LoaderKind) -> LoaderKind {
    match kind {
        dto::LoaderKind::Vanilla => LoaderKind::Vanilla,
        dto::LoaderKind::Fabric => LoaderKind::Fabric,
        dto::LoaderKind::Quilt => LoaderKind::Quilt,
        dto::LoaderKind::Forge => LoaderKind::Forge,
        dto::LoaderKind::NeoForge => LoaderKind::Neoforge,
        dto::LoaderKind::OptiFine => LoaderKind::Optifine,
        dto::LoaderKind::LiteLoader => LoaderKind::Liteloader,
    }
}

/// 运行时状态 → (磁贴外观相位, 进度 0~1, 阶段文案)。
pub fn phase(runtime: &InstanceRuntime) -> (RuntimePhase, f32, String) {
    match runtime {
        InstanceRuntime::Idle => (RuntimePhase::Idle, 0.0, String::new()),
        InstanceRuntime::Busy { stage, progress } => {
            let phase = match stage {
                dto::LaunchStage::Download => RuntimePhase::Downloading,
                dto::LaunchStage::Verify => RuntimePhase::Verifying,
                dto::LaunchStage::Launch => RuntimePhase::Launching,
                dto::LaunchStage::Window => RuntimePhase::Running,
            };
            (phase, progress.ratio(), stage.label().to_string())
        }
        InstanceRuntime::Running { .. } => (RuntimePhase::Running, 1.0, "运行中".to_string()),
        InstanceRuntime::Failed { reason } => (RuntimePhase::Failed, 0.0, reason.clone()),
    }
}

/// 名字首字（无图标时的占位字母）。
pub fn initial_of(name: &str) -> String {
    name.chars()
        .next()
        .map(|c| c.to_string())
        .unwrap_or_else(|| "?".to_string())
}

/// id → 稳定底色（同一实例每次启动同色）。
pub fn hue_of(id: &str) -> Color {
    let hash = id.bytes().fold(0u32, |acc, byte| {
        acc.wrapping_mul(31).wrapping_add(u32::from(byte))
    });
    Color::from_argb_encoded(0xFF00_0000 | HUES[(hash as usize) % HUES.len()])
}

/// ISO-8601 UTC → 逗号少的本地展示串（`2026-10-29 19:22`）；None/空 → 从未启动。
pub fn last_played(raw: &Option<String>) -> String {
    match raw {
        None => "从未启动".to_string(),
        Some(value) if value.is_empty() => "从未启动".to_string(),
        Some(value) => {
            let trimmed = value.trim_end_matches('Z');
            let mut out = String::with_capacity(trimmed.len());
            let mut seen_time = false;
            for ch in trimmed.chars() {
                match ch {
                    'T' => {
                        seen_time = true;
                        out.push(' ');
                    }
                    ':' if seen_time && out.ends_with(|c: char| c.is_ascii_digit()) => {
                        // 只保留到分钟：秒（第三段冒号之后）直接截断
                        if out.matches(':').count() >= 1 {
                            break;
                        }
                        out.push(ch);
                    }
                    _ => out.push(ch),
                }
            }
            out
        }
    }
}

/// 单个磁贴。
pub fn tile(cell: &TileCell<'_>) -> UiTile {
    let instance = cell.instance;
    let (phase, progress, label) = phase(&instance.runtime);
    UiTile {
        id: instance.id.clone().into(),
        name: instance.name.clone().into(),
        mc_version: instance.mc_version.clone().into(),
        loader: loader(instance.loader.kind),
        loader_version: instance.loader.version.clone().into(),
        loader_label: instance.loader.badge().into(),
        last_played: last_played(&instance.last_played).into(),
        phase,
        phase_label: label.into(),
        progress,
        icon: slint::Image::default(),
        hue: hue_of(&instance.id),
        initial: initial_of(&instance.name).into(),
        row: i32::try_from(cell.row).unwrap_or(0),
        col: i32::try_from(cell.col).unwrap_or(0),
    }
}

/// 磁贴网格（已按当前尺寸算好 row/col）。
pub fn tiles(state: &AppState) -> Vec<UiTile> {
    state.tiles().iter().map(tile).collect()
}

/// 运行中实例（顶部迷你状态条）。
pub fn running_tiles(state: &AppState) -> Vec<UiTile> {
    state
        .running()
        .iter()
        .enumerate()
        .map(|(index, instance)| {
            let cell = TileCell {
                instance,
                row: 0,
                col: index,
            };
            tile(&cell)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{hue_of, initial_of, last_played};

    #[test]
    fn initial_is_first_grapheme_or_question_mark() {
        assert_eq!(initial_of("生存整合包"), "生");
        assert_eq!(initial_of("Vanilla"), "V");
        assert_eq!(initial_of(""), "?");
    }

    #[test]
    fn hue_is_stable_per_id() {
        assert_eq!(hue_of("survival"), hue_of("survival"));
        // 不同 id 大体不同色（哈希池 8 色，这里只验证确定性 + 不 panic）
        let _ = hue_of("another-instance");
    }

    #[test]
    fn last_played_formats_iso_to_minutes() {
        assert_eq!(
            last_played(&Some("2026-10-29T19:22:45Z".to_string())),
            "2026-10-29 19:22"
        );
        assert_eq!(last_played(&None), "从未启动");
        assert_eq!(last_played(&Some(String::new())), "从未启动");
    }
}
