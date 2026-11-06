//! 导航轨的**部件版**实现：`NAV` 表 + [`rail`]（宽 = 令牌 `nav_rail_width` = 88）。
//!
//! ## 两套 rail 的关系（M90 r3 订正，复审 P2-1；M99 把这张表从 `pages/mod.rs` 搬到这里）
//!
//! 产品路径（`app/view.rs`）走 `shell::view_with_content`，导航轨由 **`shell/nav.rs` 的手搓实现**画
//! ——它与 M78 组件几何同值（宽 88、药丸 56×32），复审逐像素量过「r1 产品图（真组件 rail）vs r2
//! 产品图（shell rail）在 y≥48 区域差异为 0」。组件版的 [`rail`] 只服务 `pages::app`
//! （`pages_demo` 出口图、harness）与单测。
//!
//! 两边的守门是分开的：`shell/nav.rs` 自带 indicator 56×32 的单测，`pages/tests.rs` 守的是下面
//! 这张表（表长 5 / 每一项都指向能高亮它的路由 / 第 5 项没有 section）。**将来哪边先漂没人拦**
//! ——统一要泛型化 shell 的消息类型，成本大于收益；在那之前任何改 rail 几何的改动都要两处一起改
//! 并各自跑守门（`pages/mod.rs` 偏离登记 #10）。

use iced::Element;

use unlucky_design::icons::IconKind;
use unlucky_i18n::{Key, Lang};
use unlucky_presenter::router::{NavSection, Route};

use crate::app::components::navigation::drawer::{self, Navigation};
use crate::app::components::navigation::navigation;
use crate::theme::Ui;

use super::Message;

/// 导航轨的固定项：图标 + 文案 key + 目标路由（顺序 = 自上而下）。
///
/// 顺序与 `presenter::router::NavSection` 的声明顺序一致，改这里等于改导航栏。
///
/// 第 3 列是 [`Key`] 而不是字面量（M108）：导航文案归目录管，`pages_demo` 出的图与将来的中文
/// 界面读的是同一份表，不会出现「页面翻译了、导航还是硬编码英文」。
///
/// **显示名 ≠ 标识符**：`Key::NavDownload` 这条说的是「版本管理 / Versions」，而它指向的仍是
/// `Route::Download`（用户令只改显示层，标识符改名后置）。
///
/// 表里存的是**收起态（88）的 key**；展开态（240）要对 [`label_key`] 过一道才换全名。
/// 可见性是 `pub(crate)`：`pages/tests.rs` 的三条断言读它（M99 从 `pages/mod.rs` 搬出来时，
/// 它从「模块私有」变成「crate 内可见」，断言一条没动、强度也没降）。
pub(crate) const NAV: [(Option<NavSection>, IconKind, Key, Route); 5] = [
    (
        Some(NavSection::Instances),
        IconKind::Instances,
        Key::NavInstances,
        Route::Instances,
    ),
    (
        Some(NavSection::Download),
        IconKind::Download,
        Key::NavDownload,
        Route::Download,
    ),
    (
        Some(NavSection::Logs),
        IconKind::Logs,
        Key::NavLogs,
        Route::Logs,
    ),
    (
        Some(NavSection::Settings),
        IconKind::Settings,
        Key::NavSettings,
        Route::Settings,
    ),
    // AI 独立页没有导航高亮项（`Route::nav()` 对它返回 `None`）；文案用 M106 给导航项新开的
    // `Key::NavAi`（与产品路径的 `shell/nav.rs` 同一条 key，两条 rail 不许各借一个词）。
    (None, IconKind::Ai, Key::NavAi, Route::Agent),
];

/// 这一项在当前宽度态下的文案 key。
///
/// `expanded` = D33 的 240 展开抽屉（用全名），`false` = 88 收起轨（用短名）。产品路径**当前只有
/// 88 收起态**，所以调用点只出现 `false`。
///
/// **为什么是 bool 参数而不是 `enum NavWidth { Rail, Drawer }`**：D33 的抽屉还没有渲染路径，
/// 枚举里那个 `Drawer` 变体在产品代码里永远构造不到，`-D warnings` 会按 `dead_code` 把它拦下来
/// （M108 第一版就是这么红的）。参数化之后没有「构造不到的状态」，切换点仍只有这一处。
pub(crate) const fn label_key(label: Key, expanded: bool) -> Key {
    if expanded && matches!(label, Key::NavDownload) {
        Key::NavDownloadExpanded
    } else {
        label
    }
}

/// 导航轨：**真组件**（`components::navigation` 的 rail 形态，宽 = 令牌 `nav_rail_width` = 88）。
///
/// M84 这里是一条留白带；M90 换成 M78 的 `NavigationRail`，于是「导航项 → 路由」是真的可点。
///
/// 语言暂时固定 [`Lang::En`]：组件版 rail 的调用点只有 `pages/mod.rs` 的 `nav::rail(ui, model.active)`，
/// 那里还没把 `Model.lang` 传进来（`pages/mod.rs` 不在 M108 的 scope 内）。文案本身已经走 [`Key`]，
/// 接线时只改这一行——产品路径那边同一笔欠账归 M106（它给 `shell/nav.rs` 接 i18n）。
pub(super) fn rail<'a>(ui: &'a Ui, active: Option<NavSection>) -> Element<'a, Message> {
    let catalog = Lang::En.catalog();
    let mut group = drawer::NavGroup::new();

    for (section, icon, label, route) in NAV {
        // 88 收起轨 = 短名（240 展开抽屉才用全名，见 [`label_key`]）
        let label = label_key(label, false);
        let mut item = drawer::NavItem::new(catalog.text(label), icon, Message::Navigate(route));
        if section.is_some() && active == section {
            item = item.selected();
        }
        group = group.item(item);
    }

    navigation(ui, Navigation::rail().group(group))
}

#[cfg(test)]
mod tests {
    //! 导航表与文案的守门（M108）。
    //!
    //! 这条断言原先挂在 `pages/tests.rs`，但那份文件在 M102 合并后是 464 行、撞了「单文件 ≤400 行」
    //! 的硬门禁（`components/gate.rs` 按全仓判红），于是按「谁的数据谁守」挪到本文件——`NAV` 表在
    //! 这里，守门也在这里，强度一条没降。

    use super::*;

    /// **M108**：这一页对外叫「版本管理」，代码里仍是 `Route::Download`。
    ///
    /// 用户令（2026-11-04）只要**显示层**改名：全仓重命名标识符会与在途的多条线全面撞车，
    /// 所以 `Route::Download` / `NavSection::Download` / `Screen::Download` / 文件名一个都不动。
    ///
    /// 英文有两个不同长度的名字，**是刻意的**（见 `catalog/nav.rs` 的「两条 key」一节）：
    /// 88px 收起轨放不下 `Version management`（≈101px），用短名 `Versions`；页标题与 240 展开抽屉
    /// 用全名。中文两处都是「版本管理」（≈48px，放得下就不缩）。
    ///
    /// **抓得住**：① 导航项或页标题又写回「下载」「Download」；② 88 轨上误用全名（会截断/换行）；
    /// ③ 副标题退回「只说下载」；④ 中文被「为了对称」砍短。**抓不住**：产品路径的导航轨
    /// （`shell/nav.rs` 是另一套手搓实现，它自己有一条同样的断言）、真画出来会不会截断（那要出图）、
    /// 中文目录在真机上的字体（截图通道固定 En，见 M108 报告与 finding）。
    #[test]
    fn the_download_route_is_presented_as_version_management() {
        let en = Lang::En.catalog();
        let zh = Lang::ZhCn.catalog();

        let entry = NAV
            .iter()
            .find(|entry| entry.3 == Route::Download)
            .expect("导航表里有版本管理页那一项");
        assert_eq!(entry.2, Key::NavDownload, "导航文案走目录，不写字面量");

        // 88 收起轨 = 短名；240 展开与页标题 = 全名（中文两处同值）
        assert_eq!(en.text(Key::NavDownload), "Versions");
        assert_eq!(en.text(Key::NavDownloadExpanded), "Version management");
        assert_eq!(en.text(Key::DownloadTitle), "Version management");
        assert_eq!(zh.text(Key::NavDownload), "版本管理");
        assert_eq!(zh.text(Key::NavDownloadExpanded), "版本管理");
        assert_eq!(zh.text(Key::DownloadTitle), "版本管理");
        assert_eq!(
            label_key(Key::NavDownload, false),
            Key::NavDownload,
            "88 收起态必须用短名"
        );
        assert_eq!(
            label_key(Key::NavDownload, true),
            Key::NavDownloadExpanded,
            "240 展开态换全名"
        );
        assert_eq!(
            label_key(Key::NavLogs, true),
            Key::NavLogs,
            "其余四项两态同一条 key"
        );

        // 目录里凡是指向这一页的文案都不许再说「下载」。唯一豁免是 `Downloading files`：
        // 那是安装流程里真在传文件的那一步，说的是当时的动作、不是页面身份。
        let page_keys = Key::ALL.iter().filter(|key| {
            (key.name().starts_with("Download") && **key != Key::DownloadStageDownloading)
                || **key == Key::NavDownload
                || **key == Key::NavDownloadExpanded
        });
        let mut checked = 0;
        for key in page_keys {
            assert!(
                !en.text(*key).to_ascii_lowercase().contains("download"),
                "{} 的英文文案还在说「下载」：{}",
                key.name(),
                en.text(*key)
            );
            assert!(
                !zh.text(*key).contains("下载"),
                "{} 的中文文案还在说「下载」：{}",
                key.name(),
                zh.text(*key)
            );
            checked += 1;
        }
        assert!(checked > 20, "只扫到 {checked} 条，守门形同虚设");

        // 副标题要讲这个页面「管什么」（版本 + 加载器），不是「能下载什么」
        let subtitle_en = en.text(Key::DownloadSubtitle);
        let subtitle_zh = zh.text(Key::DownloadSubtitle);
        assert!(
            subtitle_en.contains("versions") && subtitle_en.contains("loaders"),
            "副标题要提到版本与加载器：{subtitle_en}"
        );
        assert!(
            subtitle_zh.contains("版本") && subtitle_zh.contains("加载器"),
            "副标题要提到版本与加载器：{subtitle_zh}"
        );
    }
}
