use std::path::Path;

use crate::meta;

/// 创建开始菜单快捷方式
pub fn create_shortcuts(install_dir: &Path) -> Result<(), String> {
    let start_menu_dir = get_start_menu_folder();
    let setting_exe = install_dir.join(meta::setting_exe());
    let uninstall_exe = install_dir.join("uninstall.exe");

    std::fs::create_dir_all(&start_menu_dir)
        .map_err(|e| format!("Failed to create start menu directory: {}", e))?;

    if setting_exe.exists() {
        let shortcut_path = start_menu_dir.join(format!("{} 设置.lnk", meta::app_display_name()));
        create_shortcut(
            &setting_exe,
            &shortcut_path,
            Some(&install_dir.to_string_lossy()),
            Some(&format!("{} 设置", meta::app_display_name())),
        )?;
    }

    if uninstall_exe.exists() {
        let shortcut_path = start_menu_dir.join(format!("卸载 {}.lnk", meta::app_display_name()));
        create_shortcut(
            &uninstall_exe,
            &shortcut_path,
            Some(&install_dir.to_string_lossy()),
            Some(&format!("卸载 {}", meta::app_display_name())),
        )?;
    }

    Ok(())
}

/// 创建单个快捷方式
fn create_shortcut(
    target: &Path,
    shortcut_path: &Path,
    working_dir: Option<&str>,
    description: Option<&str>,
) -> Result<(), String> {
    let mut link = mslnk::ShellLink::new(target)
        .map_err(|e| format!("Failed to create link: {}", e))?;

    if let Some(dir) = working_dir {
        link.set_working_dir(Some(dir.to_string()));
    }

    if let Some(desc) = description {
        link.set_name(Some(desc.to_string()));
    }

    link.create_lnk(shortcut_path)
        .map_err(|e| format!("Failed to save shortcut: {}", e))?;

    Ok(())
}

/// 删除开始菜单快捷方式
pub fn delete_shortcuts() -> Result<(), String> {
    let start_menu_dir = get_start_menu_folder();

    if start_menu_dir.exists() {
        std::fs::remove_dir_all(&start_menu_dir)
            .map_err(|e| format!("Failed to remove start menu directory: {}", e))?;
    }

    Ok(())
}

/// 获取开始菜单快捷方式目录
fn get_start_menu_folder() -> std::path::PathBuf {
    let program_data = std::env::var("ProgramData")
        .unwrap_or_else(|_| r"C:\ProgramData".to_string());
    std::path::PathBuf::from(program_data)
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs")
        .join(meta::start_menu_folder())
}
