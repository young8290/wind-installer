//! 文件释放辅助。释放主流程见 `steps::ExtractFiles`。

use std::path::Path;

/// 检查文件是否被锁定
pub fn is_file_locked(path: &Path) -> bool {
    if !path.exists() {
        return false;
    }

    // 尝试以写模式打开文件
    match std::fs::OpenOptions::new()
        .write(true)
        .open(path)
    {
        Ok(_) => false,
        Err(_) => true,
    }
}

/// 备份并替换锁定的文件
/// 如果文件被锁定，重命名为 .old_<random>，然后复制新文件
pub fn backup_if_locked(source: &Path, dest: &Path) -> Result<bool, String> {
    if !dest.exists() {
        // 目标不存在，直接复制
        std::fs::copy(source, dest)
            .map_err(|e| format!("Failed to copy file: {}", e))?;
        return Ok(false);
    }

    // 检查是否被锁定
    if !is_file_locked(dest) {
        // 未锁定，直接替换
        std::fs::copy(source, dest)
            .map_err(|e| format!("Failed to copy file: {}", e))?;
        return Ok(false);
    }

    // 文件被锁定，重命名为 .old_<random>
    let random_suffix = rand::random::<u32>();
    let file_name = dest.file_name()
        .ok_or_else(|| "Invalid file name".to_string())?
        .to_string_lossy();
    let old_name = format!("{}.old_{:08x}", file_name, random_suffix);
    let old_path = dest.parent()
        .ok_or_else(|| "Invalid parent directory".to_string())?
        .join(old_name);

    // 尝试重命名
    std::fs::rename(dest, &old_path)
        .map_err(|e| format!("Failed to rename locked file: {}", e))?;

    // 复制新文件
    std::fs::copy(source, dest)
        .map_err(|e| format!("Failed to copy new file: {}", e))?;

    Ok(true)
}

/// 清理 .old_* 和 .bak 备份文件
pub fn cleanup_backup_files(install_dir: &Path) -> Result<(), String> {
    let entries = std::fs::read_dir(install_dir)
        .map_err(|e| format!("Failed to read install directory: {}", e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
        let file_name = entry.file_name().to_string_lossy().to_string();

        if file_name.contains(".old_") || file_name.ends_with(".bak") {
            // 尝试删除
            if let Err(_) = std::fs::remove_file(entry.path()) {
                // 无法删除，计划重启后删除
                // 在 Windows 上，可以使用 MoveFileExW + MOVEFILE_DELAY_UNTIL_REBOOT
                eprintln!("Warning: Cannot delete backup file: {}", file_name);
            }
        }
    }

    Ok(())
}
