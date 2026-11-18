//! 快照条目的**现在**：这一条还在不在盘上（M304）。
//!
//! ## 为什么需要
//!
//! M304 把「点启动」的实例解析从「同步扫一遍 [`super::super::instances`]」改成「查已发布快照」。
//! 快照是**过去**的一拍：用户可能在别处（文件管理器 / 另一个启动器 / 同一会话的另一条命令）
//! 把这个实例删了、或者把它绑的版本改了。照旧解析就会「按快照把它拉起来」——拉的是**已经不
//! 存在的东西**，最坏的一种是 `game_for` 反查不到那个根、退回配置根，于是**启动的是另一个根里
//! 同名版本**（用户看着 A，起来的却是 B）。
//!
//! ## 判据（单实例，不扫 `versions/`）
//!
//! 一次只判**这一条**，所以绝不能再用 `instances()`（那是全量扫盘：官方根 200 个版本 236 ms，
//! 叠加外部档案 1.35 s）——那等于把「点了就受理」换成「点了等一扫」。两条判据各用最便宜的
//! 那个事实：
//!
//! | 实例 | 判据 | 成本 |
//! |---|---|---|
//! | 档案实例（id 不是 `ext-`） | `profiles.json` 里还有这条 id，且 `version_id` 没被改过 | 一次小文件读 |
//! | 自动发现的（`ext-`） | 逐个根重算 id（哈希）定位到 (根, 版本)，再用 core 的 `adopt::inspect` 判这一个版本 | #根 次哈希 + 一次单版本检查 |
//!
//! ## 反查落空的两种情形，必须分开（r1 复审 P2-1）
//!
//! `roots::all` 对非配置根是 `if !extra.is_dir() { continue; }`（只有配置根无条件入列）。于是
//! 「官方根被删 / 改名 / 盘没挂上 / `canonicalize` 变了」时，那个 `ext-` id 一个根都对不上——
//! 若照旧放行，`build_request` → `game_for`（`discovered::game_for` 的回落）会退回**配置根**，
//! 配置根里恰好有同名 `version_id` 就把**另一份**拉起来（gameDir / 存档都在另一边）。这是相对
//! 改前（同步扫盘 ⇒ 扫不出来 ⇒ 拒绝「找不到实例」）的回归，所以必须分开判：
//!
//! - **M273 老档案**（id 只哈希版本目录名 ⇒ `legacy_external_instance_id` 命中）：**放行**——
//!   那种 id 的语义本来就是「按配置根解析」，真没版本时 `build_request` 给具名阶段失败；
//! - **所属根不见了**（删 / 改名 / 盘没挂上 / `canonicalize` 变了）：**拒绝** + 具名原因，并由
//!   调用方立刻重扫（`Rejection::stale`）。
//!
//! 放行老档案不是偷懒：改前那条同步扫盘也不会拦它——`version_present: false` 的磁贴在旧实现里
//! 照样能点、照样走到同一条具名失败（`VersionManifest` + 路径），进程一个都没起来。
//!
//! ## 为什么挂在 [`super::Scan`] 上
//!
//! [`super::super::discovered`] / [`super::super::roots`] 是 `game` 的私有子模块，`host` 其它地方
//! 只够得着 `game` 转出的那几个名字（`Scan` / `instances` / `game_for` …）。`impl` 块可以写在任何
//! 模块里，于是这一层用关联函数挂在 [`super::Scan`] 上——语义也对：被判的正是「这份扫描账里的
//! 那一条现在还成不成立」。

use unlucky_core::game::adopt;
use unlucky_core::game::install::instance::read_profiles;

use crate::dto::InstanceSummary;

use super::super::super::HostEnv;
use super::super::discovered::{external_instance_id, legacy_external_instance_id};
use super::super::roots;

impl super::Scan {
    /// 这条快照条目**现在**还在不在盘上；不在就给一句给用户看的具名原因。
    ///
    /// 只判这一条（成本见模块文档的表格），**不重扫 `versions/`**：调用方是「点启动」那条同步
    /// 路径，它必须便宜到能在同一拍里给出 Busy。
    ///
    /// # Errors
    ///
    /// 返回的字符串是**给人读的原因**（presenter 没有 i18n 依赖，文案在这里拼，与 `toast` 同一种
    /// 形状）：实例清单里没了 / 版本被改过 / 版本目录已不可用。调用方负责拒绝受理并重扫一遍。
    pub(in crate::host) fn revalidate(
        env: &HostEnv,
        instance: &InstanceSummary,
    ) -> Result<(), String> {
        if crate::dto::is_discovered_instance(&instance.id) {
            return discovered(env, instance);
        }
        profile(env, instance)
    }
}

/// 档案实例：`profiles.json` 里还有这条 id，且绑的版本没被改过。
///
/// 「版本被改过」也算过期：用户改的是**同一个档案**，但屏幕上那条磁贴写着旧版本、启动链也会按
/// 旧版本装配——那不是他刚才看中的那一条。
fn profile(env: &HostEnv, instance: &InstanceSummary) -> Result<(), String> {
    let profiles =
        read_profiles(&env.layout).map_err(|error| format!("读实例清单失败：{error}"))?;
    let Some(profile) = profiles
        .profiles
        .iter()
        .find(|profile| profile.id.as_str() == instance.id)
    else {
        return Err(format!(
            "实例 `{}` 已不在实例清单里（可能刚在别处被删掉），列表已过期",
            instance.name
        ));
    };
    if profile.version_id != instance.version_id {
        return Err(format!(
            "实例 `{}` 绑的版本已被改成 `{}`（列表里还是 `{}`），列表已过期",
            instance.name, profile.version_id, instance.version_id
        ));
    }
    Ok(())
}

/// 自动发现的实例：id 是 (根, 版本) 的哈希，逐个根重算一遍定位，再判这一个版本。
///
/// 反查不到根 ⇒ 放行（M273 老档案，理由见模块文档）：不猜、也不在这里改成「拒绝」，
/// 否则所有老用户的 `ext-` 磁贴会集体点不动。
///
/// **别把末尾那三行当死代码删掉**（M306 两次复审的账，别只记一半）：
///
/// - 删掉它，`cargo test` **仍然全绿**（M304 r2 实测：整棵 presenter 426 passed / 0 failed）；
/// - 但**不是无声的**：`unused_imports` 会点名 `legacy_external_instance_id`，
///   `clippy -- -D warnings` 当场红（M306 r1 复审实测 EXIT=101）。`#![allow(dead_code)]` 只盖
///   「函数变死码」，盖不住「use 行变空」——那是 `unused` 组里**另一条** lint；
/// - **这道保险可以被绕过**：把 import 一起删掉、或在那行加 `#[allow(unused_imports)]`，编译器
///   就真的不吭声了。所以还有两道网，删之前先跑：
///   `host::game::tests::a_m273_legacy_ext_id_is_allowed`（判**行为**）与
///   `the_legacy_pass_through_is_still_in_the_source`（直接扫本文件的**源码**）。
///
/// 拒绝那一侧由 `an_ext_id_from_no_visible_root_is_refused` 守着。
fn discovered(env: &HostEnv, instance: &InstanceSummary) -> Result<(), String> {
    for root in roots::all(env) {
        if external_instance_id(root.key(), &instance.version_id) != instance.id {
            continue;
        }
        return adopt::inspect(root.game(), &env.layout, &instance.version_id)
            .map(|_| ())
            .map_err(|error| {
                format!(
                    "实例 `{}` 的版本 `{}` 已不可用（{error}），列表已过期",
                    instance.name, instance.version_id
                )
            });
    }
    // 反查落空：老档案放行，**根不见了就拒绝**（理由见模块文档——放行会让 `game_for` 退回配置根，
    // 把另一个根里同名的那个版本拉起来）。
    if legacy_external_instance_id(&instance.version_id) == instance.id {
        return Ok(());
    }
    Err(format!(
        "实例 `{}` 所属的游戏目录已不在（被移走 / 改名 / 盘没挂上），列表已过期",
        instance.name
    ))
}
