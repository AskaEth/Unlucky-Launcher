//! `transparency` 的测试（M116 从 `transparency.rs` 搬出来：那个文件要留行数给 DWM 的 FFI）。
//!
//! **本文件零 `unsafe`、零 `extern`**：只调父模块的安全包装。别把 FFI 写进来——
//! 「全仓唯一允许 unsafe 的文件」是 `transparency.rs`，不是它下面的每个文件。

use super::{
    alpha_byte, apply_window_alpha, ensure_layered, AlphaOutcome, AlphaRejection, GWL_EXSTYLE,
    LWA_ALPHA, WS_EX_LAYERED,
};
use unlucky_presenter::prefs::WINDOW_ALPHA_MIN;

#[test]
fn layered_constants_match_the_win32_header() {
    // 抓得住：常量写错（例如把 GWL_EXSTYLE 写成 -16 = GWL_STYLE，或 WS_EX_LAYERED 少写一个 0）。
    // 抓不住：这些值在真实窗口上是否真的生效（那要靠真机日志，见 app 侧）。
    assert_eq!(WS_EX_LAYERED, 0x0008_0000);
    assert_eq!(GWL_EXSTYLE, -20);
}

#[test]
fn ensure_layered_refuses_a_missing_or_invalid_window() {
    // 句柄 0 = 还没拿到窗口：不许「假设成功」。
    assert!(!ensure_layered(0), "句柄 0 必须返回 false");
    // 1 号句柄永远无效：读不到扩展样式，也就加不上这一位。
    assert!(!ensure_layered(1), "无效句柄必须返回 false（不 panic）");
}

#[test]
fn alpha_byte_quantizes_upward_never_below_the_floor() {
    assert_eq!(alpha_byte(1.0), 255);
    assert_eq!(alpha_byte(0.5), 128, "0.5×255=127.5 → 向上取整 128");
    assert_eq!(
        alpha_byte(WINDOW_ALPHA_MIN),
        21,
        "0.08×255=20.4 → 向上取整 21；用 round 会得到 20，实际下发 20/255=0.0784 低于 D36 下限"
    );
    // 漏过预处理的坏值也不会变成「穿透」：一律落到下限字节。
    for bad in [0.0, -3.0, f64::NAN] {
        assert_eq!(
            alpha_byte(bad),
            alpha_byte(WINDOW_ALPHA_MIN),
            "{bad} 应取下限"
        );
    }
    assert_eq!(
        LWA_ALPHA, 0x2,
        "0x2 就是 LWA_ALPHA；写成 0x1 会变成色键、等于没设 α"
    );
}

#[test]
fn applied_alpha_is_never_below_the_floor() {
    // 真正生效的 α 是 `bAlpha / 255`，不是传进来的 f64。这条把「改用 round」的行为钉死：
    // round 下 0.08 → 20 → 20/255 = 0.0784 < 0.08 → 这里红。
    for requested in [WINDOW_ALPHA_MIN, 0.081, 0.1, 0.33, 0.5, 0.77, 0.999, 1.0] {
        let byte = alpha_byte(requested);
        let applied = f64::from(byte) / 255.0;
        assert!(
            applied >= WINDOW_ALPHA_MIN - 1e-12,
            "requested={requested} → byte={byte} → 实际 α={applied}，低于下限 {WINDOW_ALPHA_MIN}"
        );
        assert!(
            applied <= 1.0,
            "requested={requested} → 实际 α={applied} 超过 1.0"
        );
    }
}

#[test]
fn missing_or_opaque_alpha_never_touches_the_window() {
    for none in [None, Some(0.0), Some(-0.5), Some(f64::NAN)] {
        assert_eq!(
            apply_window_alpha(1234, none),
            AlphaOutcome::Rejected(AlphaRejection::NoAlpha),
            "{none:?} 应被拒绝（整窗穿透不许发生）"
        );
    }
    // α = 1.0：两个平台都返回 Opaque —— 正好证明「不调 FFI」不是靠 Unsupported 混过去的。
    assert_eq!(apply_window_alpha(1234, Some(1.0)), AlphaOutcome::Opaque);
    // 合法 α 但没有句柄：拒绝，绝不下发。
    assert_eq!(
        apply_window_alpha(0, Some(0.5)),
        AlphaOutcome::Rejected(AlphaRejection::NoWindow)
    );
}

#[cfg(target_os = "windows")]
#[test]
fn invalid_handle_reports_a_win32_failure_instead_of_panicking() {
    // 用 1 号句柄（永远无效）真调一次 API：验的是**返回值语义**，不是观感。
    match apply_window_alpha(1, Some(0.5)) {
        AlphaOutcome::Failed { code } => {
            assert_eq!(code, 1400, "ERROR_INVALID_WINDOW_HANDLE")
        }
        other => panic!("无效句柄应报 Failed，实际 {other:?}"),
    }
}

#[cfg(not(target_os = "windows"))]
#[test]
fn non_windows_stub_reports_unsupported() {
    assert_eq!(apply_window_alpha(1, Some(0.5)), AlphaOutcome::Unsupported);
}

/// **受控对照实验**（M116）：同进程造三种窗口样式，逐行读 DWM 属性。
///
/// 为什么是 `#[ignore]`：它要真造窗口（CI 的无桌面环境里跑不出读数），所以只在需要出证据时
/// 手动跑：
///
/// ```text
/// cargo test -p unlucky-app-iced --lib m116 -- --ignored --nocapture
/// ```
///
/// 输出就是报告里那张表（`| style | requested | exstyle | readback | … |`）。
/// **抓不住**：角上到底是透明还是方块（那要合成输出像素，本机 RDP 断连拿不到）。
#[cfg(target_os = "windows")]
#[test]
#[ignore = "要真窗口：手动跑 cargo test -p unlucky-app-iced --lib m116 -- --ignored --nocapture"]
fn m116_corner_controlled_experiment() {
    println!("{}", super::probe::TABLE_HEADER);
    for row in super::controlled_experiment() {
        println!("{}", row.describe());
    }
}

/// **成功判据的语义**：`HRESULT` 看严重性位（bit 31），不是「等于 0」——`S_FALSE`(1) 也算成功。
///
/// ⚠️ 这条测试**不是**「本机 DWM 会返回某个正数」的证据：本机
/// `DwmGetWindowAttribute` 三种窗口样式实测全 `E_INVALIDARG (0x80070057)`（失败）。
/// 下面那个 `0x7FF8_FFA9` 是**合成值**，只用来钉住「非零但严重性位为 0 ⇒ 成功」这条规则，
/// 以及钉住「`unsigned_abs()` 不能用来打印 HRESULT」这个打印坑。
///
/// 抓得住：把成功判据写回 `hr == 0`；把打印改回 `unsigned_abs()`。
/// 抓不住：DWM 在别的机器上返回什么（那是运行期读数）。
#[test]
fn hresult_success_follows_the_severity_bit_not_equality_with_zero() {
    use super::hresult_ok;
    assert!(hresult_ok(0), "S_OK");
    assert!(hresult_ok(1), "S_FALSE 也是成功");
    assert!(
        hresult_ok(0x7FF8_FFA9),
        "合成值：非零但严重性位为 0 ⇒ 按 HRESULT 语义是成功（不是 DWM 的实测返回值）"
    );
    // 失败码：严重性位为 1（i32 负数）。**本机探针实际拿到的就是这个**。
    assert!(!hresult_ok(0x8007_0057_u32 as i32), "E_INVALIDARG");
    assert!(!hresult_ok(-1));
    // **打印规则**：失败码必须印成 `hr as u32` 的原始位。
    // `unsigned_abs()` 会把 `0x8007_0057` 印成 `0x7FF8_FFA9`（M116 踩过，一次误读的源头），
    // 于是「E_INVALIDARG」读起来像「一个信息级成功码」，整个证据链就毁了。
    let e_invalidarg = 0x8007_0057_u32 as i32;
    assert_eq!(e_invalidarg as u32, 0x8007_0057);
    assert_ne!(e_invalidarg.unsigned_abs(), 0x8007_0057);
}

/// **两种「错误号」不同源，打印方式也不同**（tower 裁定 2026-11-03：不许统一）。
///
/// - `GetLastError()` 是 **DWORD**：`87`（`ERROR_INVALID_PARAMETER`）/ `1400`
///   （`ERROR_INVALID_WINDOW_HANDLE`）这类小整数，没有严重性位，`set_layered` 用
///   `raw_os_error().unsigned_abs()` 取的就是它；
/// - `Dwm*WindowAttribute` 返回 **HRESULT**：成功看 bit 31，打印用 `as u32` 原始位。
///
/// 抓得住：有人把 `set_layered` 也改成 `as u32`（或把 DWM 那侧改成 `unsigned_abs`）——
/// 前者对 `87` 无害但对高位被置的错误号会印出负数，后者会把 `E_INVALIDARG` 印成
/// `0x7FF8FFA9`（M116 实测踩过的那个坑）。
#[test]
fn dword_error_codes_and_hresults_are_printed_differently() {
    use super::hresult_ok;

    // DWORD 侧：典型值是小的正数，两种打印结果一致（所以错误只会在高位情形暴露）。
    for dword in [87_i32, 1400] {
        assert_eq!(dword.unsigned_abs(), dword as u32);
        assert!(
            hresult_ok(dword),
            "DWORD 不是 HRESULT，不该拿这个判据去读它"
        );
    }

    // HRESULT 侧：失败码的原始位与 `unsigned_abs()` 必须不同。
    let e_invalidarg = 0x8007_0057_u32 as i32;
    assert!(!hresult_ok(e_invalidarg));
    assert_eq!(e_invalidarg as u32, 0x8007_0057);
    assert_ne!(e_invalidarg.unsigned_abs(), e_invalidarg as u32);
}

/// **产品日志行 `[corner] …` 的真实取值**（同一套判据 + 同一套格式化，句柄换成探针窗口）。
///
/// `excluded_reason=` / `frame=` / `layered=` 三栏是「为什么我们的窗口没有圆角」的唯一现场线索；
/// 报告里要贴真实取值，不是模板。跑法同 `m116`：
///
/// ```text
/// cargo test -p unlucky-app-iced --lib m116 -- --ignored --nocapture
/// ```
#[cfg(target_os = "windows")]
#[test]
#[ignore = "要真窗口：手动跑 cargo test -p unlucky-app-iced --lib m116 -- --ignored --nocapture"]
fn m116_corner_log_line_on_real_windows() {
    use crate::platform::win32::corner::plan_corner;
    use unlucky_presenter::prefs::WindowCorner;

    let os = crate::platform::current_windows_version();
    let plan = plan_corner(WindowCorner::System, os, crate::Backend::Gpu);
    // 直接遍历 `controlled_experiment` 的**同一份**行定义（含 `product` / `product+alpha`），
    // 免得这里另抄一份样式表、跟探针悄悄分叉。
    for row in super::controlled_experiment() {
        let hwnd = super::create_probe_window(row.style, row.ex_style);
        if hwnd == 0 {
            println!("[corner] {} window=none", row.label);
            continue;
        }
        super::show_probe_window(hwnd);
        // layered=on 的那几行（产品的 α<1 档与裸 popup 对照）按裁定 A 带 `suppressed_by=`。
        let layered = row.ex_style & 0x0008_0000 != 0;
        println!(
            "{}",
            crate::platform::win32::corner::format_line(
                row.label,
                &plan,
                WindowCorner::System,
                &super::apply_corner(hwnd, &plan).describe(),
                &super::dwm::style_summary(hwnd),
                layered.then_some("window_alpha"),
            )
        );
        super::destroy_probe_window(hwnd);
    }

    // 设置里选「直角」时的取值（`excluded_reason` 必须变成 `user`）。
    let square = plan_corner(WindowCorner::Square, os, crate::Backend::Gpu);
    println!(
        "[corner] policy=square {}",
        square.log_line(WindowCorner::Square)
    );
}
