//! 向导「数据目录」输入框的初值来源。
//!
//! 缺陷现场：二次安装时数据目录被置灰不可改（正确），但显示的仍是清单默认模板
//! `%APPDATA%\{id}`——控件禁用只保证改不了，值却没读回 `datadir.conf`，于是一个
//! 装在 D 盘的数据目录会被界面显示成 `%APPDATA%`，用户看到的是假的。
//!
//! 断言的是 `default_data_dir()`（界面初值本身），而非底层的 `read_datadir_conf()`：
//! 只测底层读函数漏得掉「读了但没接到界面上」。
//!
//! 三个场景写在同一个 test 里顺序执行：它们共享同一个 `LOCALAPPDATA` 临时目录与
//! 同一份 conf 文件，拆成多个 test 会被 cargo 的并发调度打乱。
//!
//! 文件名不含 "install"/"setup"/"update"/"patch"：命中 Windows UAC 安装器检测
//! 启发式的测试二进制无法在普通权限下启动（os error 740）。

#![cfg(windows)]

use wind_installer::manifest::AppManifest;
use wind_installer::{meta, ui::install_wizard, uninstaller::cleanup};

const MANIFEST: &str = r#"
[app]
id           = "Demo"
display_name = "Demo App"
version      = "1.0.0"
publisher    = "Demo Inc"
main_exe     = "demo.exe"

[datadir]
conf_file = "datadir.conf"
"#;

#[test]
fn data_dir_initial_value_follows_conf_file() {
    meta::init(AppManifest::from_toml_bytes(MANIFEST.as_bytes()).expect("解析清单失败"));

    let root = std::env::temp_dir().join(format!("wind-installer-datadir-{}", std::process::id()));
    let conf_dir = root.join("Demo");
    std::fs::create_dir_all(&conf_dir).expect("建临时目录失败");
    std::env::set_var("LOCALAPPDATA", &root);
    let conf = conf_dir.join("datadir.conf");

    // 首装（conf 尚不存在）→ 清单默认模板
    let _ = std::fs::remove_file(&conf);
    assert_eq!(install_wizard::default_data_dir(), r"%APPDATA%\Demo");

    // 二次安装：conf 已记录用户当初选的目录 → 界面必须显示它，而不是默认模板
    std::fs::write(&conf, r"D:\MyData\Demo").expect("写 conf 失败");
    assert_eq!(install_wizard::default_data_dir(), r"D:\MyData\Demo");

    // 尾部换行是文本文件常态，不能把它当成路径的一部分
    std::fs::write(&conf, "D:\\MyData\\Demo\r\n").expect("写 conf 失败");
    assert_eq!(install_wizard::default_data_dir(), r"D:\MyData\Demo");

    // 内容为空白 → 回退默认；主程序读端同样回退，二者必须一致
    std::fs::write(&conf, "  \r\n").expect("写 conf 失败");
    assert_eq!(install_wizard::default_data_dir(), r"%APPDATA%\Demo");

    // 卸载侧同一份真相：确认页显示的路径必须就是 remove_dir_all 要删的那个。
    // 曾经的现场是两处硬编码 `%APPDATA%\{id}`——用户在同意删 A 的前提下把 B 删了。
    std::fs::write(&conf, r"D:\MyData\Demo").expect("写 conf 失败");
    let shown = cleanup::resolve_user_data_dir();
    let deleted = cleanup::CleanupOptions::default().user_data_dir();
    assert_eq!(shown, std::path::PathBuf::from(r"D:\MyData\Demo"));
    assert_eq!(shown, deleted, "确认页显示的路径与实际删除的路径不一致");
    assert_eq!(
        shown.to_string_lossy(),
        install_wizard::default_data_dir(),
        "安装向导与卸载器对同一台机器的数据目录给出了不同答案"
    );

    let _ = std::fs::remove_dir_all(&root);
}
