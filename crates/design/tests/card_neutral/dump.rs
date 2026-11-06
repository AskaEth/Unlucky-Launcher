//! 出图脚本读的 hex 行（无断言，纯打印）+ D43 的实测表（本文件就是那张表的来源）。
//!
//! 跑法：`cargo test -p unlucky-design --test card_neutral -- --nocapture`
//!
//! ## 改前 / 改后 `surface` 实测（2026-11）
//!
//! | 卡 | 改前 `surface`(浅) | 改后 `surface`(浅) | 改前 HCT(浅) | 改后 HCT(浅) |
//! |---|---|---|---|---|
//! | sky | `#F6FAFF` | `#F6FAFF` | 236.86° / 6.41 | 236.86° / 6.41 |
//! | lavender | `#FDF7FF` | `#F6FAFF` | 298.10° / 5.87 | 236.86° / 6.41 |
//! | forest | `#EEFDEE` | `#F6FAFF` | 157.01° / 11.77 | 236.86° / 6.41 |
//! | amber | `#FFF8F4` | `#F6FAFF` | 71.48° / 1.89 | 236.86° / 6.41 |
//! | rose | `#FFF8F7` | `#F6FAFF` | 22.09° / 1.29 | 236.86° / 6.41 |
//! | teal | `#EBFEF8` | `#F6FAFF` | 182.42° / 12.03 | 236.86° / 6.41 |
//! | dusk | `#FBF8FF` | `#F6FAFF` | 280.31° / 5.44 | 236.86° / 6.41 |
//! | olive | `#F9FBE1` | `#F6FAFF` | 120.99° / 11.84 | 236.86° / 6.41 |
//!
//! 深色档同理：改前 `#09151C`~`#1F0F10`、彩度 11.63~12.45，改后八张卡统一 `#09151C`
//! （235.70° / 11.70）。
//!
//! ## 这条用例为什么没有断言
//!
//! 它是**数据导出**（`CARD|` 行给 `/d/tmp/m88/render.py` 排版成验收图），断言在同目录的
//! `card_neutral.rs` 里。放这里是为了让根文件留在 ≤400 行以内。

use super::common::{before_d43, flavors};
use unlucky_design::color::Role;
use unlucky_design::palette::CARDS;

/// 出图脚本读的 hex 行（改前 + 改后并排），跑法：
/// `cargo test -p unlucky-design --test card_neutral -- --nocapture`。
///
/// 格式 `CARD|key|flavor|before|role=hex;role=hex;…`（承接那张「8 张卡 surface 色块并排 +
/// 切 sky→teal 窗口底色」的验收图；渲染脚本 `/d/tmp/m88/render.py` 只做排版，数据全在这里）。
#[test]
fn dump_card_palette_for_the_color_card() {
    const SHOWN: [Role; 10] = [
        Role::Surface,
        Role::OnSurface,
        Role::OnSurfaceVariant,
        Role::SurfaceVariant,
        Role::SurfaceContainer,
        Role::SurfaceContainerHigh,
        Role::Outline,
        Role::Primary,
        Role::OnPrimary,
        Role::PrimaryContainer,
    ];
    for card in CARDS {
        for flavor in flavors() {
            for (tag, scheme) in [
                ("before", before_d43(card, flavor)),
                ("after", card.scheme(flavor)),
            ] {
                let pairs: Vec<String> = SHOWN
                    .iter()
                    .map(|role| format!("{}={}", role.name(), scheme.get(*role).to_hex()))
                    .collect();
                println!(
                    "CARD|{}|{}|{tag}|{}",
                    card.key,
                    flavor.name(),
                    pairs.join(";")
                );
            }
        }
    }
}
