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

/// 仓库自带的 app.toml 必须始终能被当前结构体解析——防止配置与代码漂移。
#[test]
fn repo_app_toml_parses_with_all_capability_sections() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/app.toml");
    let text = std::fs::read_to_string(path).expect("读取仓库 app.toml 失败");
    let cfg = ProjectConfig::from_toml_str(&text).expect("解析仓库 app.toml 失败");

    let autostart = cfg.manifest.autostart.as_ref().expect("应有 autostart 段");
    assert!(autostart.enabled);
    assert_eq!(autostart.exe_or("fallback.exe"), "wind_input.exe");

    assert_eq!(cfg.manifest.shortcut.len(), 2);
    assert_eq!(cfg.manifest.shortcut[0].effective_name(), "清风输入法 设置");

    assert!(cfg.manifest.startup.as_ref().expect("应有 startup 段").prestart);
    assert_eq!(
        cfg.manifest.datadir.as_ref().expect("应有 datadir 段").conf_file,
        "datadir.conf"
    );

    // UI 段：仓库自带清单显式声明了输入法专用文案，内置默认保持中性
    assert_eq!(cfg.manifest.theme.accent, "#4C8BF5");
    assert_eq!(cfg.manifest.paths.install, r"%ProgramFiles%\{id}");
    assert_eq!(cfg.manifest.strings.data_dir_hint, "词库、配置路径");
}

/// 占位符使一份快捷方式配置对 dev/release 变体通用：同一段 config，按各变体的
/// [app] 字段展开出不同的 target/name。这是消除「dev.ps1 双真相」的关键机制。
#[test]
fn shortcut_placeholders_expand_per_variant() {
    use wind_installer::manifest::expand_placeholders;

    // release 变体
    assert_eq!(
        expand_placeholders("{setting_exe}", "wind_input.exe", "wind_setting.exe", "清风输入法", "WindInput"),
        "wind_setting.exe"
    );
    assert_eq!(
        expand_placeholders("{display_name} 设置", "wind_input.exe", "wind_setting.exe", "清风输入法", "WindInput"),
        "清风输入法 设置"
    );

    // dev 变体：同一份 config 文本，展开出 dev 的 exe 名与显示名
    assert_eq!(
        expand_placeholders("{setting_exe}", "wind_input_dev.exe", "wind_setting_dev.exe", "清风输入法 (开发版)", "WindInputDev"),
        "wind_setting_dev.exe"
    );
    assert_eq!(
        expand_placeholders("卸载 {display_name}", "wind_input_dev.exe", "wind_setting_dev.exe", "清风输入法 (开发版)", "WindInputDev"),
        "卸载 清风输入法 (开发版)"
    );
}

/// 未配置设置程序时 {setting_exe} 展开为空——create_shortcuts 据此跳过该快捷方式，
/// 无需在配置里为「有/无设置程序」各写一份。
#[test]
fn setting_exe_placeholder_empty_when_unset() {
    use wind_installer::manifest::expand_placeholders;
    let expanded = expand_placeholders("{setting_exe}", "app.exe", "", "App", "App");
    assert!(expanded.trim().is_empty());
}

/// 无占位符的字面 target 原样保留。
#[test]
fn literal_target_passes_through_unchanged() {
    use wind_installer::manifest::expand_placeholders;
    assert_eq!(
        expand_placeholders("uninstall.exe", "app.exe", "set.exe", "App", "App"),
        "uninstall.exe"
    );
}

/// 能力段的空值回退：name 取 target 文件名，exe 回退 main_exe。
#[test]
fn capability_fallbacks_resolve_from_target_and_main_exe() {
    let toml = r#"
[app]
id           = "MyApp"
display_name = "My App"
version      = "1.0.0"
publisher    = "Me"
main_exe     = "app.exe"

[autostart]

[[shortcut]]
target = "bin/tool.exe"

[startup]
prestart = true
"#;
    let m = AppManifest::from_toml_bytes(toml.as_bytes()).expect("解析失败");

    let autostart = m.autostart.as_ref().unwrap();
    assert!(autostart.enabled, "段落存在即默认启用");
    assert_eq!(autostart.exe_or(&m.app.main_exe), "app.exe");

    // name 留空 → 取 target 的文件名去扩展名；description 留空 → 回退 name
    assert_eq!(m.shortcut[0].effective_name(), "tool");
    assert_eq!(m.shortcut[0].effective_description(), "tool");

    assert_eq!(m.startup.as_ref().unwrap().exe_or(&m.app.main_exe), "app.exe");
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

#[test]
fn project_config_parses_version_info() {
    let toml_str = r#"
[app]
id           = "MyApp"
display_name = "My App"
version      = "1.0.0"
publisher    = "Me"
main_exe     = "myapp.exe"

[package]
source_dir = "./build"
[package.version_info]
company_name      = "My Company"
file_description  = "My File Description"
file_version      = "1.0.0.0"
product_name      = "My Product"
product_version   = "1.0.0.0"
copyright         = "Copyright (c) 2026"
original_filename = "myapp.exe"
"#;
    let cfg = ProjectConfig::from_toml_str(toml_str).expect("解析包含 version_info 的配置失败");
    let version_info = cfg.package.version_info.as_ref().expect("应当解析出 version_info");
    assert_eq!(version_info.company_name.as_deref(), Some("My Company"));
    assert_eq!(version_info.file_description.as_deref(), Some("My File Description"));
    assert_eq!(version_info.file_version.as_deref(), Some("1.0.0.0"));
    assert_eq!(version_info.product_name.as_deref(), Some("My Product"));
    assert_eq!(version_info.product_version.as_deref(), Some("1.0.0.0"));
    assert_eq!(version_info.copyright.as_deref(), Some("Copyright (c) 2026"));
    assert_eq!(version_info.original_filename.as_deref(), Some("myapp.exe"));

    // 验证缺省情况
    let toml_str_empty_info = r#"
[app]
id           = "MyApp"
display_name = "My App"
version      = "1.0.0"
publisher    = "Me"
main_exe     = "myapp.exe"

[package]
source_dir = "./build"
[package.version_info]
"#;
    let cfg_empty = ProjectConfig::from_toml_str(toml_str_empty_info).expect("解析空 version_info 失败");
    let info_empty = cfg_empty.package.version_info.as_ref().expect("空 version_info 块也应解析出 Option");
    assert!(info_empty.company_name.is_none());
    assert!(info_empty.file_description.is_none());

    // 验证完全不提供 version_info 时
    let toml_str_no_info = r#"
[app]
id           = "MyApp"
display_name = "My App"
version      = "1.0.0"
publisher    = "Me"
main_exe     = "myapp.exe"

[package]
source_dir = "./build"
"#;
    let cfg_no_info = ProjectConfig::from_toml_str(toml_str_no_info).expect("不包含 version_info 应解析成功");
    assert!(cfg_no_info.package.version_info.is_none());
}

