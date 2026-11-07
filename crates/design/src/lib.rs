//! # unlucky-design · ULL 设计令牌唯一真源
//!
//! 视觉基线 = **Material 3 Expressive**（D26 锁定，风格与框架不再更换）。这个 crate 只装
//! **渲染无关的数据与纯函数**：色彩角色、排版 ramp、形状刻度、高程、动效参数、图标几何。
//! 它不依赖任何 UI / 渲染框架（iced / slint / winit / wgpu / gtk …），也不依赖任何第三方 crate，
//! 因此 iced 迁移、CLI、甚至离屏截图工具都能吃同一套值——这是「UI 整体性」的地基。
//!
//! ## 边界（谁负责什么）
//!
//! | 层 | 负责 | 不负责 |
//! |---|---|---|
//! | `unlucky-design`（本 crate） | 令牌的值与它们之间的纯函数关系（tone→色、type role→字号/行高/字重、shape→半径、spring→时长） | 布局、控件、绘制、字体加载、图片、动画驱动 |
//! | iced 适配层（后续 mission） | 把令牌翻成 iced 的类型（`Color` / `Length` / `Font` / 动画时钟） | 自己发明新令牌名或新取值 |
//! | 页面 mission | 消费令牌与 [`component_map`] 的档位约定 | 硬编码色值 / 字号 / 半径 / 时长 |
//!
//! ## 分模块
//!
//! - [`color`]：M3 语义色角色（49 个）× light/dark 双套 + tone 调色板（**真 HCT**：CAM16 色相/彩度
//!   + CIELAB `L*`，官方 `ColorSpec2021` tone 表，与官方 Java 实现逐通道对齐——口径见模块文档）
//! - [`palette`]：配色来源（D38/D40）——图片像素 → 种子 → 完整方案（Monet 式量化 + 打分）、
//!   内置色卡、三级优先与失败回落、对比度收敛。**只吃已解码的像素**，解码在 `crates/palette-extract`
//! - [`typography`]：M3 type scale 15 档 + 三平台字体族回退栈
//! - [`shape`]：M3 Expressive 形状刻度 + shape morph 数据 + [`shape::ComponentRadius`]（微控件圆角层，D32）
//! - [`elevation`]：elevation level 与 surface tint / surface container 规则
//! - [`motion`]：时长、easing（可采样）、Expressive 弹簧参数
//! - [`spacing`]：4px 栅格、骨架尺寸（原 `metrics.slint` 的框架无关部分）、[`stroke::HAIRLINE`]（一像素线）
//! - [`icons`]：24 网格图标几何（SVG path 字符串）+ 描边语义
//! - [`component_map`]：组件 → 档位映射表（哪个组件用哪档形状/高程/动效/字体）
//!   + [`component_map::ComponentMetrics`]（控件内部微几何，与 `LayoutMetrics` 分工见该模块文档）
//!
//! ## 最小用法
//!
//! ```
//! use unlucky_design::color::{Role, Scheme, SchemeFlavor};
//! use unlucky_design::typography::{FontFamily, Platform, TypeRole};
//!
//! let light = Scheme::baseline(SchemeFlavor::Light);
//! let ratio = light.contrast(Role::OnSurface, Role::Surface);
//! assert!(ratio >= 4.5, "正文对表面必须满足 WCAG AA");
//!
//! let title = TypeRole::TitleLarge.style();
//! assert_eq!(title.size_px, 22.0);
//! let stack = Platform::current().font_stack(FontFamily::Display);
//! assert!(!stack.is_empty());
//! ```
//!
//! ## baseline MD3 vs M3 Expressive 差异清单
//!
//! 「M3 Expressive」不是「M3 换皮」——它真的加了一批令牌。本 crate 两代都提供，差异如下
//! （来源等级：**A** = androidx 源码 / 官方 token 文件；**B** = m3.material.io 文档表格；**C** = 社区转述）。
//!
//! | 领域 | baseline MD3 | M3 Expressive 新增/不同 | 本 crate 落点 | 等级 |
//! |---|---|---|---|---|
//! | 排版 | 15 档 type role，只有 Regular/Medium 两种字重档 | **每档多一条 `*Emphasized`**：字号/行高不变，字重升到 Medium(500) 或 Bold(700)，字距按表微调（display-large −0.2→0、body-large 0.5→0.15、title-medium 0.2→0.15、body-medium 0.2→0.25） | [`typography::TypeRole::emphasized`]（官方值）；[`typography::TypeRole::emphasized_simplified`] 是我们的「只加一档字重」简化规则，**非官方** | A |
//! | 形状 | ExtraSmall 4 / Small 8 / Medium 12 / Large 16 / **ExtraLarge 28** / Full | 多出 **LargeIncreased 20 / ExtraLargeIncreased 32 / ExtraExtraLarge 48**，并引入 **shape morph**（按下/选中时档位之间弹性过渡） | [`shape::ShapeScale`]（10 档）+ [`shape::ShapeMorph`] + [`component_map`] 的 `morph` 列 | A |
//! | 动效 | 16 档 duration + 7 条 cubic-bezier easing（时间驱动） | 把弹簧打包成 **standard / expressive 两套方案**，每套各有 spatial（位移，会回弹）与 effects（颜色/透明度，不回弹）× fast/default/slow。standard spatial ζ=0.9（k 1400/700/300），expressive spatial ζ=0.6~0.8（k 800/380/200），effects 两套同值 ζ=1.0（k 3800/1600/800） | [`motion::spring`] 的 **方案 × 类别 × 速度**二维表（12 格，`StandardMotionTokens`/`ExpressiveMotionTokens` 原值）+ 9 个具名常量；duration/easing 两代共用 | A |
//! | 组件尺寸 | 触控目标 48dp（无障碍下限本来就是 48） | Expressive 把按钮/列表的整体节奏放大（列表项 56/72/88），并明确 48dp 为**可点下限** | [`spacing::LayoutMetrics::DESKTOP`]：`touch_target = 48`（照规范）+ `control_height = 40`（可见高度，上下各 4px 内边距凑满 48）+ `row_height`/`nav_item_height` = 44、`titlebar_height` = 48（M63 §1.4+R13 的桌面档） | A/B |
//! | 字体 | 只有 400/500/700 的「概念」字重，没规定字体族要覆盖几档 | Expressive 的强调档**真的会要 700**（label-*、title-medium/small），中文字体覆盖差异巨大：雅黑只有 400/700（无 500）、苹方到 600 断（无 700）、Noto/思源可变字体全覆盖 | [`typography::FontFace`] 的 `WeightCoverage`（可变区间 / 静态表）+ [`typography::Platform::resolve_weight`] 纯函数给出「原生 / 最近档 / 合成加粗」三档结论 | B |
//! | 色彩 | 49 个角色（primary/secondary/tertiary/error 族、surface-container 系、`*-fixed*` 系、outline、inverse、scrim/shadow） | **没有新增颜色角色**；变化在用法：surface-container 系（Lowest…Highest）取代旧的 elevation tint 叠色，表达层级 | [`color::Role`] 全 49 个；[`elevation::Elevation::surface_role`] 给出层级映射，`deprecated_tint_alpha` 保留旧口径作对照 | A |
//! | 高程 | Level0..5 = 0/1/3/6/8/12dp | 数值不变；组件默认值不同（卡片 L0、菜单 L2、对话框 L3，见 [`component_map`]） | [`elevation::Elevation`] | A |
//!
//! ### 两条由此衍生的项目纪律
//!
//! 1. **shape morph 是克制的状态信号，不是每屏装饰**：Expressive 官方鼓励「形状即状态」，但
//!    桌面端信息密度高，圆角到处变会显得凌乱（社区已有人批评控制中心这么做）。本项目的约束是
//!    **一屏最多 1 处形变、且只用于状态切换信号**；静态元素一律用 [`shape::ShapeScale`] 的固定档。
//!    [`component_map`] 的 `morph` 列就是允许清单——不在表里的组件不许自己加形变。
//! 2. **中文字重必须真的有三档**：Expressive 的 700 落在中文字体上，雅黑缺 500、苹方缺 700，
//!    缺档就走「合成加粗」= 观感廉价。所以字体栈带 `WeightCoverage`，并在
//!    [`typography::Platform::resolve_weight`] 里把降级路径显式化（合成加粗是最后一档）。
//!
//! 已知与文档的 0.05px 级差异：Compose 这一代 `TypeScaleTokens` 的 tracking 与 m3.material.io
//! 网页表格不同（display-large −0.2 vs −0.25、body-medium 0.2 vs 0.25、title-medium 0.2 vs 0.15），
//! 本项目**跟 Compose**（那是真正渲染出来的值，A 级）。
//!
//! ## 未支持的官方能力（gap 登记，不要以为已经有了）
//!
//! - **对比度等级**：Android 16 的「默认 / 中 / 高」三档对比度本 crate **不支持**。将来扩法是
//!   **按等级偏移角色 tone 表**（例如高对比把 `on-*` 与承载色的 tone 差拉大 5~10），
//!   而不是新增一批角色——`Role` 的数量保持 49 不变，`Scheme` 多一个 `contrast_level` 维度。
//!   （M63 的 G4 只说「高对比模式走 CPU 档」，那是渲染档位，不等价于支持对比度等级。）
//! - **动态壁纸取色**：M3 的多色种子派生（一张图出 4 个 source color、`*_container` 各自换色相）
//!   未实现。本模块只取**一个**主色种子（D38 的产品口径：整套主题一个强调色），
//!   实现落在 [`palette`]。
//! - **M3 的 elevation tint 叠色**：现行 M3 已改用 surface-container 表达层级，旧的
//!   `deprecated_tint_alpha()` 只为读旧稿保留。

#![forbid(unsafe_code)]

pub mod color;
pub mod component_map;
pub mod elevation;
pub mod icons;
pub mod motion;
pub mod palette;
pub mod shape;
pub mod spacing;
pub mod typography;

pub use color::{Color, Role, Scheme, SchemeFlavor};
pub use motion::MotionScale;
pub use palette::{Card, CardKey, Level as SourceLevel, Provenance, Seed};
pub use spacing::stroke;
pub use typography::TypeRole;

/// 设计令牌的整体入口：一份「当前该长什么样」的聚合快照。
///
/// iced 适配层只持有这个结构（深浅切换 = 重新 `DesignTokens::baseline(mode)`），
/// 不要在适配层里缓存散装色值——那样深浅热切会有一半画对一半画错。
#[derive(Debug, Clone, Copy)]
pub struct DesignTokens {
    /// 当前深浅模式。
    pub flavor: SchemeFlavor,
    /// 当前模式的 M3 语义色角色全表。
    pub colors: Scheme,
    /// 动效时长的全局缩放（`1.0` = 标准；用户关掉动画时适配层传 `0.0`）。
    pub motion_scale: MotionScale,
}

impl DesignTokens {
    /// 按模式给一套**产品默认**令牌：种子 `#3C8DBC`（M70 ④ 天蓝，用户 2026-11-01 拍板）
    /// + 中性彩度 12，即 `color::palettes::DEFAULT_SEED` / `DEFAULT_NEUTRAL_CHROMA`。
    ///
    /// 想要**官方 M3 baseline** 口径（种子 `#6750A4` + 中性彩度 6 / neutral-variant 8）请走
    /// `color::Scheme::official_baseline(flavor)`——两条路径的算法完全相同，只差入参。
    pub fn baseline(flavor: SchemeFlavor) -> Self {
        Self {
            flavor,
            colors: Scheme::baseline(flavor),
            motion_scale: MotionScale::STANDARD,
        }
    }

    /// 按用户强调色重建一套令牌（表面中性色仍走 baseline，强调色只驱动 primary/secondary/tertiary）。
    pub fn with_accent(flavor: SchemeFlavor, accent: Color) -> Self {
        Self {
            flavor,
            colors: Scheme::from_accent(flavor, accent),
            motion_scale: MotionScale::STANDARD,
        }
    }

    /// 便捷取色：`tokens.color(Role::Surface)`。
    pub fn color(&self, role: Role) -> Color {
        self.colors.get(role)
    }
}
