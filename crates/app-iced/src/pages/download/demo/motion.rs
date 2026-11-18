//! **列表增删动效的中间态**取样点（M305）：`ULL_DL_DEMO=list-enter|list-exit`。
//!
//! 从 `demo.rs` 拆出来（那边正好 397 行、零余量）：本文件只管「制造一次真实的增删」。
//!
//! ## 两件事都走产品路径
//!
//! 1. 模型由 [`Download::from_snapshot`] 建（分类过滤、退场池都在里面）；
//! 2. 差分由泵真做（`observe_rows` 比对两次观察），**不是**硬塞一个 progress 值。
//!
//! ## 唯一的「演示特权」是顺序与定档
//!
//! 产品里两次观察隔着一条事件；这里在同一个 view 里连做两次（`Once` 只做一次，否则每帧重摆会把
//! 轨道重起）。相位由 `ULL_MOTION_AT` 钉死 ⇒ 同一张图每次都一样（D30）。
//!
//! 为什么定档（`motion::configure`）在这一层：截图走的是 `pages/harness.rs` 那条独立 app，
//! 它**不建 `App`**，也就不会经过 `app/update.rs` 的定档；而 harness 不在本 mission 的 scope 里。
//! 给了 `ULL_MOTION_AT` 才开动效，没给就保持静息——与产品截图通道「不钉相位就是静帧」同一条纪律。

use unlucky_design::motion::MotionScale;
use unlucky_i18n::Catalog;
use unlucky_presenter::dto::{AppEvent, CoreEvent};
use unlucky_presenter::state::AppState;

use crate::app::components::motion;
use crate::pages::download::model::{Category, Download};
use crate::pages::download::options::InstallDraft;

use super::demo_versions;

/// 演一次「分类从 A 换到 B」的列表增删，返回**换完之后**的模型（退场池已填好）。
pub(super) fn list_motion(catalog: Catalog, entering: bool) -> Download {
    static ONCE: std::sync::Once = std::sync::Once::new();

    let mut state = AppState::new();
    state.apply(AppEvent::Core(CoreEvent::VersionListChanged(
        demo_versions(),
    )));
    let draft = InstallDraft::default();
    ONCE.call_once(|| {
        // 出图通道的定档：harness 不建 `App`，所以这一档只能由演示模型自己钉——`ULL_MOTION_AT`
        // 给了相位才开动效，否则保持静息（D30：没钉相位的出图不许有动画）。
        let pin = std::env::var("ULL_MOTION_AT")
            .ok()
            .and_then(|raw| raw.trim().parse::<f64>().ok());
        if pin.is_some() {
            motion::configure(MotionScale::STANDARD, pin);
        }
        // 用「远古版 ⇄ 全部」而不是「正式版 ⇄ 全部」：远古版只有两行、落在列表**最前面**，
        // 于是进去的 6 行错位档是 0/1/2/3/3/3（`list-enter` 图里能一眼看出「错位」）；
        // 反过来退的那 6 行同理。
        let (before, after) = if entering {
            (Category::Ancient, Category::All)
        } else {
            (Category::All, Category::Ancient)
        };
        // 两次真转换 = 两次真观察（第二次的差分就是这一次增删）。
        let _ = Download::from_snapshot(&state, before, &draft, catalog);
        let _ = Download::from_snapshot(&state, after, &draft, catalog);
    });

    let category = if entering {
        Category::All
    } else {
        Category::Ancient
    };
    Download::from_snapshot(&state, category, &draft, catalog)
}
