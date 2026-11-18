//! 加载器词表（磁贴徽标 + 向导可选）与 [`LoaderInfo`]。

use serde::{Deserialize, Serialize};

use unlucky_core::game::LoaderKind as CoreLoaderKind;

/// 加载器种类（磁贴徽标 + 详情页版本链）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LoaderKind {
    /// 原版（无加载器）
    Vanilla,
    /// Fabric
    Fabric,
    /// Quilt
    Quilt,
    /// Forge
    Forge,
    /// NeoForge
    NeoForge,
    /// OptiFine
    OptiFine,
    /// LiteLoader
    LiteLoader,
}

impl LoaderKind {
    /// 徽标显示文字。
    pub fn label(self) -> &'static str {
        match self {
            Self::Vanilla => "原版",
            Self::Fabric => "Fabric",
            Self::Quilt => "Quilt",
            Self::Forge => "Forge",
            Self::NeoForge => "NeoForge",
            Self::OptiFine => "OptiFine",
            Self::LiteLoader => "LiteLoader",
        }
    }

    /// 本 DTO 种类 → core 的宽种类。
    ///
    /// **结构映射，一个 id 字面量都没有**：词表只有一个真相源
    /// （[`CoreLoaderKind::as_str`]），本 crate 写不进第二份。加一支 DTO 种类这里编不过。
    const fn to_core(self) -> CoreLoaderKind {
        match self {
            Self::Vanilla => CoreLoaderKind::Vanilla,
            Self::Fabric => CoreLoaderKind::Fabric,
            Self::Quilt => CoreLoaderKind::Quilt,
            Self::Forge => CoreLoaderKind::Forge,
            Self::NeoForge => CoreLoaderKind::NeoForge,
            Self::OptiFine => CoreLoaderKind::OptiFine,
            Self::LiteLoader => CoreLoaderKind::LiteLoader,
        }
    }

    /// core 的宽种类 → 本 DTO 种类（[`Self::to_core`] 的反向，同样不含字面量）。
    ///
    /// core 给 `game::LoaderKind` 加一支、而 DTO 还没跟上时，这里编不过——这正是要的：
    /// UI 不许「新加载器默默显示成原版」。
    const fn from_core(kind: CoreLoaderKind) -> Self {
        match kind {
            CoreLoaderKind::Vanilla => Self::Vanilla,
            CoreLoaderKind::Fabric => Self::Fabric,
            CoreLoaderKind::Quilt => Self::Quilt,
            CoreLoaderKind::Forge => Self::Forge,
            CoreLoaderKind::NeoForge => Self::NeoForge,
            CoreLoaderKind::OptiFine => Self::OptiFine,
            CoreLoaderKind::LiteLoader => Self::LiteLoader,
        }
    }

    /// 稳定 id（`vanilla` / `fabric` / …；GUI 与 `--json` 同词表）。
    ///
    /// **委派**给 [`CoreLoaderKind::as_str`]（M126）：词表的源头表在 core（0.2.0 已发布契约；
    /// 另一处逐字副本是 `ull help instances` 的词表行），这里只剩一次结构映射。
    #[must_use]
    pub fn as_str(self) -> &'static str {
        self.to_core().as_str()
    }

    /// 从稳定 id 解析（未知给 `None`，由调用方决定报错文案）。
    ///
    /// 七个 id 的匹配委派给 [`CoreLoaderKind::parse`]；本方法只额外收 `none` / 空串这两个
    /// **GUI 输入别名**（表单里「不装加载器」的写法）——它们不是对外输出的 id，也绝不参与
    /// [`Self::as_str`]。
    pub fn parse(raw: &str) -> Option<Self> {
        let wanted = raw.trim();
        if wanted.is_empty() || wanted.eq_ignore_ascii_case("none") {
            return Some(Self::Vanilla);
        }
        Some(Self::from_core(CoreLoaderKind::parse(wanted)?))
    }

    /// 向导里的可选加载器（顺序即展示顺序）。
    ///
    /// 六选 = 原版 + 五家能装的。OptiFine 仍**不在这里**：它要跑外部 `optifine.Patcher` 并原地
    /// 改 zip，core 侧至今没有安装路线（见 `DevDocs/artifacts/m218/SURVEY-p1.md` §3），
    /// 放进向导只会让用户选完才失败。
    pub fn wizard_choices() -> [Self; 6] {
        [
            Self::Vanilla,
            Self::Fabric,
            Self::Quilt,
            Self::Forge,
            Self::NeoForge,
            Self::LiteLoader,
        ]
    }
}

/// 加载器 + 版本（例如 Fabric 0.16.5）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoaderInfo {
    /// 加载器种类
    pub kind: LoaderKind,
    /// 加载器版本号；原版为空串
    pub version: String,
}

impl Default for LoaderInfo {
    fn default() -> Self {
        Self {
            kind: LoaderKind::Vanilla,
            version: String::new(),
        }
    }
}

impl LoaderInfo {
    /// 徽标文字：原版只写「原版」，装加载器时写「Fabric 0.16.5」。
    pub fn badge(&self) -> String {
        if self.kind == LoaderKind::Vanilla || self.version.is_empty() {
            self.kind.label().to_string()
        } else {
            format!("{} {}", self.kind.label(), self.version)
        }
    }
}
