//! 内容库的状态机（M109）：结果落地与条件提交两件事。
//!
//! 从 `state.rs` 拆出（那个文件重新合流 M105 之后贴着 400 行上限）。放在 `impl AppState` 的
//! **另一个 impl 块**里，路径与可见性都不变：`apply_core` 直接调 `self.apply_content(..)`。

use crate::dto::{ContentQuery, ContentResults};

use super::AppState;

impl AppState {
    /// 内容库结果落地（M109）：同条件换页 ⇒ **追加**；换条件 ⇒ **整批替换**；迟到 ⇒ 丢掉。
    ///
    /// 这三条就是「列表画的永远是当前筛选条件」的全部保证——页面自己没有过滤逻辑，也没有第二份真相。
    pub(super) fn apply_content(&mut self, results: ContentResults) {
        self.content.pending = false;
        self.content.failed = None;
        if !self.content.accepts(&results) {
            // 迟到的一批：用户已经改了条件，覆盖上去就会出现「筛选条说 A、列表是 B」。
            return;
        }
        // **先**按「这一页装没装满」记一位，再把它并进累计列表（复审 P1-2：判据不能读累计值）。
        self.content.last_page_filled = results.has_more();
        match &mut self.content.results {
            Some(current) if results.query.offset > 0 => {
                current.hits.extend(results.hits);
                current.total = results.total;
                current.curseforge = results.curseforge;
                current.query = results.query;
            }
            _ => self.content.results = Some(results),
        }
    }

    /// 记下新的检索条件并进入「请求在飞」态（指令由调用方随后 dispatch）。
    pub fn set_content_query(&mut self, query: ContentQuery) {
        self.content.pending = true;
        self.content.failed = None;
        if query.offset == 0 {
            self.content.last_page_filled = false;
            // 换条件（不是换页）：先把旧结果清掉，否则加载态会画出上一批命中项。
            self.content.results = None;
        }
        self.content.query = query;
    }
}
