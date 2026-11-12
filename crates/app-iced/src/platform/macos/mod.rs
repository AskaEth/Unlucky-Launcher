//! macOS 的窗口外框（M182）：**本轮不实现**，这里只有结论、证据与后续配方。
//!
//! 用户第三轮 Mac 真机验收的现象：四角是直角，日志 `[corner] … source=square os=unknown`。
//! 「macOS 不支持圆角」是**错的**——macOS 的窗口四角本来就有系统圆角，只是我们交上去的窗口
//! 不在会被裁的那一类里。三环链条如下，每一环都能在本机复核（读锁文件里的依赖源码即可）：
//!
//! ## 第 1 环：我们交的是什么窗
//!
//! `crates/app-iced/src/app/boot/diagnostics.rs` 的 `window_settings()` 给的是
//! `decorations: false`（M93 自绘三键）。那是**唯一产地**，`[window]` / `[caps]` 两行
//! 验收日志也从它取字段。
//!
//! ## 第 2 环：winit 把 `decorations: false` 翻成什么
//!
//! winit 0.30.13 `src/platform_impl/macos/window_delegate.rs:540-552`：
//!
//! ```text
//! let mut masks = if (!attrs.decorations && screen.is_none()) || titlebar_hidden {
//!     // Resizable without a titlebar or borders
//!     NSWindowStyleMask::Borderless | NSWindowStyleMask::Resizable | NSWindowStyleMask::Miniaturizable
//! } else {
//!     // default case, resizable window with titlebar and titlebar buttons
//!     NSWindowStyleMask::Closable | Miniaturizable | Resizable | Titled
//! };
//! ```
//!
//! ⇒ `decorations: false` 拿到的是 **`Borderless`**，**没有 `Titled`**。
//!
//! ## 第 3 环：窗口服务器只给 `Titled` 窗做圆角裁剪
//!
//! 这是 AppKit 的标准窗口行为（A 级外部事实：标准窗口的圆角由窗口服务器连同阴影一起画，
//! 无边框窗口不参与）。本仓库**没有**任何一行代码能影响它：winit 的 macOS 后端里
//! `cornerRadius` / `CornerRadius` 命中数为 **0**（`src/platform_impl/macos/` 内四命中全是
//! 「屏幕/视图左上角」的注释），也就是说圆角从来没有一行业是我们调的。
//!
//! 反向印证：用户第三轮实测（本机窗口就是 borderless）拍到四角直角，与第 3 环一致。
//!
//! 判据一侧的落点：`plan_corner_for(MacOs, …)` 现在返回
//! `source=compositor excluded_reason=borderless`（见 [`crate::platform::win32::corner`]），
//! 而不是原先含糊的 `source=square os=unknown`。
//!
//! ## 为什么**不**用 objc 给 `contentView.layer` 设 `cornerRadius`（M182 任务书原路线，已否）
//!
//! 1. **违反 D46 第 4 条**「macOS / X11 / Wayland 按合成器默认、**不主动自绘**」：
//!    把内容层裁成圆角就是自绘裁切，同一份决策日志里已经否决过这条（还附带
//!    「会把标题栏三键与边缘缩放热区一起裁掉」的问题）。
//! 2. **撞 `unsafe` 铁律**：根 `Cargo.toml` 的 `[workspace.lints.rust] unsafe_code = "deny"`，
//!    且 `guards.yml` 的 `unsafe-scope` 作业要求「带 `allow(unsafe_code)` 的文件集合**恰好等于**
//!    `crates/app-iced/src/platform/win32/transparency.rs`」——新增一个 objc FFI 文件会让该作业
//!    直接红，而那个工作流不在 M182 的改动范围内。
//! 3. **拿不到 `NSWindow`**：`app/window_alpha.rs` 的 `probe_hwnd` 在非 Windows 恒返回 `None`，
//!    于是 `App::apply_window_corner` 走 `NoWindow` 分支——macOS 上那条下发链**从来没被调用过**。
//!    要接线得改 `app/window_alpha.rs` + `app/update.rs`（M182 范围之外，且 M168/M178 正在改
//!    `app/**`）。
//! 4. **本机编不了**：macOS 分支要靠 macOS 工具链，Windows 上连类型检查都跑不到。
//!
//! ## 后续口径（**未实现**，需另立 mission + Mac 真机第 4 轮验收）
//!
//! 目标：把外框交还给窗口服务器当标准窗口，由它按自己的规则圆角（= 用户定的
//! 「其他系统按照窗口合成器的默认规则来」）：
//!
//! ```text
//! decorations: true,
//! platform_specific: { title_hidden: true, titlebar_transparent: true, fullsize_content_view: true }
//! ```
//!
//! - 三个字段 iced 0.14 已暴露（`iced_core-0.14.0/src/window/settings/macos.rs`），
//!   iced_winit 1:1 转发（`iced_winit-0.14.1/src/conversion.rs:125-137` 的
//!   `#[cfg(target_os = "macos")]` 块）；
//!   `FullSizeContentView` 让内容铺满整窗（不留原生标题栏高度），窗口服务器把整窗
//!   ——**连同我们的背景图**——裁成圆角，不会出现「外框圆了、里面还是方块」。
//! - **必须同时隐藏我们自绘的三键**（`crates/app-iced/src/shell/mod.rs`），否则屏幕上会同时存在
//!   左上角原生三色灯与右上角自绘三键两套控件——那不是「跟随合成器」，是两套控件打架。
//! - **已知阻塞点（真做之前必须先解掉）**：winit 有 `with_titlebar_buttons_hidden`
//!   （`src/platform/macos.rs:290`，能一次藏掉全部三色灯），但 **iced 0.14 的
//!   `PlatformSpecific` 没有转发它**；而 `WindowExtMacOS` 也没有运行期隐藏按钮的接口
//!   （只有 `set_has_shadow` / `set_document_edited` 等）。三条出路：① 给 iced 上游提转发
//!   （最干净，但要等版本）；② 仅用 `closeable: false, minimizable: false` 藏掉关闭/最小化
//!   两键——**缩放键藏不掉**，会留一个孤零零的绿灯；③ 拿到 `NSWindow` 后手动 `setHidden:`
//!   （回到 objc + `unsafe`，见上面的第 2、3 条）。
//! - **验收日志会一起变**：`renderer::acceptance::visual` 从 `decorations` 推
//!   `controls=self-drawn|native`，一旦改成 `true`，那行会写 `controls=native`，而我们当时
//!   仍在自绘三键（除非同一次改动把三键撤掉）⇒ 那条改动必须**同一个 mission 内**一起做。
//! - **验证路径**：本机（Windows）拿不到任何 macOS 像素；只能靠
//!   `ci.yml` 的 `runtime-smoke-macos` 作业（dispatch 触发、`continue-on-error`）看**编不编得过**，
//!   视觉效果必须等**第 4 轮 Mac 真机验收图**或用户手动确认。
//!
//! ## 本轮明确未验
//!
//! - macOS 圆的半径、平滑度、与系统其它窗口是否一致（无真机）；
//! - 高 DPI（Retina）下系统圆角的视觉半径（由系统决定，我们读不到，也不需要缩放——这一档
//!   本来就「不归我们管」，见 [`crate::platform::win32::corner`] 的 `radius=system`）；
//! - 原生三色灯的最终观感与「是否保留自绘三键」的产品取舍。
