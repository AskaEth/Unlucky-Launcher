//! 禁用态的**底**：控件脚下那一层容器实际画出来的底色（M234）。
//!
//! ## 为什么不是一个 `Role::Surface`
//!
//! 禁用态的容器（按钮 10% / 面状控件 12%）与内容（38%）都要**预先合成成不透明色**
//! （D27：GPU/CPU 两档逐像素一致，所以不许把 α 交给渲染器混合）。合成必须知道「叠在什么上」，
//! 而这个「什么」是**宿主容器**的属性，不是控件的属性：
//!
//! ```text
//! 画布（Surface）      ← 页面底色
//!   └ 卡片（SurfaceContainer）      ← `surfaces::elevated::card`
//!       └ 面板（SurfaceContainerLow）← 取景板 / Card 层
//!           └ 禁用按钮（OnSurface@10% 叠在面板底上）
//! ```
//!
//! M229 之前 `disabled.rs` 的入口签名是 `(ui, surface: Role)`，而**所有调用点都传死
//! `Role::Surface`** —— 于是卡片里的禁用按钮把 10% 叠在画布色上，出来是一块与卡片底
//! 不协调的灰（M223 记的「CR 1.05、形丢失」）。参数在、语义也在，只是没人给真值。
//!
//! ## 纪律：底由**画那个底的容器**给
//!
//! 每个自己画底的容器**导出 `BASE` 常量**，画的时候用同一个 `Role`、给子控件时给这个常量
//! （见 [`super::super::surfaces::elevated::BASE`]、`inputs::menu::PANEL_BASE`）。两边同源，
//! 改色不会漂一半。
//!
//! iced 没有「环境上下文」（样式闭包只拿得到 `&Theme`，拿不到父容器的样式），所以这条信息
//! **只能显式往下传**：容器 → 控件（`.base(..)`）→ 禁用色求值。控件自己不猜。
//!
//! ## 抓得住 / 抓不住
//!
//! - 抓得住：合成用了哪个 `Role`（`interaction::disabled::tests` 的逐值断言、gallery 的像素 probe）；
//! - 抓不住：① 容器画的是**合成色**（半透明面板 α、状态层叠过的底）——`Base` 只装一个 `Role`；
//!   ② 调用点给了**错的** `Role`（机制不校验语义，判据只能落在「给的底对不对」的像素上）；
//!   ③ 忘了调 `.base(..)` 时回落到 [`Base::CANVAS`]，编译期不会报错（文档 + 像素 probe 兜）。

use unlucky_design::color::Role;

/// 控件脚下那一层的底（容器自己画的角色，**不是**「反正是 surface」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Base(Role);

impl Base {
    /// 画布底（`Surface`）：控件**直接**摆在页面/画布上时用它。
    ///
    /// 这也是各控件 `.base(..)` 的默认值——默认值只对「没有容器」这一种情形成立，
    /// 放进卡片/面板/浮层里**必须**换成那个容器的 `BASE`。
    pub const CANVAS: Base = Base(Role::Surface);

    /// 从容器**画自己底时用的那个角色**构造（容器应导出一个 `BASE` 常量走这条路）。
    pub const fn of(role: Role) -> Self {
        Self(role)
    }

    /// 该底对应的角色（合成时取色用）。
    pub const fn role(self) -> Role {
        self.0
    }
}

/// 容器也可以直接把**自己的角色**传进来（`disabled_*` 的入口收 `impl Into<Base>`）：
/// 这样历史调用点不必全改签名，但语义只有一条——「你脚下那一层是谁」。
impl From<Role> for Base {
    fn from(role: Role) -> Self {
        Self(role)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 画布底就是画布角色：`Base` 不许偷偷改语义。
    #[test]
    fn canvas_is_surface_and_of_keeps_the_role() {
        assert_eq!(Base::CANVAS.role(), Role::Surface);
        assert_eq!(
            Base::of(Role::SurfaceContainer).role(),
            Role::SurfaceContainer
        );
        // 容器底与画布底**不是同一个值**：这一条红了说明 `Base` 把角色丢了
        assert_ne!(Base::of(Role::SurfaceContainer), Base::CANVAS);
    }
}
