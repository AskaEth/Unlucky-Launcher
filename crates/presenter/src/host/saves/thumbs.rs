//! 截图的**后台解码 + 进程级 memo**（M251 · SAVESHOTS，落地 D54 第 3/4/5/6 条）。
//!
//! ## 为什么解码在后台线程
//!
//! 一屏 60 张 PNG 解码是几百毫秒量级的工作。D54 明写「**不许**在渲染回调里解码」：把这段放进
//! `iced` 的 `view()` 会直接把帧率打穿。所以解码发生在 [`super::spawn_list`] 那条后台线程里
//! （`job::spawn`，与 `host/saves.rs` 同一模式），UI 那一侧只收一份**带像素的快照**。
//!
//! ## 缓存键为什么必须是三段
//!
//! `(路径, mtime, 大小)`——**只按路径**是错的，而且是那种「测试不会红」的错：
//!
//! | 场景 | 只按路径的后果 |
//! |---|---|
//! | 游戏又存了一张同名截图（覆盖） | 永远显示上一张的缩略图 |
//! | 用户拿同名文件替换（修图后放回） | 同上，且没有任何日志 |
//!
//! 带上 mtime 与大小之后，覆盖 ⇒ 键变了 ⇒ 重解。这条由
//! `tests::a_changed_file_is_decoded_again` 钉住（把 mtime 从键里去掉，它必红）。
//!
//! ## 为什么是**进程级** memo 而不是状态字段
//!
//! 解码发生在 `spawn_list` 起的线程里，而那条线程拿到的东西只有 `HostEnv` + `tx`（见
//! `host/saves.rs`）。把缓存挂到 `CoreHost` 上要改 `host/mod.rs`（贴着 400 行）与
//! `host/dispatch.rs`（M251 的 scope 外），换来的语义与这里**完全一样**：键是全路径，多实例
//! 共享一份不会串味；它也不是业务状态，只是一份「同样的键不再重解」的记忆。
//!
//! ## 两条硬上限（D54 第 5 条）
//!
//! | 上限 | 值 | 为什么 |
//! |---|---|---|
//! | 单张字节数 | [`MAX_SOURCE_BYTES`] = 8 MiB | 一张 200MB 的 PNG 解码要几 GB 内存；**解码前**查大小，超了就不进解码器，并留一条可见原因 |
//! | 每次刷新张数 | [`MAX_PER_REFRESH`] = 60 | 按 mtime 倒序只解前 60 张（最近玩的那几屏最有用），其余给「未生成预览」占位 |
//!
//! 上限是**逐条可见**的：跳过的那张在快照里带 [`ScreenshotPreview::Failed`] 或
//! [`ScreenshotPreview::NotGenerated`]，不是静默少一张（D54 第 6 条）。
//!
//! ## 缓存不会无限长
//!
//! 键里有 mtime，覆盖一次就多一个键。所以超过 [`MAX_CACHE_ENTRIES`] 时只保留**这一次刷新碰到
//! 的键**（换目录、删截图之后旧键一起掉）。代价是极端情况下重解一次，收益是常驻内存有界。

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use unlucky_palette_extract::{thumbnail, ExtractError, THUMBNAIL_MAX_EDGE};

use crate::dto::{
    ScreenshotPreview, ScreenshotThumbFailure, ScreenshotThumbKind, ScreenshotView, ThumbnailView,
};

/// 单张截图的解码上限（字节）。超过就**不解码**（D54 第 5 条）。
pub const MAX_SOURCE_BYTES: u64 = 8 * 1024 * 1024;

/// 每次刷新最多解几张（按 mtime 倒序取前这么多）。
pub const MAX_PER_REFRESH: usize = 60;

/// 缓存的条目上限（超过就只留这一拍碰到的键）。
///
/// **算过内存**：一张缩略图最多 `160×160×4 = 102.4 KB`，120 条 ⇒ **上限 ≈ 12 MB**；
/// 一次刷新最多碰 60 张（≈6 MB），所以 120 = 「当前这一屏 + 上一屏」的余量。
/// 定这条上限的背景：缓存是**进程级静态**（见 [`cache`]），不随实例/数据根释放，
/// 所以要有一个能算出来的天花板，而不是「反正删了就少一张」。
pub const MAX_CACHE_ENTRIES: usize = 120;

/// 缓存键：**(完整路径, mtime 毫秒, 字节数)**。
///
/// 三段缺一不可，理由见模块文档的表格。`mtime` 是 `Option`：取不到时间的文件用 `None` 占位
/// （那种文件本来就没有「变了」的可靠判据，大小仍然兜住一部分）。
type Key = (String, Option<u64>, u64);

/// 缓存里存的是**结局**（成功或失败）。
///
/// 失败也缓存：一张坏 PNG 不该每次刷新都重解一遍（那是「滚动一次卡一次」的另一种来源）。
/// 键里带 mtime ⇒ 用户修好文件（mtime 变）之后自然会重解。
type Cache = HashMap<Key, Result<ThumbnailView, ScreenshotThumbFailure>>;

fn cache() -> &'static Mutex<Cache> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 清空缓存（**只给用例**：这是进程级静态，用例之间必须能从已知状态起跑）。
///
/// 生产路径没有任何调用点——键里带完整路径，两个数据根/两个实例共用一个静态表也不会串味
/// （见模块文档「为什么是进程级 memo」）；但**用例**会反复用短路径造键，谁都不该依赖
/// 「上一个用例刚好留了/没留什么」。
#[cfg(test)]
pub(super) fn clear_cache() {
    cache()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clear();
}

/// 给一批截图挂上预览（**在后台线程里调**，`super::view` 的唯一调用点）。
pub(super) fn attach(dir: &Path, views: &mut [ScreenshotView]) {
    attach_with(dir, views, &mut decode);
}

/// [`attach`] 的可测内核：解码器是一个参数，于是用例能数「到底进了几次解码器」。
///
/// 这类断言正是 D54 要求的那条「**缓存命中不重复解码**」：数的是解码器的调用次数，不是
/// 「界面上看起来一样」。
fn attach_with(
    dir: &Path,
    views: &mut [ScreenshotView],
    decode: &mut impl FnMut(&Path, u64) -> Result<ThumbnailView, ScreenshotThumbFailure>,
) {
    if views.is_empty() {
        return;
    }
    // ① 排序：mtime 倒序（取不到的排最后），同刻按文件名倒序兜底（两次刷新之间不抖）。
    let mut order: Vec<usize> = (0..views.len()).collect();
    order.sort_by(|&left, &right| {
        views[right]
            .modified_ms
            .cmp(&views[left].modified_ms)
            .then_with(|| views[right].file.cmp(&views[left].file))
    });

    let mut store = cache().lock().unwrap_or_else(|poison| poison.into_inner());
    let mut touched: Vec<Key> = Vec::with_capacity(order.len().min(MAX_PER_REFRESH));
    for (rank, &index) in order.iter().enumerate() {
        // ② 超出这一拍的解码预算：占位，不碰盘、不进解码器。
        if rank >= MAX_PER_REFRESH {
            views[index].preview = ScreenshotPreview::NotGenerated;
            continue;
        }
        let (file, bytes, modified) = {
            let shot = &views[index];
            (shot.file.clone(), shot.bytes, shot.modified_ms)
        };
        // ③ 单张太大：**解码前**就拒，理由逐条可见。
        if bytes > MAX_SOURCE_BYTES {
            views[index].preview = ScreenshotPreview::Failed(ScreenshotThumbFailure {
                kind: ScreenshotThumbKind::TooLarge,
                detail: format!("{bytes} > {MAX_SOURCE_BYTES}"),
            });
            continue;
        }
        // ④ 缓存 → 解码 → 回填。
        let path = dir.join(&file);
        let key: Key = (path.display().to_string(), modified, bytes);
        let outcome = match store.get(&key) {
            Some(cached) => cached.clone(),
            None => {
                let computed = decode(&path, bytes);
                store.insert(key.clone(), computed.clone());
                computed
            }
        };
        touched.push(key);
        views[index].preview = match outcome {
            Ok(view) => ScreenshotPreview::Ready(view),
            Err(failure) => ScreenshotPreview::Failed(failure),
        };
    }
    // ⑤ 有界：只留这一拍碰到的键（旧的一律掉）。
    prune(&mut store, &touched);
}

/// 淘汰策略：超过 [`MAX_CACHE_ENTRIES`] 时只留这一拍碰到的键。
///
/// 单独成函数是为了**能在本地 `Cache` 上测**：全局缓存被所有用例共享，谁往它里面灌 240+ 条
/// 就会顺手把别的用例刚缓存的键清掉（M251 实测：并行跑时 `the_second_identical_refresh_...`
/// 会假红）。策略与「共享状态」解耦之后，两条判据各自确定。
fn prune(store: &mut Cache, touched: &[Key]) {
    if store.len() > MAX_CACHE_ENTRIES {
        store.retain(|key, _| touched.contains(key));
    }
}

/// 真解码（唯一碰 `image` 的地方；错都在 `palette-extract` 里分好类了）。
fn decode(path: &Path, _bytes: u64) -> Result<ThumbnailView, ScreenshotThumbFailure> {
    match thumbnail(path, THUMBNAIL_MAX_EDGE) {
        Ok(image) => Ok(ThumbnailView {
            width: image.width(),
            height: image.height(),
            pixels: image.pixels().to_vec(),
        }),
        Err(error) => Err(failure(&error)),
    }
}

/// `palette-extract` 的错误 → 预览失败的大类 + 原文（不翻译，翻译在 app 层）。
fn failure(error: &ExtractError) -> ScreenshotThumbFailure {
    let kind = match error {
        ExtractError::Io(_) => ScreenshotThumbKind::Unreadable,
        // 取色失败那条支在本路径上不可能出现（`thumbnail` 不做量化），但穷尽 match 比
        // 通配兜底诚实：真出现了也算「解不出来」，不会被当成读到了。
        ExtractError::Decode(_) | ExtractError::Palette(_) => ScreenshotThumbKind::Decode,
    };
    ScreenshotThumbFailure {
        kind,
        detail: error.to_string(),
    }
}

#[cfg(test)]
mod tests;
