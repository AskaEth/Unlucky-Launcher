//! # 双主题色板（Ink 深 / Silver 浅）
//!
//! 产品视觉的两套固定色板，来源是**用户自制的规格卡** `DevDocs/artifacts/components-ink.svg`
//! （Ink Cat / M3 Expressive，1480×2260，色值/圆角/尺寸/字号全在源码里）。本模块只做一件事：
//! **把规格卡的色值固化成 49 个 M3 角色 × 2 套**，让 iced 侧拿到的仍是
//! [`Scheme`](crate::color::Scheme)——角色名、取色入口、对比度门禁一个都不用换。
//!
//! ## 两层配色来源，互不顶替
//!
//! | 来源 | 入口 | 什么时候用 |
//! |---|---|---|
//! | **品牌色板**（本模块） | [`BrandBoard`] → [`Scheme::from_board`](crate::color::Scheme::from_board) | 出厂外观：Ink = 深色主方案、Silver = 浅色主方案 |
//! | **tone 派生**（既有） | [`Scheme::baseline`](crate::color::Scheme::baseline) / [`Scheme::from_accent`](crate::color::Scheme::from_accent) | D38 取色链（背景图 / 壁纸取色、用户自选强调色） |
//!
//! **为什么保留 `baseline` 而不是把它换成色板**（「保留旧令牌名兼容层」这一选，理由）：
//!
//! 1. `Scheme::baseline` 是 D38 取色链的**中性锚点**：`from_accent` 的全部语义就是「表面色与
//!    baseline 一致、只换强调族」。把 baseline 改成固定板，取色链要么改语义、要么在板色上再叠
//!    一层 tone 派生，两者都会让「换强调色不该让窗口底色变脏」这条产品口径失效。
//! 2. 改 `baseline` 的色值等于**同时改 app-iced 的观感**（`theme::Ui::new` 直接吃 baseline），
//!    而换肤（含截图验收）归 M229；令牌层先把两套板备好，再让壳层一次性切过去，比「两处各改一半」
//!    可控。
//! 3. 兼容层是**零成本**的：`Role` 名字与 49 个下标一个没动，新增的是并列入口而不是替换入口。
//!
//! ## 「跟随系统」是第三种设置态，不是第三套色板
//!
//! 外观设置有三态：浅色 / 深色 / 跟随系统。前两态直接选板，第三态按系统外观落到
//! [`BrandBoard::for_flavor`]。色板本身只有两套，**不要**再加一个 `System` 变体——那会让
//! 「两套板」变成「三套板」，而系统外观随时会变，色板却应该是静态的。
//!
//! ## 色值来源分两类（`[spec]` / `[derive]`）
//!
//! - **`[spec]`**：规格卡直写。每块的 [`BrandBoard::anchors`] 表就是这份清单，测试逐条比对 hex，
//!   所以「规格卡改了但代码没改」会红。
//! - **`[derive]`**：规格卡没给的角色（`on-secondary`、`surface-container-low`、`*-fixed*` 等），
//!   按「同色相 + M3 的 tone 关系」从邻居推导后**固化成字面量**。推导规则写在
//!   `boards/ink.rs` / `boards/silver.rs` 的模块文档与行注释里，运行期不重算——色板是数据，
//!   不是每次启动现算的函数。
//!
//! ## 对比度纪律（两套板同一道门，数字见 `palette::visibility` 的实测表）
//!
//! - 26 组文字/图标对两套板全部 ≥4.5:1（`tests/ink_cat_boards.rs`）；
//! - **状态容器**（primary / secondary / tertiary / error 的 `*-container`）对 `surface` ≥
//!   [`crate::palette::visibility::STATE_CONTAINER_MIN`]（1.20:1）——这是债务台账
//!   「SecondaryContainer 对 Surface 仅 1.06」的令牌层裁决：旧 baseline 的选中行底实测
//!   **1.057:1**（m223 视觉复审），Ink 板抬到 **1.97:1**、Silver 板 **1.23:1**；
//! - **层级容器**（`surface-container-*`）不参与那条数字门禁，只要求 L\\* 沿阶梯单调、相邻步长
//!   ≥ [`crate::palette::visibility::LAYER_STEP_MIN_LSTAR`]。M3 的容器层天生贴在一起
//!   （Silver 的状态容器压在容器层上实测最低 **1.02**、Ink 最低 1.54，见 `palette::visibility`
//!   的实测表），所以裁决第 3 条是**构成要求**：选中态必须「容器底 + 文字对比 + 形状/图标/描边」
//!   三件套，不许只靠底色差。

mod ink;
mod silver;

use super::{Color, Role, Scheme, SchemeFlavor};

/// 产品色板（两套，与深浅模式一一对应）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BrandBoard {
    /// **Ink Cat**：深色主方案（规格卡 `COLOR ROLES / INK`）。窗口底 `#0F150F`，主色淡绿。
    Ink,
    /// **Silver Cat**：浅色主方案（规格卡 `SILVER CAT / LIGHT SCHEME`）。窗口底 `#F9F9FA`，主色钢蓝。
    Silver,
}

impl BrandBoard {
    /// 全部色板（由深到浅）。
    pub const ALL: [BrandBoard; 2] = [BrandBoard::Ink, BrandBoard::Silver];

    /// 稳定字符串名（`ink` / `silver`），写配置与日志用。
    pub const fn name(self) -> &'static str {
        match self {
            BrandBoard::Ink => "ink",
            BrandBoard::Silver => "silver",
        }
    }

    /// 该板对应的深浅模式（Ink = 深、Silver = 浅）。
    pub const fn flavor(self) -> SchemeFlavor {
        match self {
            BrandBoard::Ink => SchemeFlavor::Dark,
            BrandBoard::Silver => SchemeFlavor::Light,
        }
    }

    /// 按深浅模式选板——这就是「跟随系统」的落点。
    pub const fn for_flavor(flavor: SchemeFlavor) -> Self {
        match flavor {
            SchemeFlavor::Dark => BrandBoard::Ink,
            SchemeFlavor::Light => BrandBoard::Silver,
        }
    }

    /// 49 个角色的色值表（下标 = `Role as usize`）。
    pub const fn colors(self) -> [Color; 49] {
        match self {
            BrandBoard::Ink => ink::COLORS,
            BrandBoard::Silver => silver::COLORS,
        }
    }

    /// 直接取角色色值。
    pub const fn get(self, role: Role) -> Color {
        self.colors()[role as usize]
    }

    /// 规格卡直写的色值清单（`[spec]` 行），出处对账与文档生成用。
    pub const fn anchors(self) -> &'static [(Role, &'static str)] {
        match self {
            BrandBoard::Ink => &ink::ANCHORS,
            BrandBoard::Silver => &silver::ANCHORS,
        }
    }

    /// 这套色板对应的完整语义方案。
    pub fn scheme(self) -> Scheme {
        Scheme::from_board(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn boards_map_to_flavors_both_ways() {
        for board in BrandBoard::ALL {
            assert_eq!(BrandBoard::for_flavor(board.flavor()), board);
        }
        assert_eq!(
            BrandBoard::for_flavor(SchemeFlavor::Light),
            BrandBoard::Silver
        );
        assert_eq!(BrandBoard::for_flavor(SchemeFlavor::Dark), BrandBoard::Ink);
    }

    #[test]
    fn names_are_unique_and_ascii() {
        let mut seen = BTreeSet::new();
        for board in BrandBoard::ALL {
            let name = board.name();
            assert!(seen.insert(name), "色板名重复：{name}");
            assert!(name.chars().all(|c| c.is_ascii_lowercase()));
        }
    }

    #[test]
    fn every_role_is_opaque_on_both_boards() {
        for board in BrandBoard::ALL {
            for role in Role::ALL {
                assert!(
                    board.get(role).is_opaque(),
                    "{} 的 {} 不是不透明色",
                    board.name(),
                    role.name()
                );
            }
        }
    }

    #[test]
    fn anchors_are_a_subset_of_the_board() {
        for board in BrandBoard::ALL {
            for (role, hex) in board.anchors() {
                assert_eq!(
                    board.get(*role),
                    Color::from_hex(hex).expect("规格卡色值"),
                    "{} 的 {} 与规格卡 {hex} 不符",
                    board.name(),
                    role.name()
                );
            }
        }
    }

    #[test]
    fn scheme_flavor_follows_the_board() {
        for board in BrandBoard::ALL {
            assert_eq!(board.scheme().flavor(), board.flavor());
        }
        // 深板确实深、浅板确实浅（不是两个同义词）。
        let ink = BrandBoard::Ink.get(Role::Surface).relative_luminance();
        let silver = BrandBoard::Silver.get(Role::Surface).relative_luminance();
        assert!(ink < 0.02, "Ink 的窗口底太亮：{ink}");
        assert!(silver > 0.9, "Silver 的窗口底太暗：{silver}");
    }

    #[test]
    fn the_two_boards_are_actually_different() {
        let ink = BrandBoard::Ink.colors();
        let silver = BrandBoard::Silver.colors();
        let same = Role::ALL
            .iter()
            .filter(|role| ink[**role as usize] == silver[**role as usize])
            .count();
        // 只有 shadow / scrim（纯黑，M3 定值）该跨板同值——其余都得变。
        assert_eq!(same, 2, "跨板同值的角色数应从 2（shadow/scrim）开始算");
    }
}
