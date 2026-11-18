//! 出图取证用的 SAVES 夹具（`ULL_SAVES_FIXTURE=1`）。
//!
//! # 它解决什么
//!
//! D30 的 SAVES 区截图需要一屏**有内容的列表**（世界 + 截图）。产品路径的列表来自后台线程
//! （列 `saves/` → 快照），而截图通道在第 3 帧就出图（`CAPTURE_AT_FRAME`）：那条后台往返来不及
//! 完成，截出来会是「正在读取 saves/…」。M157 / M222 / M223 / M224 都是同一个问题，处理方式一致：
//! **状态注入**，注入点放在「快照 → 页面模型」之前，于是转换逻辑照样被执行。
//!
//! # 边界（不要把它读成产品能力）
//!
//! - 只在环境变量存在时生效；产品路径读不到它 ⇒ 一帧都不影响；
//! - 它注入的是**快照**（[`SavesListView`]），不是页面模型——`Saves::from_state` 的时间格式化、
//!   计数、「有图标」标记走的都是产品代码；
//! - 真实链路由端到端用例钉住：`core::game::saves::tests`（真盘只读列举）与
//!   `presenter::host::saves::tests`（派生运行目录 → 快照）。
//!
//! ```text
//! ULL_SAVES_FIXTURE=1 ULL_INSTANCE_STATE=demo ULL_START_ROUTE=instances \
//!   unlucky-launcher-iced --screenshot=saves.png --software-renderer
//! ```

/// 夹具开关是否打开（`ULL_SAVES_FIXTURE` 非空且不为 `0`）。
#[must_use]
pub fn enabled() -> bool {
    std::env::var("ULL_SAVES_FIXTURE")
        .map(|value| {
            let value = value.trim();
            !value.is_empty() && value != "0"
        })
        .unwrap_or(false)
}

/// 2024-05-01 10:00 UTC（写死的 UTC 毫秒，不是"现在"）。
///
/// 写死有两个原因：两张主题图要能逐像素对照；而且它能证明「毫秒 → `YYYY-MM-DD HH:MM UTC`」
/// 这条换算在页面上真的走通了（`docs` 里的期望值就是这个时刻）。
const NEWEST: u64 = 1_714_557_600_000;
/// 比 [`NEWEST`] 早 32 天。
const MIDDLE: u64 = NEWEST - 32 * 24 * 3_600_000;
/// 比 [`NEWEST`] 早 42 天。
const OLDEST: u64 = NEWEST - 42 * 24 * 3_600_000;

/// 一份固定快照（实例 id 用调用方给的那个）。
///
/// 其中一个世界/一张截图的 mtime 是 `None`：那是「时间未知」这条分支的唯一出图证据。
#[must_use]
pub fn view(instance: &str) -> unlucky_presenter::dto::SavesListView {
    use unlucky_presenter::dto::{
        SavesListView, ScreenshotPreview, ScreenshotThumbFailure, ScreenshotThumbKind,
        ScreenshotView, WorldView,
    };

    let world = |dir: &str, icon: bool, modified_ms: Option<u64>| WorldView {
        dir: dir.to_string(),
        icon,
        modified_ms,
    };
    let shot = |file: &str, bytes: u64, modified_ms: Option<u64>, preview: ScreenshotPreview| {
        ScreenshotView {
            file: file.to_string(),
            bytes,
            modified_ms,
            preview,
        }
    };

    SavesListView {
        instance: instance.to_string(),
        dir: "D:/ULL/.ULL/data/instances/Better-MC/saves".to_string(),
        exists: true,
        worlds: vec![
            world("Better World", true, Some(NEWEST)),
            world("Redstone Lab", true, Some(MIDDLE)),
            world("Skyblock", false, Some(OLDEST)),
            world("Test Flat", false, None),
        ],
        screenshots_dir: "D:/ULL/.ULL/data/instances/Better-MC/screenshots".to_string(),
        screenshots_exists: true,
        // 四张截图，把缩略图墙的**四种格子**一次摆出来（M251）：
        // ① 白天（真像素）② 黄昏（真像素）③ 「未生成预览」占位 ④ 具名的失败原因。
        // 像素是**合成的**（下面 `scene`）：夹具不连真实解码器，出图只证明墙的排版与状态，
        // 解码由 presenter 的 `host::saves::thumbs` 用例用真 PNG 钉住。
        screenshots: vec![
            shot(
                "2024-05-01_10.00.00.png",
                1_887_436,
                Some(NEWEST),
                ScreenshotPreview::Ready(scene(1)),
            ),
            shot(
                "2024-04-30_21.14.05.png",
                2_097_152,
                Some(MIDDLE),
                ScreenshotPreview::Ready(scene(2)),
            ),
            shot(
                "2024-04-20_08.30.00.png",
                964_608,
                None,
                ScreenshotPreview::NotGenerated,
            ),
            shot(
                "2024-04-11_19.02.41.png",
                9_437_184,
                Some(OLDEST),
                ScreenshotPreview::Failed(ScreenshotThumbFailure {
                    kind: ScreenshotThumbKind::TooLarge,
                    detail: "9437184 > 8388608".to_string(),
                }),
            ),
        ],
        skipped: Vec::new(),
        // 出图夹具不造回执：默认那一张就是「刚列完目录」的样子。
        // 要看「备份成功那句话」长什么样，用 `ULL_SAVES_FIXTURE=backed-up`（见 `seed`）。
        last_op: None,
    }
}

/// 夹具形态（由 `ULL_SAVES_FIXTURE` 的取值决定；未知值 = 默认那一种）。
///
/// | 取值 | 出图抓什么 |
/// |---|---|
/// | `1`（或任何未知值） | 有内容的列表：世界行右侧的重命名 / 备份 / 删除 |
/// | `rename` | 行内重命名进行中（输入框 + 改名/取消） |
/// | `confirm` | 删除的二次确认模态框 |
/// | `backed-up` | 备份成功的回执那句话（带落点路径与大小） |
/// | `running` | 「游戏正在运行，先退出再操作」那条写失败（M247 的在飞闸门） |
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// 默认：列表 + 行操作区。
    Plain,
    /// 行内重命名进行中。
    Rename,
    /// 删除确认模态。
    Confirm,
    /// 备份成功的回执。
    BackedUp,
    /// 写操作被「游戏在运行」挡下。
    Running,
}

fn mode() -> Mode {
    match std::env::var("ULL_SAVES_FIXTURE")
        .unwrap_or_default()
        .trim()
    {
        "rename" => Mode::Rename,
        "confirm" => Mode::Confirm,
        "backed-up" => Mode::BackedUp,
        "running" => Mode::Running,
        _ => Mode::Plain,
    }
}

/// 注入这一屏需要的状态（**只走产品路径的入口**：`changed` / `start_rename` / `ask_delete` /
/// `dialogs.push`，一处新渲染分支都没有）。
pub fn seed(app: &mut crate::app::state::App, instance: &str) {
    let mode = mode();
    let mut view = view(instance);
    if mode == Mode::BackedUp {
        view.last_op = Some(unlucky_presenter::dto::SavesOped::BackedUp {
            dir: "Better World".to_string(),
            file: "Better World-20240501-100000.zip".to_string(),
            path: "D:/ULL/.ULL/data/exports/saves/Better World-20240501-100000.zip".to_string(),
            bytes: 84_213_760,
            skipped: 0,
        });
    }
    let first = view.worlds.first().map(|world| world.dir.clone());
    app.state.saves.changed(view);
    let Some(dir) = first else {
        return;
    };
    match mode {
        Mode::Rename => app.state.saves.start_rename(&dir),
        // 在飞闸门那条失败（`host::saves::dispatch` 在 `running` 时直接回它）。
        Mode::Running => app
            .state
            .saves
            .failed(unlucky_presenter::dto::SavesFailureView {
                instance: instance.to_string(),
                file: dir,
                op: Some(unlucky_presenter::dto::SavesFailureOp::Rename),
                kind: unlucky_presenter::dto::SavesFailureKind::GameRunning,
                detail: String::new(),
            }),
        Mode::Confirm => {
            let (title, body, primary, secondary) =
                crate::pages::saves_delete_prompt(app.lang().catalog(), &dir);
            let dialog = app.state.dialogs.push(
                unlucky_presenter::dialogs::DialogSource::UserAction,
                title,
                body,
                unlucky_presenter::dialogs::DialogKind::Confirm { primary, secondary },
            );
            app.state.saves.ask_delete(dialog, &dir);
        }
        Mode::Plain | Mode::BackedUp => {}
    }
}

/// 一张**合成的**缩略图（160×90 RGBA）：天空渐变 + 地面 + 太阳/月亮。
///
/// 为什么合成而不是读真图：夹具的职责是「让页面有东西可画」，而它跑在截图通道的第 3 帧——
/// 真解码要真的有一张 PNG 在盘上、还要等后台线程回来。合成像素让出图**确定**（两张主题图
/// 逐像素可比），并且明确地不假装自己是解码证据（那由 presenter 的用例负责）。
///
/// `tone`：`1` = 白天（亮天空 + 绿地面），其余 = 黄昏（橙天空 + 暗地面）。同一套几何，
/// 两种配色——墙上两格一眼能看出"每格各画各的"。
fn scene(tone: u8) -> unlucky_presenter::dto::ThumbnailView {
    const W: u32 = 160;
    const H: u32 = 90;
    // 地平线在 2/3 高度（截图里天空总比地面多）。
    const HORIZON: u32 = H * 2 / 3;
    let (sky_top, sky_bottom, ground, sun): ([u8; 3], [u8; 3], [u8; 3], [u8; 3]) = if tone == 1 {
        (
            [128, 176, 240],
            [200, 224, 248],
            [86, 138, 66],
            [255, 246, 186],
        )
    } else {
        ([96, 66, 96], [232, 148, 96], [58, 74, 48], [255, 214, 150])
    };
    let mut pixels = Vec::with_capacity((W * H * 4) as usize);
    for y in 0..H {
        for x in 0..W {
            let (r, g, b) = if y < HORIZON {
                // 天空：从顶到底线性插值（`t` = 0..=255 的定点比例）。
                let t = y * 255 / HORIZON.max(1);
                let mix = |from: u8, to: u8| -> u8 {
                    let span = i32::from(to) - i32::from(from);
                    (i32::from(from) + span * t as i32 / 255) as u8
                };
                (
                    mix(sky_top[0], sky_bottom[0]),
                    mix(sky_top[1], sky_bottom[1]),
                    mix(sky_top[2], sky_bottom[2]),
                )
            } else {
                // 地面：按 3px 一条的条纹给出「方块地形」的观感。
                let stripe = (y / 3) % 2 == 0;
                let shade: u8 = if stripe { 0 } else { 14 };
                (
                    ground[0].saturating_sub(shade),
                    ground[1].saturating_sub(shade),
                    ground[2].saturating_sub(shade),
                )
            };
            // 太阳/月亮：右上角一个 18px 的圆（够小，不会盖住画面主体）。
            let (dx, dy) = (x as i32 - 126, y as i32 - 26);
            let (r, g, b) = if dx * dx + dy * dy <= 18 * 18 {
                (sun[0], sun[1], sun[2])
            } else {
                (r, g, b)
            };
            pixels.extend_from_slice(&[r, g, b, 255]);
        }
    }
    unlucky_presenter::dto::ThumbnailView {
        width: W,
        height: H,
        pixels,
    }
}
