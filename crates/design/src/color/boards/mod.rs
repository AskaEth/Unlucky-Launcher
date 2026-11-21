//! # 双主题色板（Ink 深 / Silver 浅）
//!
//! 产品视觉的两套固定色板，来源是**用户自制的规格卡** `DevDocs/artifacts/components-ink.svg`
//! （Ink Cat / M3 Expressive，1480×2260，色值/圆角/尺寸/字号全在源码里）。本模块只做一件事：
//! **把规格卡的色值固化成 49 个 M3 角色 × 2 套**，让 iced 侧拿到的仍是
//! [`Scheme`]——角色名、取色入口、对比度门禁一个都不用换。
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
//! - **`[derive]`**：规格卡没给的角色（`on-secondary`、`surface-container-low` 等），
//!   按「同色相 + M3 的 tone 关系」从邻居推导后**固化成字面量**。推导规则写在
//!   `boards/ink.rs` / `boards/silver.rs` 的模块文档与行注释里，运行期不重算——色板是数据，
//!   不是每次启动现算的函数。
//! - **`[D59]`**：**强调族 26 个角色**（primary/secondary/tertiary 三族的 01~12、`inverse-primary`、
//!   `surface-tint`、`*-fixed*` 12 个）的色相换成**图标紫**（用户 2026-11-18 指令）：
//!   色相取图标采样值 `#3A3170` 的 CAM16 色相 **292.3397°**（tertiary 族 +60° = 352.3397°），
//!   彩度取 M3 Tonal Spot 常数（36 / 16 / 24），**每个角色的 tone 逐条沿用原表**。
//!   只换色相不动明度 ⇒ 相对亮度几乎不变 ⇒ **26 对文字对比度几乎不变**（**逐对最大 |Δ| = 0.056**：
//!   Ink 的 `on-primary-fixed` / `primary-fixed` 13.708 → 13.652，Silver 最大 0.053。最差对
//!   4.982 → 5.001，无一对跌破 AA；差来自 8 bit 量化 + 彩度重取——**不是** ≤0.02）。
//!   这也是这次改动能一条 revert 回去的原因（旧值记在 D59）。
//!   ⚠️ 中性族（`surface` / `surface-container-*`）与 error 族**一个字没动**：Ink 的表面仍是
//!   带绿味的 `#0F150F`（M378 报告「没验的部分」里登记了「紫强调 + 绿黑底」的色相张力未评估）。
//!
//! ## 对比度纪律（两套板同一道门，数字见 `palette::visibility` 的实测表）
//!
//! - 26 组文字/图标对两套板全部 ≥4.5:1（`tests/ink_cat_boards.rs`）；
//! - **状态容器**（primary / secondary / tertiary / error 的 `*-container`）对 `surface` ≥
//!   [`crate::palette::visibility::STATE_CONTAINER_MIN`]（1.20:1）——这是债务台账
//!   「SecondaryContainer 对 Surface 仅 1.06」的令牌层裁决：旧 baseline 的选中行底实测
//!   **1.057:1**（m223 视觉复审），Ink 板抬到 **1.97:1**、Silver 板 **1.23:1**；
//!   Silver 那 1.23 的出处是**tone 关系**不是色相：tone 90 的容器压在 tone 98 的 `surface` 上，
//!   相对亮度由 `L*` 决定（0.808 → 1.229），所以 D59 换色相**不改这一个数**（改前 1.228 /
//!   改后 1.228）。要把它抬离底线只有两条路：把四个容器（含 **error**）一起压到 tone ≤88，
//!   或把 `surface` 提到 `#FFFFFF`——后者会撞上 `surface-container-lowest`（现为 `#FFFFFF`）
//!   让容器阶梯归零，两条都超出「只换强调色」的范围；
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
    /// **Ink Cat**：深色主方案（规格卡 `COLOR ROLES / INK`）。窗口底 `#0F150F`，主色紫（D59）。
    Ink,
    /// **Silver Cat**：浅色主方案（规格卡 `SILVER CAT / LIGHT SCHEME`）。窗口底 `#F9F9FA`，主色紫（D59）。
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

    /// 跨板同值角色名（升序）。抽成函数是为了能给「第 4 个同值」跑反例（见下一条测试）。
    fn cross_board_equal_roles(ink: [Color; 49], silver: [Color; 49]) -> Vec<&'static str> {
        let mut same: Vec<&'static str> = Role::ALL
            .iter()
            .filter(|role| ink[**role as usize] == silver[**role as usize])
            .map(|role| role.name())
            .collect();
        same.sort_unstable();
        same
    }

    /// 两板跨板同值**只允许**这三个角色，别的一律不许。
    ///
    /// - `shadow` / `scrim`：M3 定值纯黑 + 令牌 alpha；
    /// - `secondary-fixed-dim`：两板 tone 都是 80 的紫。M3 的 `*-fixed*` 族本来就「不随明暗改变」，
    ///   两板 tone 相同时同值才是对的——但这是 12 个 fixed 角色里**唯一**一处真碰撞，另外 11 个
    ///   仍各不相同。所以判据按**角色**列白名单，**不能**把 `*-fixed*` 整族豁免（那会白丢 12 条
    ///   断言，「两板 primary-fixed 都写成同值」这种真漂移就会静默通过）。
    const CROSS_BOARD_EQUAL: [&str; 3] = ["scrim", "secondary-fixed-dim", "shadow"];

    #[test]
    fn the_two_boards_are_actually_different() {
        let same = cross_board_equal_roles(BrandBoard::Ink.colors(), BrandBoard::Silver.colors());
        // 白名单式断言：49 个角色里同值的**恰好**是这三个（连同名字）。第 4 个角色一旦收敛——
        // 那正是「换肤会露出旧颜色」——这条当场红，并且把多出来的角色名报出来。
        assert_eq!(same, CROSS_BOARD_EQUAL);
    }

    #[test]
    fn a_fourth_cross_board_equal_role_turns_the_criterion_red() {
        // 反例（塔令：改判据必须配「改坏了就该红」）。把两板的 `primary-fixed` 收敛成同值——
        // 这是最容易真的发生的漂移（两板 fixed 族本来就同色相、差 tone），判据必须把它命名出来，
        // 而不是静默通过。
        let ink = BrandBoard::Ink.colors();
        let mut silver = BrandBoard::Silver.colors();
        silver[Role::PrimaryFixed as usize] = ink[Role::PrimaryFixed as usize];
        assert_eq!(
            cross_board_equal_roles(ink, silver),
            ["primary-fixed", "scrim", "secondary-fixed-dim", "shadow"]
        );
    }
}
