//! 原子写：覆盖语义、父目录创建、不留临时文件、可选读。

use unlucky_core::platform::atomic::{read_optional, tmp_path, write_atomic};

use crate::common::{dir_entries, TempDir};

#[test]
fn writes_and_replaces_in_place() {
    let temp = TempDir::new("atomic");
    let path = temp.child("config/launcher.json");

    write_atomic(&path, b"{\"a\":1}").expect("首次写");
    assert_eq!(temp.read("config/launcher.json"), "{\"a\":1}");

    write_atomic(&path, b"{\"a\":2}").expect("覆盖写");
    assert_eq!(temp.read("config/launcher.json"), "{\"a\":2}");

    // 目录里只该有目标文件，临时文件必须被改名掉。
    assert_eq!(dir_entries(&temp.child("config")), vec!["launcher.json"]);
}

#[test]
fn read_optional_distinguishes_missing_from_failure() {
    let temp = TempDir::new("atomic-read");
    let path = temp.child("nope.json");
    assert!(read_optional(&path).expect("读缺失文件").is_none());

    temp.write("here.json", "内容");
    let bytes = read_optional(&temp.child("here.json"))
        .expect("读文件")
        .expect("有内容");
    assert_eq!(bytes, "内容".as_bytes());
}

#[test]
fn tmp_name_stays_in_same_directory() {
    let temp = TempDir::new("atomic-tmp");
    let path = temp.child("config/profiles.json");
    let tmp = tmp_path(&path);
    assert_eq!(
        tmp.parent(),
        path.parent(),
        "临时文件必须同目录（rename 才原子）"
    );
    assert_ne!(tmp, path);
    let first = tmp_path(&path);
    let second = tmp_path(&path);
    assert_ne!(first, second, "并发写要能拿到不同的临时名");
}
