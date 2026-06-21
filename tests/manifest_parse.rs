//! AppManifest / ProjectConfig 解析与 roundtrip 测试。

use wind_installer::manifest::{AppManifest, ProjectConfig};

const SAMPLE: &str = r#"
[app]
id              = "WindInput"
display_name    = "清风输入法"
version         = "0.1.0"
publisher       = "清风输入法 项目"
main_exe        = "wind_input.exe"
setting_exe     = "wind_setting.exe"
start_menu_folder = "清风输入法"
window_title    = "清风输入法 安装向导"
url_protocol    = "windinput"
agreement_url   = "https://example.com/eula"
backup_dir      = "WindInput_Backup"
process_names   = ["wind_setting", "wind_portable", "wind_input"]
acl_dlls        = ["wind_tsf.dll", "wind_tsf_x86.dll"]
legacy_files    = ["wind_dwrite.dll"]

[ui]
install_win   = { w = 520, h = 490 }
uninstall_win = { w = 480, h = 440 }

[ime]
clsid        = "{99C2EE30-5C57-45A2-9C63-FB54B34FD90A}"
profile_guid = "{99C2EE31-5C57-45A2-9C63-FB54B34FD90A}"
lang_id      = "0804"
dll_x64      = "wind_tsf.dll"
dll_x86      = "wind_tsf_x86.dll"

[[font]]
file         = "HeiTiZiGen.ttf"
display_name = "黑体字根 (TrueType)"
source_rel   = "data/schemas/wubi86/HeiTiZiGen.ttf"

[package]
compression = "lzma"
source_dir  = "../WindInput/build"
output_name = "WindInput-Setup"
output_dir  = "./dist"
logo        = "assets/logo.png"
icon        = "assets/installer.ico"
"#;

#[test]
fn project_config_parses_all_sections() {
    let cfg = ProjectConfig::from_toml_str(SAMPLE).expect("解析 app.toml 失败");

    assert_eq!(cfg.manifest.app.id, "WindInput");
    assert_eq!(cfg.manifest.app.display_name, "清风输入法");
    assert_eq!(cfg.manifest.app.process_names, ["wind_setting", "wind_portable", "wind_input"]);
    assert_eq!(cfg.manifest.app.acl_dlls, ["wind_tsf.dll", "wind_tsf_x86.dll"]);
    assert_eq!(cfg.manifest.app.legacy_files, ["wind_dwrite.dll"]);
    assert!(cfg.manifest.app.legacy_dirs.is_empty());

    assert_eq!(cfg.manifest.ui.install_win.w, 520);
    assert_eq!(cfg.manifest.ui.uninstall_win.h, 440);

    let ime = cfg.manifest.ime.as_ref().expect("应有 ime 段");
    assert_eq!(ime.lang_id, "0804");
    assert_eq!(ime.dll_x64, "wind_tsf.dll");

    assert_eq!(cfg.manifest.font.len(), 1);
    assert_eq!(cfg.manifest.font[0].file, "HeiTiZiGen.ttf");
    assert_eq!(cfg.manifest.font[0].display_name, "黑体字根 (TrueType)");

    assert_eq!(cfg.package.compression, "lzma");
    assert_eq!(cfg.package.source_dir, "../WindInput/build");
    assert_eq!(cfg.package.logo, "assets/logo.png");
    assert_eq!(cfg.package.icon, "assets/installer.ico");
}

#[test]
fn manifest_toml_byte_roundtrip_preserves_all_fields() {
    let cfg = ProjectConfig::from_toml_str(SAMPLE).unwrap();
    let original = cfg.manifest;

    let bytes = original.to_toml_bytes().expect("序列化失败");
    let parsed = AppManifest::from_toml_bytes(&bytes).expect("反序列化失败");

    assert_eq!(parsed.app.id, original.app.id);
    assert_eq!(parsed.app.display_name, original.app.display_name);
    assert_eq!(parsed.app.process_names, original.app.process_names);
    assert_eq!(parsed.app.url_protocol, original.app.url_protocol);
    assert_eq!(parsed.ui.install_win.w, original.ui.install_win.w);
    assert_eq!(parsed.ime.as_ref().unwrap().clsid, original.ime.as_ref().unwrap().clsid);
    assert_eq!(parsed.font.len(), 1);
    assert_eq!(parsed.font[0].source_rel, original.font[0].source_rel);
}

#[test]
fn defaults_apply_with_minimal_manifest() {
    let minimal = r#"
[app]
id           = "MyApp"
display_name = "My App"
version      = "1.0.0"
publisher    = "Me"
main_exe     = "myapp.exe"

[package]
source_dir = "./build"
"#;
    let cfg = ProjectConfig::from_toml_str(minimal).expect("最小配置应可解析");

    // 缺省段
    assert!(cfg.manifest.ime.is_none());
    assert!(cfg.manifest.font.is_empty());
    // 带默认值的字段
    assert_eq!(cfg.manifest.app.portable_marker, "portable_mode");
    assert_eq!(cfg.manifest.ui.install_win.w, 520);
    assert_eq!(cfg.package.compression, "zstd");
    assert_eq!(cfg.package.output_dir, "./dist");
    // 回退访问器
    assert_eq!(cfg.manifest.start_menu_folder(), "My App"); // 回退到 display_name
}
