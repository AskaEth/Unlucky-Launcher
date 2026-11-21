//! 启动结局文案：一句话 = 一个 key（原来 UI 把 `"Launch failed"` 与 core 的「静默失败」拼成一句话）。
//!
//! 域文件的形状见 [`crate::catalog`] 的「域装配」一节。

macro_rules! launch_entries {
    ($next:ident, $($rest:ident,)* { $($acc:tt)* }) => {
        $next! { $($rest,)* {
            $($acc)*
            // ── 启动结局（原来 UI 把 "Launch failed" 与 core 的「静默失败」拼成一句话，现在一句话 = 一个 key）
            LaunchOutcomeNormal => ("Finished normally", "正常结束"),
            LaunchOutcomeCrash => ("The game crashed", "游戏崩溃"),
            LaunchOutcomeSilent => (
                "Suspected silent failure: exit code 0, but the game never really started",
                "疑似静默失败：退出码 0，但游戏没真正跑起来"
            ),
            LaunchFailedWithCode => ("Launch failed (exit {code}): {reason}", "启动失败（退出码 {code}）：{reason}"),
            // M161：启动失败的**阶段**词表。presenter 只给稳定 slug
            // （`LaunchFailureStage`），界面在这里选词——此前这些前缀是 presenter 里拼的中文
            // （「读版本 JSON 失败」「隔离目录派生失败」…），英文界面上原样露出来。
            LaunchStageVersionManifest => ("Reading the version file", "读版本文件"),
            LaunchStageInheritance => ("Resolving the inheritance chain", "解析继承链"),
            LaunchStageInstanceConfig => ("Reading the instance settings", "读实例设置"),
            LaunchStageIsolation => ("Preparing the isolated game folder", "准备隔离游戏目录"),
            LaunchStageIdentity => ("Preparing the account", "准备账户"),
            LaunchStageSpawn => ("Starting the game process", "启动游戏进程"),
            LaunchStageFailed => ("Launch failed · {stage}: {detail}", "启动失败 · {stage}：{detail}"),
            LaunchToastEnded => ("{outcome} · {seconds}s", "{outcome} · {seconds} 秒"),
            // M117：微软轨的 client id 没配是**配置缺失**，不是「本期未支持」。这句话是那条
            // 启动失败文案的**双语口径**（真源在 `core::auth::microsoft` 的
            // `MISSING_CLIENT_ID_MESSAGE`，中文逐字一致；英文这条给 En 界面用）。
            // 判据：中英都点名 `ULL_MSA_CLIENT_ID` 并给出构建期注入这条路，且**不许**出现
            // 「unsupported / 未支持」——用户实跑就是被那句误导读成「功能没做」。
            LaunchMsaNotConfigured => (
                "Microsoft sign-in is not configured: it needs an Azure public client id (the login flow itself is implemented). Build with ULL_MSA_CLIENT_ID=<id>, or set that environment variable before launching.",
                "微软登录未配置：缺少 Azure 公共客户端 id（登录功能已实现，只差这一项配置）。构建时注入：ULL_MSA_CLIENT_ID=<id> cargo build；或启动前设置同名环境变量 ULL_MSA_CLIENT_ID=<id>。"
            ),
        } }
    };
}

pub(crate) use launch_entries;

#[cfg(test)]
mod tests {
    use crate::{Catalog, Key, Lang};

    #[test]
    fn msa_not_configured_copy_is_accurate_in_both_languages() {
        // 抓得住：文案退回「本期未支持」这类说法（M117 用户实跑踩到的原 bug），
        //         以及两条译文只剩一句「需要 id」却不给怎么配。
        // 抓不住：读起来顺不顺——那要靠 review 与截图。
        let en = Catalog::of(Lang::En).text(Key::LaunchMsaNotConfigured);
        let zh = Catalog::of(Lang::ZhCn).text(Key::LaunchMsaNotConfigured);

        for (tag, text) in [("en", en), ("zh-CN", zh)] {
            assert!(text.contains("ULL_MSA_CLIENT_ID"), "{tag}: {text}");
        }
        assert!(en.contains("client id"), "{en}");
        assert!(en.contains("not configured"), "{en}");
        assert!(!en.to_lowercase().contains("unsupported"), "{en}");
        assert!(!en.to_lowercase().contains("not implemented"), "{en}");

        assert!(zh.contains("客户端 id"), "{zh}");
        assert!(zh.contains("未配置"), "{zh}");
        assert!(!zh.contains("未支持"), "{zh}");
        assert!(!zh.contains("未实现"), "{zh}");
        // 硬约束：词条里只许出现 `<id>` 占位，不出现真 id（repo 任何文件都不许带）。
        assert!(en.contains("<id>") && zh.contains("<id>"));
    }
}
