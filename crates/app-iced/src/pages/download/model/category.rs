//! 分类筛选（原顶部四个 chip / 现在是第一级侧栏的四个固定项）。
//!
//! 从 `model.rs` 搬出来：M105/M107 合流后那边到 441 行（超 400 硬线）。
//! **纯搬移**：枚举、`matches` 谓词、`command()` 与 `of()` 一字未改，
//! 路径仍由 `model.rs` 的 `pub use` 保持 `pages::download::Category`。

use unlucky_i18n::Key;
use unlucky_presenter::dto::{AncientKind, VersionCategory, VersionKind, VersionSummary};

/// 分类筛选（页面上的四个 filter chip）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Category {
    /// 不过滤：画快照里有什么就是什么（首帧与 `RefreshAll` 的落点）
    #[default]
    All,
    /// 正式版
    Release,
    /// 预览版
    Snapshot,
    /// 远古版（快照里按 `AncientKind` 分四类，页面先只认一个 chip）
    Ancient,
}

impl Category {
    /// 四个 chip 的声明顺序（= 画出来的顺序）。
    pub const ALL: [Category; 4] = [
        Category::All,
        Category::Release,
        Category::Snapshot,
        Category::Ancient,
    ];

    /// chip 文案的 key。
    pub const fn label_key(self) -> Key {
        match self {
            Category::All => Key::DownloadCategoryAll,
            Category::Release => Key::DownloadCategoryRelease,
            Category::Snapshot => Key::DownloadCategorySnapshot,
            Category::Ancient => Key::DownloadCategoryAncient,
        }
    }

    /// 点这个 chip 要派的指令（`None` = 不派：`All` 就是「把当前快照全画出来」）。
    ///
    /// 远古版在 `VersionCategory` 里是**带子类的**（`Ancient(AncientKind)`），一个 chip 拉不全四类；
    /// 这里先拉 `Classic`，四子类同屏要靠 presenter 侧支持（已登记 finding）。
    pub const fn command(self) -> Option<VersionCategory> {
        match self {
            Category::All => None,
            Category::Release => Some(VersionCategory::Release),
            Category::Snapshot => Some(VersionCategory::Snapshot),
            Category::Ancient => Some(VersionCategory::Ancient(AncientKind::Classic)),
        }
    }

    /// 这个版本属于这个分类吗。
    pub fn matches(self, kind: VersionKind) -> bool {
        match self {
            Category::All => true,
            Category::Release => kind == VersionKind::Release,
            Category::Snapshot => kind == VersionKind::Snapshot,
            Category::Ancient => kind.is_ancient(),
        }
    }

    /// 从快照里**反推**当前分类：整份列表只有一类 → 那一类；混合或为空 → [`Category::All`]。
    ///
    /// 快照没有「我上次按哪个分类拉的」这种字段，所以分类靠列表内容反推
    /// （`ListVersions` 的结果是单一分类，`RefreshAll` 的结果是混合）。
    pub fn of(versions: &[VersionSummary]) -> Self {
        let mut kinds = versions.iter().map(|version| version.kind);
        let Some(first) = kinds.next() else {
            return Category::All;
        };
        if kinds.all(|kind| kind == first) {
            match first {
                VersionKind::Release => Category::Release,
                VersionKind::Snapshot => Category::Snapshot,
                VersionKind::Ancient(_) => Category::Ancient,
            }
        } else {
            Category::All
        }
    }
}
