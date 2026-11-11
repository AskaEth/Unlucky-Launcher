//! `.minecraft` 目录布局：路径规则、Maven 坐标、已安装版本枚举。

use std::path::PathBuf;

use unlucky_core::game::error::GameError;
use unlucky_core::game::layout::GameLayout;
use unlucky_core::game::library::MavenCoordinate;

use crate::helpers::TempDir;

fn layout() -> (TempDir, GameLayout) {
    let temp = TempDir::new("layout");
    let layout = GameLayout::new(temp.path().to_path_buf());
    (temp, layout)
}

#[test]
fn version_paths_follow_mojang_layout() {
    let (_temp, layout) = layout();
    let root = layout.root();

    assert_eq!(layout.versions_dir(), root.join("versions"));
    assert_eq!(layout.version_dir("1.20.1"), root.join("versions/1.20.1"));
    assert_eq!(
        layout.version_json("1.20.1"),
        root.join("versions/1.20.1/1.20.1.json")
    );
    assert_eq!(
        layout.version_jar("1.20.1"),
        root.join("versions/1.20.1/1.20.1.jar")
    );
    // ULL 扩展：实例私有状态与 natives 解压目录。
    assert_eq!(
        layout.version_state_dir("1.20.1"),
        root.join("versions/1.20.1/.ull")
    );
    assert_eq!(
        layout.natives_dir("1.20.1"),
        root.join("versions/1.20.1/natives")
    );
    assert_eq!(
        layout.version_libraries_dir("1.20.1"),
        root.join("versions/1.20.1/libraries")
    );
    assert_eq!(layout.ull_root(), root.join(".ull"));
}

#[test]
fn asset_paths_use_two_level_sharding() {
    let (_temp, layout) = layout();
    let root = layout.root();

    assert_eq!(layout.asset_index("5"), root.join("assets/indexes/5.json"));
    let hash = "a3b3b1b0e0d8f2a8f0fbe1a4b0a8f6a1c6c6a7a2";
    assert_eq!(
        layout.asset_object(hash).expect("哈希合法"),
        root.join("assets/objects/a3").join(hash)
    );
    assert_eq!(
        layout.log_config("client-1.12.xml"),
        root.join("assets/log_configs/client-1.12.xml")
    );

    let error = layout.asset_object("a").expect_err("短哈希应报错");
    assert!(matches!(error, GameError::InvalidAssetHash { .. }));
}

#[test]
fn maven_coordinates_parse_and_lay_out() {
    let coordinate = MavenCoordinate::parse("org.lwjgl:lwjgl:3.3.2").expect("坐标应可解析");
    assert_eq!(coordinate.group, "org.lwjgl");
    assert_eq!(coordinate.group_path(), "org/lwjgl");
    assert_eq!(coordinate.file_name(), "lwjgl-3.3.2.jar");
    assert_eq!(
        coordinate.relative_path(),
        PathBuf::from("org/lwjgl/lwjgl/3.3.2/lwjgl-3.3.2.jar")
    );

    let natives =
        MavenCoordinate::parse("org.lwjgl:lwjgl:3.3.2:natives-windows-64").expect("带分类器");
    assert_eq!(
        natives.relative_path(),
        PathBuf::from("org/lwjgl/lwjgl/3.3.2/lwjgl-3.3.2-natives-windows-64.jar")
    );

    let non_jar = MavenCoordinate::parse("net.java:jinput:1.0.0@zip").expect("带扩展名");
    assert_eq!(non_jar.file_name(), "jinput-1.0.0.zip");
}

#[test]
fn invalid_coordinates_are_rejected() {
    for raw in ["a:b", "a::1", "a:b:c:d:e"] {
        let error = MavenCoordinate::parse(raw).expect_err("不合法坐标应报错");
        assert!(
            matches!(error, GameError::InvalidCoordinate { .. }),
            "{raw}"
        );
    }
}

#[test]
fn library_paths_land_under_libraries() {
    let (_temp, layout) = layout();
    let root = layout.root();
    let coordinate = MavenCoordinate::parse("com.mojang:logging:1.1.1").expect("坐标应可解析");

    assert_eq!(
        layout.library(&coordinate),
        root.join("libraries/com/mojang/logging/1.1.1/logging-1.1.1.jar")
    );
    assert_eq!(
        layout.version_library("1.20.1", &coordinate),
        root.join("versions/1.20.1/libraries/com/mojang/logging/1.1.1/logging-1.1.1.jar")
    );
}

#[test]
fn installed_versions_requires_manifest_file() {
    let (_temp, layout) = layout();

    assert!(
        layout
            .installed_versions()
            .expect("缺目录视为空")
            .is_empty(),
        "目录不存在时不能报错"
    );

    let version_dir = layout.version_dir("1.20.1");
    std::fs::create_dir_all(&version_dir).expect("建目录");
    std::fs::write(layout.version_json("1.20.1"), "{}").expect("写清单");
    // 只有 jar 没有 json 的目录不算已安装。
    let broken = layout.version_dir("1.19.4");
    std::fs::create_dir_all(&broken).expect("建目录");
    std::fs::write(layout.version_jar("1.19.4"), b"jar").expect("写 jar");

    assert!(layout.is_version_installed("1.20.1"));
    assert!(!layout.is_version_installed("1.19.4"));
    assert_eq!(
        layout.installed_versions().expect("应可枚举"),
        vec!["1.20.1".to_owned()]
    );
}
