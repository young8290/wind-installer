use std::path::Path;

use winreg::enums::*;
use winreg::RegKey;

use super::config::InstallConfig;

/// 卸载信息注册表路径
const UNINST_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\清风输入法";

/// 设置开机自启动
pub fn set_auto_start(install_dir: &Path) -> Result<(), String> {
    let exe_path = install_dir.join("wind_input.exe");
    let exe_path_str = exe_path.to_string_lossy().to_string();

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let run_key = hkcu
        .open_subkey_with_flags(
            r"Software\Microsoft\Windows\CurrentVersion\Run",
            KEY_WRITE,
        )
        .map_err(|e| format!("Failed to open Run key: {}", e))?;

    run_key
        .set_value("WindInput", &format!("\"{}\"", exe_path_str))
        .map_err(|e| format!("Failed to set auto-start: {}", e))?;

    Ok(())
}

/// 移除开机自启动
pub fn remove_auto_start() -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let run_key = hkcu
        .open_subkey_with_flags(
            r"Software\Microsoft\Windows\CurrentVersion\Run",
            KEY_WRITE,
        )
        .map_err(|e| format!("Failed to open Run key: {}", e))?;

    run_key
        .delete_value("WindInput")
        .map_err(|e| format!("Failed to remove auto-start: {}", e))?;

    Ok(())
}

/// 注册 windinput:// URL 协议
pub fn register_url_protocol(install_dir: &Path) -> Result<(), String> {
    let setting_exe = install_dir.join("wind_setting.exe");
    let setting_exe_str = setting_exe.to_string_lossy().to_string();

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let classes_key = hkcu
        .open_subkey_with_flags(r"Software\Classes", KEY_WRITE)
        .map_err(|e| format!("Failed to open Classes key: {}", e))?;

    // 创建 windinput 协议键
    let protocol_key = classes_key
        .create_subkey("windinput")
        .map_err(|e| format!("Failed to create protocol key: {}", e))?
        .0;

    protocol_key
        .set_value("", &"URL:清风输入法协议")
        .map_err(|e| format!("Failed to set protocol description: {}", e))?;
    protocol_key
        .set_value("URL Protocol", &"")
        .map_err(|e| format!("Failed to set URL Protocol flag: {}", e))?;

    // 设置处理命令
    let command_key = protocol_key
        .create_subkey(r"shell\open\command")
        .map_err(|e| format!("Failed to create command key: {}", e))?
        .0;

    command_key
        .set_value("", &format!("\"{}\" \"%1\"", setting_exe_str))
        .map_err(|e| format!("Failed to set command: {}", e))?;

    Ok(())
}

/// 移除 URL 协议注册
pub fn unregister_url_protocol() -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let classes_key = hkcu
        .open_subkey_with_flags(r"Software\Classes", KEY_WRITE)
        .map_err(|e| format!("Failed to open Classes key: {}", e))?;

    classes_key
        .delete_subkey_all("windinput")
        .map_err(|e| format!("Failed to remove protocol key: {}", e))?;

    Ok(())
}

/// 写入卸载信息到注册表
pub fn write_uninstall_info(config: &InstallConfig) -> Result<(), String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let (uninst_key, _) = hklm
        .create_subkey(UNINST_KEY)
        .map_err(|e| format!("Failed to create uninstall key: {}", e))?;

    let install_dir_str = config.install_dir.to_string_lossy().to_string();
    let uninstall_exe = config.install_dir.join("uninstall.exe");
    let uninstall_exe_str = uninstall_exe.to_string_lossy().to_string();

    uninst_key
        .set_value("DisplayName", &config.app_name)
        .map_err(|e| format!("Failed to set DisplayName: {}", e))?;
    uninst_key
        .set_value("DisplayVersion", &config.app_version)
        .map_err(|e| format!("Failed to set DisplayVersion: {}", e))?;
    uninst_key
        .set_value("Publisher", &config.publisher)
        .map_err(|e| format!("Failed to set Publisher: {}", e))?;
    uninst_key
        .set_value("InstallLocation", &install_dir_str)
        .map_err(|e| format!("Failed to set InstallLocation: {}", e))?;
    uninst_key
        .set_value("UninstallString", &format!("\"{}\" --uninstall", uninstall_exe_str))
        .map_err(|e| format!("Failed to set UninstallString: {}", e))?;
    uninst_key
        .set_value("QuietUninstallString", &format!("\"{}\" --uninstall --silent", uninstall_exe_str))
        .map_err(|e| format!("Failed to set QuietUninstallString: {}", e))?;

    // 图标（使用安装器主程序图标，第一个图标资源）
    let icon_str = format!("\"{}\",0", uninstall_exe_str);
    uninst_key
        .set_value("DisplayIcon", &icon_str)
        .map_err(|e| format!("Failed to set DisplayIcon: {}", e))?;

    // 系统设置"应用"列表所需标记
    let one: u32 = 1;
    uninst_key
        .set_value("NoModify", &one)
        .map_err(|e| format!("Failed to set NoModify: {}", e))?;
    uninst_key
        .set_value("NoRepair", &one)
        .map_err(|e| format!("Failed to set NoRepair: {}", e))?;

    // 计算安装大小
    if let Ok(size) = get_dir_size(&config.install_dir) {
        let size_kb = (size / 1024) as u32;
        uninst_key
            .set_value("EstimatedSize", &size_kb)
            .map_err(|e| format!("Failed to set EstimatedSize: {}", e))?;
    }

    Ok(())
}

/// 移除卸载信息
pub fn remove_uninstall_info() -> Result<(), String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    hklm.delete_subkey_all(UNINST_KEY)
        .map_err(|e| format!("Failed to remove uninstall key: {}", e))?;

    Ok(())
}

/// 获取目录大小（字节）
fn get_dir_size(path: &Path) -> Result<u64, String> {
    let mut total = 0u64;

    for entry in std::fs::read_dir(path)
        .map_err(|e| format!("Failed to read directory: {}", e))?
    {
        let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        let metadata = entry
            .metadata()
            .map_err(|e| format!("Failed to get metadata: {}", e))?;

        if metadata.is_dir() {
            total += get_dir_size(&entry.path())?;
        } else {
            total += metadata.len();
        }
    }

    Ok(total)
}

/// 设置安装器运行标记（防止 wind_tsf.dll 在安装期间重拉服务）
pub fn set_installer_running() -> Result<(), String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let (key, _) = hklm
        .create_subkey("Software\\WindInput")
        .map_err(|e| format!("Failed to create WindInput key: {}", e))?;

    key.set_value("InstallerRunning", &"1")
        .map_err(|e| format!("Failed to set InstallerRunning: {}", e))?;

    Ok(())
}

/// 清除安装器运行标记
pub fn clear_installer_running() -> Result<(), String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    if let Ok(key) = hklm.open_subkey_with_flags("Software\\WindInput", KEY_WRITE) {
        let _ = key.delete_value("InstallerRunning");
    }
    Ok(())
}

/// 检测已安装版本
#[allow(dead_code)]
pub fn detect_installed_version() -> Option<String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    if let Ok(key) = hklm.open_subkey_with_flags(UNINST_KEY, KEY_READ) {
        if let Ok(version) = key.get_value::<String, _>("DisplayVersion") {
            return Some(version);
        }
    }
    None
}

/// 获取已安装版本的卸载命令
#[allow(dead_code)]
pub fn get_uninstall_string() -> Option<String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    if let Ok(key) = hklm.open_subkey_with_flags(UNINST_KEY, KEY_READ) {
        if let Ok(cmd) = key.get_value::<String, _>("UninstallString") {
            return Some(cmd);
        }
    }
    None
}
