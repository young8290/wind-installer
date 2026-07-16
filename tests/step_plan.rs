//! 安装计划装配测试。
//!
//! `plan_install` 是「装什么、按什么顺序装」的唯一真相，且是纯函数（清单 → 步骤名），
//! 不触碰系统，故可无管理员权限直接断言。核心契约：**能力段缺省 = 该步骤不入计划**——
//! 这正是通用安装器的立身之本：一个普通应用的清单不该带出任何输入法/字体步骤。
//!
//! 文件名不含 "install"/"setup"/"update"/"patch"：Windows 的 UAC 安装器检测启发式会
//! 按文件名自动要求提权，命中后测试二进制将无法在普通权限下启动（os error 740）。

#![cfg(windows)]

use wind_installer::installer::plan::plan_install;
use wind_installer::installer::InstallMode;
use wind_installer::manifest::AppManifest;

/// 最小清单：只有应用身份，不声明任何能力。
const MINIMAL: &str = r#"
[app]
id           = "Demo"
display_name = "Demo App"
version      = "1.0.0"
publisher    = "Demo Inc"
main_exe     = "demo.exe"
"#;

/// 全能力清单：每个能力段都声明。
const FULL: &str = r#"
[app]
id            = "Demo"
display_name  = "Demo App"
version       = "1.0.0"
publisher     = "Demo Inc"
main_exe      = "demo.exe"
setting_exe   = "demo_setting.exe"
url_protocol  = "demo"
process_names = ["demo"]
acl_dlls      = ["demo.dll"]
legacy_files  = ["old.dll"]

[ime]
clsid        = "{00000000-0000-0000-0000-000000000001}"
profile_guid = "{00000000-0000-0000-0000-000000000002}"
lang_id      = "0804"
dll_x64      = "demo_tsf.dll"

[[font]]
file         = "Demo.ttf"
display_name = "Demo (TrueType)"
source_rel   = "data/Demo.ttf"

[autostart]
exe = "demo.exe"

[[shortcut]]
target = "demo.exe"

[startup]
prestart = true

[datadir]
conf_file = "datadir.conf"
"#;

fn plan_names(toml: &str, mode: InstallMode) -> Vec<String> {
    let m = AppManifest::from_toml_bytes(toml.as_bytes()).expect("解析清单失败");
    plan_install(&m, mode).iter().map(|s| s.name()).collect()
}

#[test]
fn portable_plan_only_extracts_and_marks() {
    let names = plan_names(MINIMAL, InstallMode::Portable);
    assert_eq!(
        names,
        ["正在解压数据...", "正在释放文件...", "正在写入便携模式标记..."]
    );
}

#[test]
fn portable_plan_never_touches_system_even_with_all_capabilities() {
    // 便携模式的全部语义 = 解压 + 标记：即便清单声明了输入法/字体/自启动，也一律不执行
    let names = plan_names(FULL, InstallMode::Portable);
    assert_eq!(
        names,
        ["正在解压数据...", "正在释放文件...", "正在写入便携模式标记..."]
    );
}

/// 一个只声明身份的普通应用：计划里只剩「解压 + 可卸载」这些任何安装器都要做的事，
/// 不含任何输入法/字体/数据目录约定。这是通用安装器的核心断言。
#[test]
fn minimal_manifest_plans_no_capability_steps() {
    let names = plan_names(MINIMAL, InstallMode::Standard);
    assert_eq!(
        names,
        [
            "正在准备安装环境...",
            "正在解压数据...",
            "正在释放文件...",
            "正在写入卸载器清单...",
            "正在写入卸载信息...",
            "正在完成安装...",
        ]
    );
}

#[test]
fn datadir_conf_is_opt_in() {
    // datadir.conf 是输入法的约定，普通应用不该被塞一个它永远不读的文件
    assert!(!plan_names(MINIMAL, InstallMode::Standard)
        .iter()
        .any(|n| n.contains("数据目录")));

    let toml = format!("{}\n[datadir]\n", MINIMAL);
    assert!(plan_names(&toml, InstallMode::Standard)
        .iter()
        .any(|n| n.contains("数据目录")));
}

#[test]
fn full_manifest_plans_every_capability_in_order() {
    let names = plan_names(FULL, InstallMode::Standard);
    assert_eq!(
        names,
        [
            "正在准备安装环境...",
            "正在停止旧进程...",
            "正在反注册旧 COM...",
            "正在清理旧版遗留文件...",
            "正在解压数据...",
            "正在释放文件...",
            "正在写入卸载器清单...",
            "正在设置文件权限...",
            "正在安装字体...",
            "正在注册 COM 组件...",
            "正在注册系统输入法...",
            "正在配置开机自启动...",
            "正在注册协议...",
            "正在创建快捷方式...",
            "正在写入卸载信息...",
            "正在写入数据目录配置...",
            "正在启动服务...",
            "正在完成安装...",
        ]
    );
}

#[test]
fn autostart_can_be_declared_but_disabled() {
    let toml = format!("{}\n[autostart]\nenabled = false\n", MINIMAL);
    let names = plan_names(&toml, InstallMode::Standard);
    assert!(!names.iter().any(|n| n.contains("自启动")));
}

#[test]
fn startup_section_without_prestart_does_not_launch() {
    let toml = format!("{}\n[startup]\nprestart = false\n", MINIMAL);
    let names = plan_names(&toml, InstallMode::Standard);
    assert!(!names.iter().any(|n| n.contains("启动服务")));
}

#[test]
fn install_and_clear_flags_bracket_the_standard_plan() {
    // InstallerRunning 标志必须首尾配对，否则宿主进程会永久停摆
    let names = plan_names(FULL, InstallMode::Standard);
    assert_eq!(names.first().unwrap(), "正在准备安装环境...");
    assert_eq!(names.last().unwrap(), "正在完成安装...");
}
