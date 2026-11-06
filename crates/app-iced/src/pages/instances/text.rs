//! 实例页的**文案入口**：页面上一处可见英文字面量都不留，全部从目录取。
//!
//! ## 为什么要有这一层（而不是在控件树里直接 `catalog.text(..)`）
//!
//! 页面要的文案有 20 多条，直接写的话每个调用点都是「key 名 + 这一处画什么」的重复劳动，
//! 而且**守门测试没法取样**：iced 0.14 的 `Element` 没有内省接口（`instances/tests.rs` 里
//! 「读源码」那条注释解释过同一件事），所以「上屏文本」只能从**页面取词的那一个入口**拿。
//! 于是所有取词收进 [`Texts`]，页面用 `texts.xxx()` 画，测试用同一个 `Texts` 断言——
//! 一处定义、两处消费，不存在「测试抄了一份平行实现」。
//!
//! ## 它抓得住什么 / 抓不住什么
//!
//! - **抓得住**：目录里某条中文忘了翻（「无 ASCII 英文单词」）、某条中英写成一样（「切换语言文本真的变」）、
//!   `Running · pid {pid}` 的数字与分隔符被翻译走样、页面里写死**英文**字面量
//!   （`every_english_literal_in_the_page_sources_is_whitelisted`，白名单判定）；
//! - **抓不住**：页面里写死的**中文**字面量（没有 ASCII 词，扫不到）、`PAGE_SOURCES` 五个文件
//!   之外的东西（如 `data.rs` 夹具数据）、译文翻得顺不顺——这些只能靠出图人工看
//!   （`DevDocs/artifacts/m120/`）。
//!
//! ## 口径
//!
//! 品牌名不翻译（`Java` / `Fabric` / `Forge` 原样）；`pid` 是进程号的技术标识，两套语言都写
//! `pid`（改成「进程」会丢掉「拿它去任务管理器里搜」这条可操作性）。
//!
//! **启动阶段（下载中 / 校验中 / …）不在这张表里**：它在 `app/view/instances.rs` 拼进
//! `Row.status`（「阶段 · 百分比」是一整句），与元信息卡同一层——页面只画模型给的字符串。

use unlucky_i18n::{Catalog, Key};

/// 实例页 + 详情列的文案（一个 `Catalog` 的薄包装，`Copy`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Texts(Catalog);

impl Texts {
    /// 从目录取一套（调用方给 `model.lang.catalog()`）。
    pub(super) fn of(catalog: Catalog) -> Self {
        Self(catalog)
    }

    // ------------------------------------------------------------- 页头
    pub(super) fn title(self) -> &'static str {
        self.text(Key::InstancesTitle)
    }
    pub(super) fn subtitle(self) -> &'static str {
        self.text(Key::InstancesSubtitle)
    }
    pub(super) fn new_instance(self) -> &'static str {
        self.text(Key::InstancesNew)
    }
    pub(super) fn import(self) -> &'static str {
        self.text(Key::InstancesImport)
    }

    // --------------------------------------------------------------- 列表列
    pub(super) fn all_title(self) -> &'static str {
        self.text(Key::InstancesAllTitle)
    }

    // --------------------------------------------------------------- 详情列
    pub(super) fn back(self) -> &'static str {
        self.text(Key::InstancesBack)
    }
    pub(super) fn play(self) -> &'static str {
        self.text(Key::InstancesPlay)
    }
    pub(super) fn stop(self) -> &'static str {
        self.text(Key::InstancesStop)
    }
    pub(super) fn duplicate(self) -> &'static str {
        self.text(Key::InstancesDuplicate)
    }
    pub(super) fn open_folder(self) -> &'static str {
        self.text(Key::InstancesOpenFolder)
    }
    pub(super) fn delete(self) -> &'static str {
        self.text(Key::InstancesDelete)
    }
    pub(super) fn empty_title(self) -> &'static str {
        self.text(Key::InstancesEmptyTitle)
    }
    pub(super) fn empty_body(self) -> &'static str {
        self.text(Key::InstancesEmptyBody)
    }
    pub(super) fn narrow_note(self) -> &'static str {
        self.text(Key::InstancesNarrowNote)
    }

    fn text(self, key: Key) -> &'static str {
        self.0.text(key)
    }

    /// 这一页的全部 key（守门测试的取样口：**从 `Key::ALL` 过滤出来**，不是手抄一份清单）。
    ///
    /// 域判据是前缀 `Instances`——本族的 key 名是契约（`catalog/instances.rs` 里一条一条写着），
    /// 新加的 key 只要还在这个族里就自动进守门范围，不需要有人记得来登记。
    #[cfg(test)]
    pub(super) fn keys() -> impl Iterator<Item = Key> {
        Key::ALL
            .iter()
            .copied()
            .filter(|key| key.name().starts_with("Instances"))
    }
}
