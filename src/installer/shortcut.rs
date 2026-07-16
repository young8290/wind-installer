use std::path::{Path, PathBuf};

use crate::manifest::{ShortcutInfo, ShortcutLocation};
use crate::meta;

/// 创建清单 [[shortcut]] 段声明的快捷方式。目标不存在的条目跳过（不报错）。
pub fn create_shortcuts(install_dir: &Path, items: &[ShortcutInfo]) -> Result<(), String> {
    for item in items {
        let target = install_dir.join(&item.target);
        if !target.exists() {
            continue;
        }

        let dir = location_dir(item.location);
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("无法创建快捷方式目录 {:?}: {}", dir, e))?;

        let link_path = dir.join(format!("{}.lnk", item.effective_name()));
        create_shortcut(
            &target,
            &link_path,
            &install_dir.to_string_lossy(),
            item.effective_description(),
        )?;
    }
    Ok(())
}

/// 删除清单声明的快捷方式：开始菜单整个文件夹 + 逐个桌面快捷方式。
///
/// 开始菜单删除失败不提前返回——否则会跳过桌面清理，在桌面留下孤儿 .lnk。
pub fn delete_shortcuts(items: &[ShortcutInfo]) -> Result<(), String> {
    let mut errors = Vec::new();

    let start_menu = start_menu_dir();
    if start_menu.exists() {
        if let Err(e) = std::fs::remove_dir_all(&start_menu) {
            errors.push(format!("无法删除开始菜单目录: {}", e));
        }
    }

    // 桌面快捷方式散落在公共桌面，只能按名字逐个删
    for item in items
        .iter()
        .filter(|i| i.location == ShortcutLocation::Desktop)
    {
        let link = desktop_dir().join(format!("{}.lnk", item.effective_name()));
        if link.exists() {
            if let Err(e) = std::fs::remove_file(&link) {
                errors.push(format!("无法删除桌面快捷方式 {:?}: {}", link, e));
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

fn create_shortcut(
    target: &Path,
    link_path: &Path,
    working_dir: &str,
    description: &str,
) -> Result<(), String> {
    let mut link = mslnk::ShellLink::new(target)
        .map_err(|e| format!("无法创建快捷方式对象: {}", e))?;

    link.set_working_dir(Some(working_dir.to_string()));
    link.set_name(Some(description.to_string()));

    link.create_lnk(link_path)
        .map_err(|e| format!("无法保存快捷方式 {:?}: {}", link_path, e))?;

    Ok(())
}

fn location_dir(loc: ShortcutLocation) -> PathBuf {
    match loc {
        ShortcutLocation::StartMenu => start_menu_dir(),
        ShortcutLocation::Desktop => desktop_dir(),
    }
}

/// 全局开始菜单下的应用文件夹。
fn start_menu_dir() -> PathBuf {
    let program_data =
        std::env::var("ProgramData").unwrap_or_else(|_| r"C:\ProgramData".to_string());
    PathBuf::from(program_data)
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs")
        .join(meta::start_menu_folder())
}

/// 公共桌面（对所有用户可见）。
fn desktop_dir() -> PathBuf {
    let public = std::env::var("PUBLIC").unwrap_or_else(|_| r"C:\Users\Public".to_string());
    PathBuf::from(public).join("Desktop")
}
