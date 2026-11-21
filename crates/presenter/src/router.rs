//! 页面路由与 AI 助手栏形态（09 §1 / D16）。
//!
//! 栈语义抄 HMCL 的导航栈：**顶层页面互相替换**（实例/下载/日志/设置），
//! 子页面（实例详情、AI 独立页）压栈，`back()` 逐层回退。
//! 本模块刻意不依赖 Slint 类型，便于单元测试；UI 侧枚举映射在 main.rs 的转换层做。

use crate::dto::InstanceId;

/// 一级导航项（左侧导航栏高亮单位）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavSection {
    /// 实例
    Instances,
    /// 下载 / 版本管理 + 内容库（M109：内容库复用这一项，不新增第 6 个图标）
    Download,
    /// 日志
    Logs,
    /// 设置
    Settings,
}

/// 页面路由。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    /// G1 实例页（主页）
    Instances,
    /// G2 下载页（M107/M108 起是「版本管理」）
    Download,
    /// 内容库（M109）：整合包 / Mod / 资源包 / 光影，来源 Modrinth / CurseForge
    ///
    /// 它是**顶层页**（跟版本管理一样整栈替换），但高亮复用 [`NavSection::Download`]：
    /// 用户令「下载页只负责下载整合包和Mod/光影」⇒ 同一入口下两块内容，不是一个新图标。
    Content,
    /// G3 日志页
    Logs,
    /// G6 设置页
    Settings,
    /// G7 账户页（三轨登录：微软正版 / 第三方皮肤站 / 本地档案）
    ///
    /// 一级页，但**不进主序列**：入口是导航轨底部的头像位（`shell/nav.rs`），
    /// 所以 [`Route::nav`] 对它返回 `None`（不点亮四个主项中的任何一个）。
    Accounts,
    /// 实例详情（G4）：从磁贴点进来，返回时回到实例页
    Detail(InstanceId),
    /// AI 助手独立页（D16 双形态之一）
    Agent,
}

impl Route {
    /// 是否是一级导航页（互相替换，不进栈）。
    pub fn is_top_level(&self) -> bool {
        matches!(
            self,
            // M109 的 `Content` 与 M114 的 `Accounts` 都是一级页：两个都留。
            Self::Instances
                | Self::Download
                | Self::Content
                | Self::Logs
                | Self::Settings
                | Self::Accounts
        )
    }

    /// 该路由高亮哪个导航项（None = 没有导航项高亮，如 AI 独立页）。
    pub fn nav(&self) -> Option<NavSection> {
        match self {
            Self::Instances | Self::Detail(_) => Some(NavSection::Instances),
            Self::Download | Self::Content => Some(NavSection::Download),
            Self::Logs => Some(NavSection::Logs),
            Self::Settings => Some(NavSection::Settings),
            Self::Accounts | Self::Agent => None,
        }
    }
}

/// AI 栏形态：右侧栏 / 独立全宽页（两形态共享同一份会话状态）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiPanelMode {
    /// 右侧常驻栏
    Docked,
    /// 拆出的独立全宽页
    Page,
}

/// 导航栈。
#[derive(Debug, Clone)]
pub struct Router {
    stack: Vec<Route>,
}

impl Default for Router {
    fn default() -> Self {
        Self::new()
    }
}

impl Router {
    /// 启动时停在实例页。
    pub fn new() -> Self {
        Self {
            stack: vec![Route::Instances],
        }
    }

    /// 当前页面。
    pub fn route(&self) -> &Route {
        // 栈不变量：构造后永远非空（navigate/back 都不允许清空）
        self.stack.last().expect("router stack is never empty")
    }

    /// 栈深度（1 表示没有可回退的子页面）。
    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    /// 是否能回退。
    pub fn can_go_back(&self) -> bool {
        self.stack.len() > 1
    }

    /// 跳转到某路由：一级页替换整栈，子页面压栈（重复目标不重复压栈）。
    pub fn navigate(&mut self, route: Route) {
        if route.is_top_level() {
            self.stack.clear();
            self.stack.push(route);
            return;
        }
        if self.stack.last() == Some(&route) {
            return;
        }
        self.stack.push(route);
    }

    /// 回退一层；已在根页面时返回 `false`。
    pub fn back(&mut self) -> bool {
        if self.stack.len() <= 1 {
            return false;
        }
        self.stack.pop();
        true
    }

    /// 当前 AI 栏形态（栈顶是 Agent 页即为独立页形态）。
    pub fn ai_mode(&self) -> AiPanelMode {
        if self.route() == &Route::Agent {
            AiPanelMode::Page
        } else {
            AiPanelMode::Docked
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AiPanelMode, NavSection, Route, Router};

    #[test]
    fn top_level_navigation_replaces_the_stack() {
        let mut router = Router::new();
        router.navigate(Route::Detail("a".into()));
        assert_eq!(router.depth(), 2);

        router.navigate(Route::Download);
        assert_eq!(router.depth(), 1);
        assert_eq!(router.route(), &Route::Download);
    }

    #[test]
    fn detail_pushes_and_back_pops_to_instances() {
        let mut router = Router::new();
        router.navigate(Route::Detail("survival".into()));
        assert!(router.can_go_back());
        assert_eq!(router.route().nav(), Some(NavSection::Instances));

        assert!(router.back());
        assert_eq!(router.route(), &Route::Instances);
        assert!(!router.back());
    }

    #[test]
    fn agent_page_switches_ai_panel_mode() {
        let mut router = Router::new();
        assert_eq!(router.ai_mode(), AiPanelMode::Docked);

        router.navigate(Route::Agent);
        assert_eq!(router.ai_mode(), AiPanelMode::Page);

        router.back();
        assert_eq!(router.ai_mode(), AiPanelMode::Docked);
    }

    #[test]
    fn navigating_to_the_same_sub_route_does_not_grow_the_stack() {
        let mut router = Router::new();
        router.navigate(Route::Agent);
        router.navigate(Route::Agent);
        assert_eq!(router.depth(), 2);
        assert_eq!(router.route().nav(), None);
    }
}
