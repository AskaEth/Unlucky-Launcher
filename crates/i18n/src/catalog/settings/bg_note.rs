//! 外观分组背景图诊断的**补语**（M314 `I18NFINISH`）。
//!
//! ## 为什么单开一个链尾域
//!
//! `settings_appearance_entries` 在装配链**中间**：往那个域追加会把其后所有 key 在 `Key::ALL`
//! 里整体平移 ⇒ 金表（逐 index 逐字节）整片红。所以本域挂在链尾
//! （`settings_accent_entries` 之后），既有顺序一个不动，金表只需在 `tail2::ROWS` 末尾追加。
//! 做法与 [`super::accent`] 完全一致。
//!
//! ## 这两条 key 修的是什么
//!
//! 背景图诊断的 `{reason}` 里曾经塞着两句**本仓自写**的英文：
//! `"no path configured"`（空路径）与 `"unknown image format (magic bytes not recognised)"`
//! （图头不认识）。中文界面上它们分别变成「读取失败：no path configured」——一半中文一半英文。
//! M180 把这两句记成「不可翻译的诊断原文」，但它们不是 OS / 解码库给的，是本仓自己写的一句话，
//! 所以 M314 给它们各开一条 key（各自的修法也不同：一个是「去填路径」，一个是「换张图」）。
//!
//! key 前缀仍叫 `SettingsAppearance*`：`catalog/settings/appearance.rs` 的译文守卫按**名字前缀**
//! 筛 key，改名会让那条守卫漏掉这一条。

macro_rules! settings_bg_note_entries {
    ($next:ident, $($rest:ident,)* { $($acc:tt)* }) => {
        $next! { $($rest,)* {
            $($acc)*
            // ── 背景图：路径为空（M314）
            SettingsAppearanceBgNoPath => (
                "No background image path configured",
                "还没有填写背景图路径"
            ),
            SettingsAppearanceBgUnknownFormat => (
                "The file is not an image this build can read (unrecognised header)",
                "这个文件的格式本版读不出来（文件头不认识）"
            ),
        } }
    };
}

pub(crate) use settings_bg_note_entries;
