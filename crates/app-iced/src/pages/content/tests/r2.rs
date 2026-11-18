//! **M358 r2**（复审 P2-1 的返工）：列头**档位**必须只由尺寸与令牌决定。
//!
//! v1 把「用哪一档列头」挂在了 `status` / `hits` 上（`plan()` 的早退连 `kind` 一起否决），
//! 于是同一窗口尺寸下列头随数据变高矮：880×560 带提示条时 `Ready` 177、`Loading`/`Empty` 257
//! ⇒ 每换一次筛选/搜索就跳 80px，而这两个态本身也没被修。
//!
//! 这一组用例是本条的判据：档位在**每一个正文高度**上都与数据状态无关。

use unlucky_i18n::Lang;
use unlucky_presenter::dto::ContentKind;

use super::demo;
use super::m358::{bodies, fixture};
use crate::pages::content::layout::{BodyMode, MIN_BODY_HEIGHT};
use crate::pages::content::parts::{self, head};
use crate::pages::content::Status;

/// 演示夹具的三态：有结果 / 搜索中（`pending`，旧结果还在） / 查询无结果。
fn states() -> Vec<(Status, crate::pages::content::Content)> {
    let mut out = Vec::new();
    for status in [Status::Ready, Status::Loading, Status::Empty] {
        let mut model = demo(ContentKind::Mod, Lang::En);
        // `Empty` 在真实路径上就是「没有命中项」：夹具只换了 status，这里补齐这个事实
        let empty = matches!(status, Status::Empty);
        model.status = status.clone();
        if empty {
            model.hits.clear();
        }
        out.push((status, model));
    }
    out
}

/// **M358 r2 判据 1（复审 P2-1）**：**同一尺寸下档位恒定**——`Ready` / `Loading` / `Empty`
/// 三态在 880×560 带提示条（正文 304）都是 `Compact`。
///
/// ## v1 错在哪（这条用例就是它的红报）
///
/// v1 的 `plan()` 用 `if status != Ready || pack_before { return Plan { kind: Full, .. } }`，
/// 把「列表没画 ⇒ 不取整」那条前提**连档位一起否决**了。而列头是**画的**
/// （`view.rs` 无条件 `push(head_view)`），不画的只是列表。于是：
///
/// | 880×560 + 提示条 | status | v1 列头 | 高 |
/// |---|---|---|---|
/// | 有结果 | `Ready` | `Compact` | 177 |
/// | 点一次筛选/搜索（`offset == 0` ⇒ 清 results ⇒ `pending`） | `Loading` | `Full` | 257 |
/// | 查询无结果 | `Empty` | `Full` | 257 |
///
/// ⇒ **每换一次条件，列头跳 80px**，而且 Loading/Empty 这两态根本没被修
/// （列头仍吃 257，Loading 面板只剩 304 − 257 = 47）。
///
/// **改回 v1 的写法 ⇒ 这条必红**，红在第 2、3 个断言上：
/// `left: Full, right: Compact`（Loading / Empty 两态）。
#[test]
fn the_head_tier_is_the_same_in_every_data_state() {
    use head::Kind;

    let (ui, metrics) = fixture();
    let (_, with_banner) = bodies(&ui, metrics);

    let states = states();
    // `Empty` 那一屏真的没有命中项（不然它只是 Ready 的别名）
    assert!(states[2].1.hits.is_empty(), "Empty 态不该有命中项");
    assert!(
        !states[1].1.hits.is_empty(),
        "Loading 态要留着旧结果（危险条件）"
    );

    for (status, model) in &states {
        assert_eq!(
            head::plan(&ui, model, BodyMode::ScrollWhole, with_banner).kind,
            Kind::Compact,
            "{status:?} 态在正文 {with_banner} 下的列头档与其它态不一致（v1 的 P2-1）"
        );
    }
    // 无提示条 380：三态都保持常规档（反过来也要恒定，不然只是把跳变挪到另一个尺寸）
    let (bare, _) = bodies(&ui, metrics);
    for (status, model) in &states {
        assert_eq!(
            head::plan(&ui, model, BodyMode::ScrollWhole, bare).kind,
            Kind::Full,
            "{status:?} 态在正文 {bare} 下应当保持常规列头"
        );
    }
}

/// **M358 r2 判据 2（跳变消失）**：在**每一个**正文高度上，三态的档位都相同 ——
/// 所以「换条件时列头跳一下」在整个 `ScrollWhole` 区间里都不可能发生。
///
/// 依据是结构而不是「图上看不出来」：`kind_of` 只读 `full_head_max(ui)`（提示行与计数行
/// **都算上**的列头上限）、`one_card(ui)`（`ListItemGeometry` 的三行档 + 行距，不读 `hits`）
/// 与 `pack_before`，三者都不随查询状态变 ⇒ 同一个 `(尺寸, 档位)` 只有一个答案。
/// v1 的写法会让这条在 304 附近整段红（`Ready` 收、其它两态不收）。
#[test]
fn the_head_tier_never_jumps_at_one_size() {
    use head::Kind;

    let (ui, metrics) = fixture();
    let (bare, with_banner) = bodies(&ui, metrics);

    let models: Vec<_> = states().into_iter().map(|(_, model)| model).collect();

    // 逐像素扫整个 `ScrollWhole` 区间（0..MIN_BODY_HEIGHT），外加两个真实尺寸点
    let bodies: Vec<f32> = (0..=MIN_BODY_HEIGHT as u32)
        .map(|body| body as f32)
        .chain([bare, with_banner])
        .collect();
    for body in bodies {
        let kinds: Vec<Kind> = models
            .iter()
            .map(|model| head::plan(&ui, model, BodyMode::ScrollWhole, body).kind)
            .collect();
        assert!(
            kinds.iter().all(|kind| *kind == kinds[0]),
            "正文 {body} 时三态的列头档不一致（{:?}）：换条件会跳变",
            kinds
        );
    }
}

/// 档位判据的两个入参都**不读数据**：`one_card` 与 `row_pitches` 同源（同一套
/// `ListItemGeometry` + `ROW_GAP`），但结果为空时仍然是 80 而不是 0。
#[test]
fn the_card_pitch_of_the_tier_decision_does_not_read_the_hits() {
    let (ui, _) = fixture();
    let ready = demo(ContentKind::Mod, Lang::En);
    let from_hits: f32 = parts::row_pitches(&ui, &ready).take(2).sum();

    let mut empty = demo(ContentKind::Mod, Lang::En);
    empty.status = Status::Empty;
    empty.hits.clear();
    assert_eq!(
        parts::row_pitches(&ui, &empty).take(2).sum::<f32>(),
        0.0,
        "`row_pitches` 读 hits：空结果给 0（所以它不能拿来做档位判据）"
    );

    let (_, with_banner) = bodies(
        &ui,
        crate::pages::layout::frame_metrics(crate::pages::layout::PaneKit::Single, 880.0),
    );
    // 档位对「有没有结果」不敏感：空结果与有结果在同一个正文高下选同一档
    for body in [200.0, with_banner, 380.0, 460.0] {
        assert_eq!(
            head::plan(&ui, &ready, BodyMode::ScrollWhole, body).kind,
            head::plan(&ui, &empty, BodyMode::ScrollWhole, body).kind,
            "正文 {body}：空结果与有结果选了不同的列头档"
        );
    }
    // 而且卡节距与 `row_pitches` 同值（80），只是不读模型
    assert_eq!(from_hits, 80.0, "演示夹具是三行档卡 76 + 行距 4");
}

/// **M358 r3**：`full_head_max` 比**实际**列头最多高多少 —— 逐项钉住「最坏 48」。
///
/// r2 的文档把它写成 24（还写成「两行 + 两条 `Space::X1`」）：两条都错——`Space::X2` 是 **8**
/// 不是 4，而且间距的条数 = 非零行数，**缺一行就少一条**。于是每缺一行是
/// `行高 16 + 一条间距 8 = 24`，两行都缺是 **48**。
///
/// 不改代码也不会红的那种数，正是 r2 让「24」活下来的原因；这条用例读生产路径的
/// [`head::full_head_max`] / [`head::chrome_of`] 逐态相减，写错就红。
#[test]
fn the_tier_threshold_never_over_estimates_by_more_than_two_lanes() {
    use unlucky_design::spacing::Space;
    use unlucky_design::typography::TypeRole;

    let (ui, _) = fixture();
    let max = head::full_head_max(&ui);
    let gap = ui.space(Space::X2);
    // 一行 = 它的行高 + 它那一条间距（提示行 `BodySmall` 与计数行 `LabelMedium` 同是 16）
    assert_eq!(TypeRole::BodySmall.style().line_height_px, 16.0);
    assert_eq!(TypeRole::LabelMedium.style().line_height_px, 16.0);
    let lane = TypeRole::BodySmall.style().line_height_px + gap;
    assert_eq!(lane, 24.0, "一行的代价 = 行高 16 + 一条 Space::X2 8");

    // 提示行在 ⇔ CF 没 key；计数行在 ⇔ Ready。四种组合各算一遍。
    for (curseforge, status, missing) in [
        (false, Status::Ready, 0.0),
        (true, Status::Ready, lane),
        (false, Status::Empty, lane),
        (true, Status::Empty, 2.0 * lane),
    ] {
        let mut model = demo(ContentKind::Mod, Lang::En);
        model.curseforge = curseforge;
        model.status = status.clone();
        let actual = head::chrome_of(&ui, &model, head::Kind::Full);
        assert_eq!(
            max - actual,
            missing,
            "curseforge={curseforge} status={status:?}：门槛比实际列头高出的量不对"
        );
    }

    // 最坏那一档的绝对值：281 − 233 = 48（233 = 搜索 56 + 四排 152 + 分隔线 1 + 3×8）
    let mut worst = demo(ContentKind::Mod, Lang::En);
    worst.curseforge = true;
    worst.status = Status::Empty;
    assert_eq!(max, 281.0, "列头上限（两行都在的那一版）");
    assert_eq!(head::chrome_of(&ui, &worst, head::Kind::Full), 233.0);
    assert_eq!(
        max - head::chrome_of(&ui, &worst, head::Kind::Full),
        48.0,
        "最坏 48（不是 r2 文档写的 24）"
    );
}
