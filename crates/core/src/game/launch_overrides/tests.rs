use super::*;

fn instance_with(field: &str) -> LaunchOverrides {
    match field {
        "jvm" => LaunchOverrides {
            jvm_args: Some("-XX:+UseG1GC".to_string()),
            ..LaunchOverrides::default()
        },
        "game" => LaunchOverrides {
            game_args: Some("--fullscreen".to_string()),
            ..LaunchOverrides::default()
        },
        "memory" => LaunchOverrides {
            memory_mb: Some(6144),
            ..LaunchOverrides::default()
        },
        "java" => LaunchOverrides {
            java_path: Some(PathBuf::from("D:/jdk21/bin/java.exe")),
            ..LaunchOverrides::default()
        },
        "backend" => LaunchOverrides {
            graphics_backend: Some(GraphicsBackend::Vulkan),
            ..LaunchOverrides::default()
        },
        other => panic!("未知字段 {other}"),
    }
}

fn global_with(field: &str) -> GlobalLaunchDefaults {
    match field {
        "jvm" => GlobalLaunchDefaults {
            jvm_args: Some("-Xss4m".to_string()),
            ..GlobalLaunchDefaults::default()
        },
        "game" => GlobalLaunchDefaults {
            game_args: Some("--foo".to_string()),
            ..GlobalLaunchDefaults::default()
        },
        "memory" => GlobalLaunchDefaults {
            memory_mb: Some(3072),
            ..GlobalLaunchDefaults::default()
        },
        "java" => GlobalLaunchDefaults {
            java_path: Some(PathBuf::from("C:/jdk17/bin/java.exe")),
            ..GlobalLaunchDefaults::default()
        },
        "backend" => GlobalLaunchDefaults {
            graphics_backend: Some(GraphicsBackend::Opengl),
            ..GlobalLaunchDefaults::default()
        },
        other => panic!("未知字段 {other}"),
    }
}

/// **三级回落逐项**：五项各自走「实例 → 全局 → 默认」，一项不得连坐另一项。
#[test]
fn every_field_falls_back_on_its_own() {
    for field in ["jvm", "game", "memory", "java", "backend"] {
        // ① 实例有值 → 实例赢（jvm 例外：两层叠加，见下一条测试）。
        let from_instance = resolve(
            Some(&instance_with(field)),
            &GlobalLaunchDefaults::default(),
        );
        let from_global = resolve(None, &global_with(field));
        let neither = resolve(None, &GlobalLaunchDefaults::default());

        match field {
            "jvm" => {
                assert_eq!(
                    from_instance.instance_jvm_args.as_deref(),
                    Some("-XX:+UseG1GC")
                );
                assert_eq!(from_global.global_jvm_args.as_deref(), Some("-Xss4m"));
                assert!(neither.global_jvm_args.is_none());
                assert!(neither.instance_jvm_args.is_none());
            }
            "game" => {
                assert_eq!(from_instance.game_args.as_deref(), Some("--fullscreen"));
                assert_eq!(from_global.game_args.as_deref(), Some("--foo"));
                assert!(neither.game_args.is_none());
            }
            "memory" => {
                assert_eq!(from_instance.memory_mb, 6144);
                assert_eq!(from_global.memory_mb, 3072);
                assert_eq!(neither.memory_mb, DEFAULT_MAX_MEMORY_MB);
            }
            "java" => {
                assert_eq!(
                    from_instance.java_path.as_deref(),
                    Some(std::path::Path::new("D:/jdk21/bin/java.exe"))
                );
                assert_eq!(
                    from_global.java_path.as_deref(),
                    Some(std::path::Path::new("C:/jdk17/bin/java.exe"))
                );
                assert!(neither.java_path.is_none());
            }
            "backend" => {
                assert_eq!(from_instance.graphics_backend, GraphicsBackend::Vulkan);
                assert_eq!(from_global.graphics_backend, GraphicsBackend::Opengl);
                assert_eq!(neither.graphics_backend, GraphicsBackend::Default);
            }
            _ => unreachable!(),
        }
    }
}

/// 只填了一项时，其余各项**不受牵连**（稀疏语义的真判据）。
#[test]
fn one_field_does_not_reset_the_others() {
    let instance = LaunchOverrides {
        memory_mb: Some(8192),
        ..LaunchOverrides::default()
    };
    let global = GlobalLaunchDefaults {
        jvm_args: Some("-XX:+UseZGC".to_string()),
        game_args: Some("--width 1280".to_string()),
        memory_mb: Some(2048),
        java_path: Some(PathBuf::from("C:/java.exe")),
        graphics_backend: Some(GraphicsBackend::Opengl),
    };
    let resolved = resolve(Some(&instance), &global);
    assert_eq!(resolved.memory_mb, 8192, "实例内存赢");
    assert_eq!(
        resolved.global_jvm_args.as_deref(),
        Some("-XX:+UseZGC"),
        "全局 JVM 参数照旧下发（追加语义）"
    );
    assert_eq!(resolved.game_args.as_deref(), Some("--width 1280"));
    assert_eq!(
        resolved.java_path.as_deref(),
        Some(std::path::Path::new("C:/java.exe"))
    );
    assert_eq!(
        resolved.graphics_backend,
        GraphicsBackend::Opengl,
        "实例没给后端 → 落到全局层"
    );
}

/// 空白字符串 = 没填：用户清空输入框必须回到跟随全局，而不是下发一个空参数。
#[test]
fn blank_text_falls_back_to_the_next_layer() {
    let instance = LaunchOverrides {
        jvm_args: Some("   ".to_string()),
        game_args: Some(String::new()),
        java_path: Some(PathBuf::from("  ")),
        ..LaunchOverrides::default()
    };
    assert!(instance.is_empty(), "全空白视为空段");
    let resolved = resolve(
        Some(&instance),
        &GlobalLaunchDefaults::with_jvm_args("-Xmx1g"),
    );
    assert_eq!(resolved.global_jvm_args.as_deref(), Some("-Xmx1g"));
    assert!(resolved.instance_jvm_args.is_none());
    assert!(resolved.game_args.is_none());
    assert!(resolved.java_path.is_none());
}

/// 越界的内存值不算覆盖（手改文件绕得过 schema，绕不过这里）。
#[test]
fn out_of_range_memory_falls_back() {
    for bad in [0, 1, MIN_MAX_MEMORY_MB - 1, MAX_MAX_MEMORY_MB + 1] {
        let instance = LaunchOverrides {
            memory_mb: Some(bad),
            ..LaunchOverrides::default()
        };
        let resolved = resolve(Some(&instance), &GlobalLaunchDefaults::default());
        assert_eq!(
            resolved.memory_mb, DEFAULT_MAX_MEMORY_MB,
            "{bad} MiB 不该生效"
        );
    }
    let ok = resolve(
        Some(&instance_with("memory")),
        &GlobalLaunchDefaults::default(),
    );
    assert_eq!(ok.memory_mb, 6144);
}

/// 无效 `graphicsBackend` 值**拒绝**（不是静默落成 default）。
#[test]
fn unknown_graphics_backend_is_rejected() {
    let error = serde_json::from_str::<LaunchOverrides>(r#"{"graphicsBackend":"vulkan2"}"#)
        .expect_err("未知取值必须报错");
    assert!(
        error.to_string().contains("unknown variant"),
        "错误文案要指出是变体不明：{error}"
    );
    // 大小写与空白容错只发生在 `parse` 这条显式路径上。
    assert_eq!(
        GraphicsBackend::parse(" VULKAN "),
        Some(GraphicsBackend::Vulkan)
    );
    assert_eq!(GraphicsBackend::parse("metal"), None);
    assert_eq!(GraphicsBackend::parse(""), None);
}

/// 三态词表与 `--graphicsBackend` 取值：`default` 不下发。
#[test]
fn backend_flag_values_skip_default() {
    assert_eq!(GraphicsBackend::Default.flag_value(), None);
    assert_eq!(GraphicsBackend::Opengl.flag_value(), Some("opengl"));
    assert_eq!(GraphicsBackend::Vulkan.flag_value(), Some("vulkan"));
    for (backend, id) in GraphicsBackend::ALL.into_iter().zip(GraphicsBackend::IDS) {
        assert_eq!(backend.as_str(), id);
        assert_eq!(GraphicsBackend::parse(id), Some(backend));
    }
}

/// 落盘形状：空段不写键；有值才出现（稀疏），且能原样读回来。
#[test]
fn empty_section_is_not_serialized() {
    let empty = serde_json::to_string(&LaunchOverrides::default()).expect("序列化");
    assert_eq!(empty, "{}");
    let filled = LaunchOverrides {
        memory_mb: Some(4096),
        graphics_backend: Some(GraphicsBackend::Vulkan),
        ..LaunchOverrides::default()
    };
    let json = serde_json::to_string(&filled).expect("序列化");
    assert_eq!(json, r#"{"memoryMb":4096,"graphicsBackend":"vulkan"}"#);
    let back: LaunchOverrides = serde_json::from_str(&json).expect("反序列化");
    assert_eq!(back, filled);
}

/// **补丁语义**：`None` 不动、`Some("")` 清空、非法值报错（GUI 与 CLI 共用这一条实现）。
#[test]
fn patch_touches_only_the_fields_it_is_given() {
    let current = LaunchOverrides {
        jvm_args: Some("-Xss4m".to_string()),
        game_args: Some("--fullscreen".to_string()),
        memory_mb: Some(4096),
        java_path: Some(PathBuf::from("C:/jdk21/bin/java.exe")),
        graphics_backend: Some(GraphicsBackend::Opengl),
    };
    // 只给内存：其余四项原样保留。
    let patched = current
        .patched(&LaunchOverridesPatch {
            memory_mb: Some("8192".to_string()),
            ..LaunchOverridesPatch::default()
        })
        .expect("合法补丁");
    assert_eq!(patched.memory_mb, Some(8192));
    assert_eq!(patched.jvm_args.as_deref(), Some("-Xss4m"));
    assert_eq!(patched.graphics_backend, Some(GraphicsBackend::Opengl));

    // 空串清空；空白也算清空。
    let cleared = current
        .patched(&LaunchOverridesPatch {
            jvm_args: Some("  ".to_string()),
            game_args: Some(String::new()),
            ..LaunchOverridesPatch::default()
        })
        .expect("清空");
    assert!(cleared.jvm_args.is_none());
    assert!(cleared.game_args.is_none());
    assert_eq!(cleared.memory_mb, Some(4096), "没给的项不许被连带清掉");

    // `clear_all` 把五项一起清掉（`default` 后端同样落成「没有覆盖」）。
    let cleared = current
        .patched(&LaunchOverridesPatch::clear_all())
        .expect("全清");
    assert!(cleared.is_empty(), "{cleared:?}");
}

/// 非法补丁值必须报错：内存非整数/越界、后端词表外。
#[test]
fn patch_rejects_invalid_values() {
    let current = LaunchOverrides::default();
    for bad in ["4g", "0", "1", "999999"] {
        let patch = LaunchOverridesPatch {
            memory_mb: Some(bad.to_string()),
            ..LaunchOverridesPatch::default()
        };
        assert!(current.patched(&patch).is_err(), "`{bad}` 不该通过内存校验");
    }
    for bad in ["vulkan2", "metal", "gl"] {
        let error = current
            .patched(&LaunchOverridesPatch::backend(bad))
            .expect_err("未知后端要报错");
        assert!(error.contains(bad), "{error}");
        assert!(error.contains("vulkan"), "错误文案要给出可选值：{error}");
    }
    // 大小写容错走 `parse`（显式路径），落盘仍是小写稳定 id。
    let patched = current
        .patched(&LaunchOverridesPatch::backend(" VULKAN "))
        .expect("大小写容错");
    assert_eq!(patched.graphics_backend, Some(GraphicsBackend::Vulkan));
    assert_eq!(
        serde_json::to_string(&patched).expect("序列化"),
        r#"{"graphicsBackend":"vulkan"}"#
    );
}
