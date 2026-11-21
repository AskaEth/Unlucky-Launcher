//! 实例页两个**视图档位**的写端（M455）：`ui.instances.view` / `ui.instances.tile_size`。
//!
//! ## 这一卷为什么单独存在
//!
//! `app/update/instances.rs` 是启动参数表单 / MODS / SAVES 的落点，M455 进来时已经 399 行
//! （用例搬去 `instances/tests.rs` 之后才腾出位置）。两个档位各一条分支，理由得放别处。
//!
//! ## 写什么、谁读
//!
//! | 用户动作 | 内存 | 盘上（**复用** M452 的单键写入器） | 谁读它 |
//! |---|---|---|---|
//! | 点视图 chip | `state.prefs.instance_view` | `SettingsBridge::persist_instance_view` | `app/view/instances.rs::view_mode` → 页面 |
//! | 点尺寸 chip | `state.prefs.tile_size` | `SettingsBridge::persist_tile_size` | 同上 |
//!
//! 两个键都**只改自己那一格**：M452 的 `persist_ui_key` 是「现读 → 改这一键 → 落盘」，
//! 于是「切完视图再改尺寸」不会把对方或 `ai.*` 抹回出厂（`ui_keys_tests` 的往返用例守着）。
//!
//! ## 为什么这两条消息在 `selected_instance_id()` 守卫**之前**
//!
//! 页头的 chip 在**列表页**也点得到，而列表页上没有选中实例（`Route::Instances`）。排在守卫后面
//! 的话，用户在列表页点尺寸什么都不会发生——那正是 M455 要修的那类「改了值界面没反应」。
//! 用例 `a_size_click_on_the_list_page_still_writes_the_key` 把这条顺序钉住（Route 停在列表页）。
//!
//! ## M461：两个 setter 还是**布局轨道的唯一触发点**
//!
//! 切视图 / 换尺寸改的是**摆放**（一列 ⇄ 每排 6/4/3 个），而行的 id 清单**一个字都没变** ⇒
//! 行增删那条差分（`pump/rows.rs::observe_rows`）永远看不见它，磁贴视图当年就是因此一跳到位。
//! 所以两个 setter 在写完档位之后各报一声 `motion::rows::relayout(List::Instances)`
//! （轨道与算术全是行轨道那一份，见 `motion/pump/layout.rs` 的模块文档）。
//!
//! 两条规矩：
//! - **只在值真的变了那一拍报**：上面两道「值没变就返回」的守卫就是幂等性本身（`relayout` 没有
//!   差分表——泵不知道上一次的列数，记一份就等于第二处「当前视图」的真相）。
//! - **换尺寸只在磁贴视图里报**：列表视图下尺寸没有任何可见效果
//!   （`pages/instances/tests/view_mode.rs` 钉着这一条），那时放一段「什么也不改变的动效」比不动更糟。

use iced::Task;
use unlucky_presenter::prefs::{InstanceView, TileSize};

use crate::app::components::motion;
use crate::pages::InstanceMessage;

use crate::app::state::App;
use crate::app::Message;

impl App {
    /// 实例页的一条消息里，属于**视图档位**的那两支（别的一律返回 `None`，由调用方继续分发）。
    ///
    /// 调用点在 `on_instance_message` 的 `selected_instance_id()` 守卫之前，理由见模块文档。
    pub(super) fn on_instance_view_mode(
        &mut self,
        message: &InstanceMessage,
    ) -> Option<Task<Message>> {
        match message {
            InstanceMessage::SetView(view) => Some(self.set_instance_view(*view)),
            InstanceMessage::SetTileSize(size) => Some(self.set_instance_tile_size(*size)),
            _ => None,
        }
    }

    /// 切实例页视图（`Tiles` / `List`）。
    ///
    /// 值没变就**什么都不做**（不写盘、不打日志）：点当前选中的那枚 chip 是一次 no-op，
    /// 而不是一次「把同一个值重写一遍」。
    fn set_instance_view(&mut self, view: InstanceView) -> Task<Message> {
        if self.state.prefs.instance_view == view {
            return Task::none();
        }
        self.state.prefs.instance_view = view;
        let saved = self.bridge.persist_instance_view(view);
        // M461：切视图 = 一次**布局**变化（一列 ⇄ 多格）。清单没变，所以行增删那条差分看不见它，
        // 必须显式报一声（轨道与算术全是行轨道那一份，见 `motion/pump/layout.rs`）。
        let replay = motion::rows::relayout(motion::rows::List::Instances);
        crate::app::diag::diag!(
            "[instances] view={} persist={} replay={}",
            view.as_str(),
            if saved { "saved" } else { "skipped" },
            replay
        );
        Task::none()
    }

    /// 选磁贴尺寸（小 / 中 / 大）。`Tiles` 之外的视图里它不影响可见结果，但仍然照写——
    /// 尺寸是用户偏好，切回磁贴视图时应当还在（不因为当时在列表视图就丢掉）。
    fn set_instance_tile_size(&mut self, size: TileSize) -> Task<Message> {
        if self.state.prefs.tile_size == size {
            return Task::none();
        }
        self.state.prefs.tile_size = size;
        let saved = self.bridge.persist_tile_size(size);
        // M461：列数换了 = 布局换了 ⇒ 逐格重走一遍进场腿。**只在磁贴视图里**报：
        // 列表视图下尺寸没有任何可见效果（`pages/instances/tests/view_mode.rs` 钉着这一条），
        // 那时放一段「什么也不改变的动效」比不动更糟。
        let replay = self.state.prefs.instance_view == InstanceView::Tiles
            && motion::rows::relayout(motion::rows::List::Instances);
        crate::app::diag::diag!(
            "[instances] tile={} columns={} persist={} replay={}",
            size.as_str(),
            size.columns(),
            if saved { "saved" } else { "skipped" },
            replay
        );
        Task::none()
    }

    /// 磁贴动效的**出图夹具**（M461）：`ULL_GRID_FIXTURE=size` 时在启动当拍**真的点一次**
    /// 「大」尺寸 chip。返回它派出的任务（这条消息本来就只改档位，`Task::none()`）。
    ///
    /// ## 为什么需要它
    ///
    /// 布局轨道（`motion/pump/layout.rs`）的触发是「用户切视图 / 换尺寸」，而截图通道从第一帧
    /// 起画的就是同一页、同一档——不制造一次**真实的布局变化**，`ULL_MOTION_AT` 钉住的相位上
    /// 没有任何东西可拍（与 `ULL_MOTION_FIXTURE` 存在的理由一字不差，见 `app/update.rs::on_boot`）。
    ///
    /// ## 为什么走消息、不直接改字段
    ///
    /// `on_instance_message` 是用户点芯片的那**同一条**通路（写内存 → 落盘 → 起轨道）。
    /// 绕过它直接写 `prefs.tile_size` 拍出来的图证明不了接线：真正会被绕过的那一段
    /// （`set_instance_tile_size` 的 `relayout` 调用）恰好是要拍的东西。
    ///
    /// ## 只影响这一次进程
    ///
    /// 两条闸门：①必须给了 `--screenshot`（产品启动读不到它）②变量值必须是 `size`。
    /// 落盘那一笔写进 `--data-dir` 指的临时目录（截图通道本来就带一个）。
    pub(in crate::app) fn grid_fixture(&mut self) -> Task<Message> {
        if self.options.screenshot.is_none() {
            return Task::none();
        }
        let Some(value) = std::env::var("ULL_GRID_FIXTURE").ok() else {
            return Task::none();
        };
        match value.trim() {
            "size" => {
                crate::app::diag::diag!("[boot] ULL_GRID_FIXTURE=size → SetTileSize(Large)");
                self.on_instance_message(InstanceMessage::SetTileSize(TileSize::Large))
            }
            other => {
                crate::app::diag::diag!("[boot] ULL_GRID_FIXTURE={other} 未知，忽略");
                Task::none()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    //! 判据是「**换一个桥对象读同一份文件**」（= 重启后的读法）+「换一个 App 启动读回来」，
    //! 不是「bridge 上有这两个方法」。与 `agent/rail_persist.rs`、`download_wiring.rs` 同一条口径。

    use std::path::PathBuf;

    use unlucky_design::color::SchemeFlavor;
    use unlucky_presenter::mock::MockBackend;
    use unlucky_presenter::prefs::keys;
    use unlucky_presenter::settings_bridge::SettingsBridge;

    use crate::app::state::App;
    use crate::cli::Options;
    use crate::pages::InstanceMessage;
    use crate::theme::Families;

    use super::*;

    const FAMILIES: Families = Families {
        text: "Segoe UI",
        display: "Segoe UI",
        mono: "Consolas",
    };

    /// 一份临时 `launcher.json`（父目录建好、文件删掉 = 首次启动）。
    fn temp_config(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ull-instances-view-{label}-{}-{}",
            std::process::id(),
            line!()
        ));
        std::fs::create_dir_all(dir.join("config")).expect("临时目录");
        let path = dir.join("config/launcher.json");
        let _ = std::fs::remove_file(&path);
        path
    }

    fn app_at(path: &PathBuf) -> App {
        App::with_presenter(
            Options::default(),
            FAMILIES,
            SchemeFlavor::Light,
            Box::new(MockBackend::new()),
            SettingsBridge::with_path(path),
        )
    }

    /// 盘上某个键的值（`None` = 这一键根本没写过）。
    ///
    /// 直接读 JSON 而不经桥：这条判据要的正是「文件里**有没有**这一键」，而
    /// `load_ui_keys` 会把缺键折成出厂值 —— 那样「写了」与「没写」就分不出来了。
    fn on_disk(path: &PathBuf, key: &str) -> Option<serde_json::Value> {
        let text = std::fs::read_to_string(path).ok()?;
        let doc: serde_json::Value = serde_json::from_str(&text).ok()?;
        let mut node = &doc;
        for part in key.split('.') {
            node = node.get(part)?;
        }
        Some(node.clone())
    }

    /// **点尺寸 chip ⇒ 写入器收到正确的 key 与值；重启读回还是它**。
    ///
    /// 抓得住：①消息没接线（`on_instance_view_mode` 没被 `on_instance_message` 调）；
    /// ②接线排在 `selected_instance_id()` 守卫之后（本用例停在**列表页**，守卫那一刻返回 `None`，
    /// 于是点了完全没反应）；③写成了别的键（`persist_tile_size` 里 key 写成 `view`）；
    /// ④只改内存不落盘（第二段「换一个 App 启动」会红）。
    #[test]
    fn a_size_click_on_the_list_page_still_writes_the_key() {
        let path = temp_config("size");
        let mut app = app_at(&path);
        // 前提：停在列表页（没有选中实例）——页头的 chip 就是在这一屏上点的。
        assert_eq!(app.selected_instance_id(), None);

        let _ = app.on_instance_message(InstanceMessage::SetTileSize(TileSize::Large));

        assert_eq!(app.state.prefs.tile_size, TileSize::Large, "内存态没跟上");
        assert_eq!(
            on_disk(&path, keys::TILE_SIZE),
            Some(serde_json::json!("large")),
            "写盘收到的 key/值不对"
        );
        assert_eq!(
            on_disk(&path, keys::INSTANCE_VIEW),
            None,
            "只该改一档，视图键不许被顺手写一条"
        );

        // 「重启」：新桥读同一份文件 → 新 App 启动读回。
        let restarted = app_at(&path);
        assert_eq!(
            restarted.state.prefs.tile_size,
            TileSize::Large,
            "重启必须读回（`adopt_ui_prefs` 的接线）"
        );
        let _ = std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
    }

    /// **切视图只改 `ui.instances.view`，不动兄弟键**（M174 的整份覆盖教训缩到单键）。
    ///
    /// 抓得住：有人图省事改用 `persist_ui_keys(UiKeys { .. })`——那会把已经写下的
    /// `ui.instances.tile_size = large` 抹回 `medium`（本用例第二段直接比盘上的值）。
    #[test]
    fn a_view_click_keeps_the_neighbouring_tile_size() {
        let path = temp_config("view");
        let mut app = app_at(&path);
        let _ = app.on_instance_message(InstanceMessage::SetTileSize(TileSize::Large));
        let _ = app.on_instance_message(InstanceMessage::SetView(InstanceView::List));

        assert_eq!(app.state.prefs.instance_view, InstanceView::List);
        assert_eq!(
            on_disk(&path, keys::INSTANCE_VIEW),
            Some(serde_json::json!("list"))
        );
        assert_eq!(
            on_disk(&path, keys::TILE_SIZE),
            Some(serde_json::json!("large")),
            "切视图把兄弟键抹回出厂了（这正是单键写入器存在的理由）"
        );

        let restarted = app_at(&path);
        assert_eq!(restarted.state.prefs.instance_view, InstanceView::List);
        assert_eq!(restarted.state.prefs.tile_size, TileSize::Large);
        let _ = std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
    }

    /// 点当前那一档 = **no-op**：不写盘（文件里连键都不出现）。
    #[test]
    fn clicking_the_current_value_is_a_no_op() {
        let path = temp_config("noop");
        let mut app = app_at(&path);
        // 出厂就是 `Tiles` / `Medium`。
        let _ = app.on_instance_message(InstanceMessage::SetView(InstanceView::Tiles));
        let _ = app.on_instance_message(InstanceMessage::SetTileSize(TileSize::Medium));

        assert_eq!(on_disk(&path, keys::INSTANCE_VIEW), None);
        assert_eq!(on_disk(&path, keys::TILE_SIZE), None);
        let _ = std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
    }
}
