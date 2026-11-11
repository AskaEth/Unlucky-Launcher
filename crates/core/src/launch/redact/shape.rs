//! 两条形状判据（`dotted` / `opaque`）的判定与阈值；[`super`] 的扫描器只负责搬运字符。
//!
//! 判据集中在这里一份，是因为它们是**同一件事的两面**：`dotted` 先跑、`opaque` 后跑，
//! 两者的字符集相互补充（`.` 只属于前者，`/` 属于两者），任何一边改了字符集，另一边的
//! 覆盖面就跟着变。
//!
//! # 四条"别抹"的判据（M239 的 D8 误杀）
//!
//! 修法是给每条规则加**必要条件**，不是抬阈值（40 / 3×8 / 64 一个字没动）：
//!
//! | 误杀形状 | 出口判据 | 为什么这一串不可能是真凭据 |
//! |---|---|---|
//! | `-D<key>=<品牌>`（`minecraft.launcher.brand=UnluckyLauncher`） | `=` 不属于 dotted 字符集（赋值边界） | JWT 的 `.` 分段里 `=` 只可能是末尾填充，拆出去不损伤覆盖 |
//! | CDN/镜像主机名（`resources.download.minecraft.net/ab/cdef`） | 首段是 DNS 主机名 → 只判路径那半 | JWT 是 base64url，**不含 `/`**，所以"主机名 + 路径"形状不可能是 JWT |
//! | 目录段/类名（`/srv/…/mods/x.jar`、`org/spongepowered/…`） | 段名是"词形"（[`is_path_run`]） | 随机 base64 的一段里不可能每个连续字母块都恰好是全小写/全大写/CamelCase |
//! | 全小写点分串（`sophisticatedbackpacks.upgrades.reference`） | ①主机名形状的走首段 DNS 豁免 ②**不是**主机名的靠"必须含大写字母"（[`jwt_shaped`]） | JWT 头是 JSON 对象，其 base64url 前三个字符恒为 `ey` + `I/J/K/L`，所以真 JWT 必含大写 |
//!
//! 四条里前两条是**位置/字符集**判据，后两条是**形状**判据；都写成必要条件：判据不成立时
//! 仍然照抹，只有"这一串根本不可能是凭据"时才放行。
//!
//! 第五处边界（M243 BEARERDOT，同样不是抬阈值）：**末尾句读不参与判定**
//! （[`split_trailing_punct`]）。判定前把 `.` `,` `;` `)` 切出去，搬运方原位写回——
//! 行尾句点原先会多切出一个空段，把真 token 整串放行。只有 `.` 会进入 dotted 字符集，
//! 其余三个本来就在外面；列全是为了「句读不参与判定」只有一处出处。
//!
//! 关于"必须含大写"的实际覆盖面（复审 P2-2 的怀疑成立一半）：**DNS 形状**的全小写点分串
//! 早在首段 DNS 豁免那一条就被救下了，跟大写判据无关；大写判据唯一扛住的是**不是 DNS 形状**
//! 的全小写点分串（标签带 `_`、标签超 63 字符、`/` 前只有一段主机名）。它保留下来，因为那类
//! 形状同样不可能是 JWT，而且删掉它就会把误杀请回来——`the_uppercase_requirement_is_load_bearing`
//! 就是钉这一点的用例（删掉判据 → 该测试红）。
//!
//! # 旧行为抹、新行为留的形状（三类，全部有测试钉住）
//!
//! 口径说明：**不是"抹除能力整体没有下降"**——收窄的就是下面三类，前两条是本次要修的误杀，
//! 第三条是代价。都没有放宽长度/段数阈值，也没有动"登记过的明文值"那条替换路径。
//!
//! 1. **`=` 边界**：`-Dminecraft.launcher.brand=UnluckyLauncher` —— 旧行为把 `键=值` 粘成
//!    一段，键里的点号凑够段数/总长；现在两半各自不达标。（这就是要修的品牌串误杀。）
//!    同一处边界的**代价**：`-DaccessToken=abcdefgh.ijklmnop.qrstuvwx`（整段 40 字符、
//!    三段各 ≥8，旧规则抹）现在 `=` 切段后值只剩 26 < 40 → 原样留下。也就是说
//!    **紧跟 `=`、自己又不够 40 的带点值**不再被形状规则兜住，只剩"登记明文值"那条路径，
//!    这是换品牌串保留付出的代价。
//! 2. **填充不算段长**：`eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.c2lnbm==` ——
//!    旧行为把 `==` 算进末段（8 ≥ 8），现在 `=` 在段外、末段实际 6 < 8，整串留下。
//!    真 JWT 的签名段是 43+ 字符（HS256）/ 342（RS256），6–7 字符的末段只可能是被裁剪或
//!    自造的票据；末段 ≥8 时（`…c2lnbmFy==`）仍照抹，缝合线由测试钉住。
//! 3. **全小写点分串**：`sophisticatedbackpacks.upgrades.reference`（41 字符 / 三段各 ≥8）
//!    —— 旧行为抹，现在放行（DNS 形状的走主机名豁免，非 DNS 形状的走"必须含大写"）。
//!    这是**刻意的**：真 JWT 必含大写（见上表末行），而全小写点分串正是点分类路径的常见形状。

/// 判定“像 token”的最短连续长度。
pub(super) const MAX_OPAQUE_RUN: usize = 64;
/// 「带点 token」（JWT 三段式）判定的最短总长。
pub(super) const DOTTED_TOKEN_MIN_TOTAL: usize = 40;
/// 「带点 token」每一段的最短长度（挡住 `Service.Unavailable.today` 这类正常文案）。
pub(super) const DOTTED_TOKEN_MIN_SEGMENT: usize = 8;

/// 不透明串的字符集（base64 / hex / 路径分隔符）。`.` 不在内，`: \ @` 也不在内。
pub(super) fn is_opaque_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '+' | '/' | '=' | '_' | '-')
}

/// 「带点 token」的字符集：不透明串的字符加 `.`、减 `=`。
///
/// `=` 是**赋值边界**而不是 token 本体。把它留在字符集里，`-D<key>=<值>` 就会被粘成一个
/// run：键里的点号（`minecraft.launcher.brand`）会把值一起拖进段数统计，M239 实测的品牌串
/// 误杀就是从这儿来的。
///
/// 这条边界的代价**有范围**，别读成"不损伤覆盖面"：`=` 切段之后，**紧跟 `=`、自己又不够
/// 40 字符**的带点值不再被形状规则兜住——`-DaccessToken=abcdefgh.ijklmnop.qrstuvwx`（整段
/// 40 字符、三段各 ≥8，旧规则抹）现在切出 26 < 40 而原样留下。这是换品牌串保留的代价，
/// 记在模块文档「旧行为抹、新行为留」第 1 条的末句；真 token 仍由 [`super::Redactor::with_secret`]
/// 那条明文值替换路径扛住。
pub(super) fn is_dotted_token_char(ch: char) -> bool {
    ch != '=' && (is_opaque_char(ch) || ch == '.')
}

/// 单段连写（无空白）的串是否像 JWT。
pub(super) fn is_dotted_token(run: &str) -> bool {
    jwt_shaped(run) && !is_url_authority(run)
}

/// 把 run 末尾的句读切出去：返回（判据只看的前半, 原样搬回的后半）。
///
/// 句读（`.` `,` `;` `)`）不属于 token 本体。`.` 是**唯一会进入** dotted 字符集的句读
/// （见 [`is_dotted_token_char`]），所以行尾一个句点就会把 run 多切出一个空段：
/// 「每段 ≥8」当场不成立，整串原样落盘——BEARERDOT（M243）。`(` `,` `;` 早被字符集
/// 切在 run 外，这里一并列出，是把「句读不参与判定」写成一条规则：字符集将来若放宽，
/// 判据不用再改一遍。
///
/// 只削**末尾**：中间的句点仍是 token 本体（JWT 的三段就是靠它切的）。
/// 削短只会让形状判据更容易成立，方向在安全那一侧；搬运方负责把切出去的后半原位写回。
pub(super) fn split_trailing_punct(run: &str) -> (&str, &str) {
    let body = run.trim_end_matches(['.', ',', ';', ')']);
    run.split_at(body.len())
}

/// 「带点 token」的形状：段数 ≥3、每段 ≥8 字节、总长 ≥40（老规则，一个字都没放宽），
/// 外加两条必要条件——首段不是主机名、**含大写字母**。
///
/// "必须含大写"依据的是 JWT 的构造而不是概率：头的明文是 JSON 对象，以 `{"` 开头。
/// 三个字节 `7B 22 xx` 的 base64url 前三个字符 = `e`、`y`、`I`/`J`/`K`/`L`
/// （第三组 6 位 = `b1` 低 4 位 `0010` ++ `b2` 高 2 位 → 索引 8..11，全是大写）。
/// 也就是说**第三个字符**必为大写，任何真 JWT 都躲不过这条。
///
/// 代价写在这里而不是藏起来：全小写点分串（`sophisticatedbackpacks.upgrades.reference`，
/// 以及非 DNS 形状的 `sophisticated_backpacks.upgrades.reference`，旧行为都会抹）因此被放行，
/// 见模块文档「旧行为抹、新行为留」第 3 条。
fn jwt_shaped(run: &str) -> bool {
    run.len() >= DOTTED_TOKEN_MIN_TOTAL
        && run.split('.').count() >= 3
        && run
            .split('.')
            .all(|segment| segment.len() >= DOTTED_TOKEN_MIN_SEGMENT)
        && run.chars().any(|ch| ch.is_ascii_uppercase())
}

/// run 的第一段（到第一个 `/` 为止）是不是主机名：是的话这就是一条 URL，不是 token。
///
/// 扫描器（`super::push_dotted`）先按这条把主机名切走、只判路径那半；这里的判据留一份，
/// 是为了让「一段串是不是 token」单独成立（`has_dotted_token` 与单测直接用它）。
pub(super) fn is_url_authority(run: &str) -> bool {
    is_dns_host(run.split('/').next().unwrap_or(run))
}

/// 整串是不是一个 DNS 主机名（至少两段标签，标签只含小写字母/数字/连字符）。
///
/// JWT 是 base64url，**不含 `/`**；主机名的标签也不会大写（URL 里从没这么写过）。
pub(super) fn is_dns_host(name: &str) -> bool {
    let labels: Vec<&str> = name.split('.').collect();
    labels.len() >= 2 && labels.iter().all(|label| is_dns_label(label))
}

/// 单个 DNS 标签：小写字母/数字/连字符，首尾不是连字符（日志里的主机名不会写大写）。
fn is_dns_label(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= 63
        && label
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
        && !label.starts_with('-')
        && !label.ends_with('-')
}

/// 长不透明串是不是一段路径/命名空间（目录、文件名、Java 包名）。
///
/// 两条：① 至少被 `/` 切开一次；② 每一段都是"名字"（[`is_name_chunk`]）。
/// 判据里没有任何熵估计，"词形 vs 随机"就靠第 ② 条：随机 base64 的一段里每个连续字母块
/// 都恰好是全小写/全大写/CamelCase 的概率可以忽略——所以一条 base64 密钥不会被当成路径。
/// 方向也是安全的：判定失败的串照旧被抹，只是路径不再整条消失。
pub(super) fn is_path_run(run: &str) -> bool {
    run.split('/').count() >= 2 && run.split('/').all(is_name_chunk)
}

/// 路径/包名的一段：只许字母数字与 `- _ .`，且每个连续字母块都是"词"（[`is_word`]）。
fn is_name_chunk(chunk: &str) -> bool {
    chunk
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
        && chunk
            .split(|ch: char| !ch.is_ascii_alphabetic())
            .all(is_word)
}

/// 连续字母块是否像词：全小写 / 全大写（缩写）/ CamelCase（`OpenGL`、`AllTheMods`、`FabricMixin…`）。
fn is_word(word: &str) -> bool {
    let mut has_lower = false;
    let mut has_upper = false;
    for ch in word.chars() {
        if ch.is_ascii_lowercase() {
            has_lower = true;
        } else if ch.is_ascii_uppercase() {
            has_upper = true;
        } else {
            return false;
        }
    }
    if !has_lower || !has_upper {
        return true;
    }
    let chars: Vec<char> = word.chars().collect();
    let mut idx = 0usize;
    while idx < chars.len() {
        if !chars[idx].is_ascii_uppercase() {
            return false;
        }
        let mut end = idx + 1;
        while end < chars.len() && chars[end].is_ascii_uppercase() {
            end += 1;
        }
        let lowered = end < chars.len() && chars[end].is_ascii_lowercase();
        if lowered && end - idx != 1 {
            return false; // `ABCDef`：缩写后面接小写，不是词
        }
        idx = end;
        while idx < chars.len() && chars[idx].is_ascii_lowercase() {
            idx += 1;
        }
    }
    true
}
