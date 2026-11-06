//! 设置页「Appearance」分组文案（M116 首次落地：窗口圆角一档）。
//!
//! 加 key 时把条目写进 `$($acc)*` 后面即可（形状见 [`crate::catalog`] 的「域装配」一节）。
//!
//! ## 为什么只有圆角这一组的 key
//!
//! 本分组的既有控件（整窗/面板透明度、背景图、配色来源……）是 M93 落的，文案是英文基线字面量
//! （`pages/appearance/section.rs`）。M116 新增的圆角档位**从一开始就走词表**（tower 口径：
//! 设置项文案不许硬编码），所以这里先只有它这一组；老控件接词表归 i18n 接线 mission。
//!
//! ## 文案口径（不许暗示所有平台一致）
//!
//! 用户定案：Win11 / Server 2025 开系统圆角，Win10 可以不开，其他系统按窗口合成器的默认规则。
//! 所以说明句必须写清「谁负责画圆」而不是「我们支持圆角」——后半句在 Win10 上是假的。

macro_rules! settings_appearance_entries {
    ($next:ident, $($rest:ident,)* { $($acc:tt)* }) => {
        $next! { $($rest,)* {
            $($acc)*
            SettingsAppearanceCorner => ("Window corners", "窗口圆角"),
            SettingsAppearanceCornerSystem => ("Follow system", "跟随系统"),
            SettingsAppearanceCornerSquare => ("Square", "直角"),
            SettingsAppearanceCornerHint => (
                "On Windows 11 and Server 2025 we ask the system for rounded corners; Windows 10 keeps square corners. On macOS and Linux the desktop's own rules apply. We never draw corners ourselves.",
                "Windows 11 与 Windows Server 2025 上我们请求系统处理外框圆角；Windows 10 保持直角；macOS 与 Linux 交给桌面合成器自己的规则；我们不自己画圆角。"
            ),
            SettingsAppearanceCornerSuspended => (
                "Not in effect right now: window transparency is below 100%. Setting it back to 100% restores the rounded corners.",
                "当前因整窗透明度不生效：把整窗透明度调回 100% 圆角即恢复。"
            ),
        } }
    };
}

pub(crate) use settings_appearance_entries;
