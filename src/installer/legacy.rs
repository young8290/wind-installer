use std::path::Path;

use crate::meta;

/// 清理旧版遗留文件和目录（新版已移除、旧版可能残留的条目）。
/// 在解压新文件之前调用，避免旧文件干扰。
pub fn cleanup_legacy(install_dir: &Path) {
    for name in meta::legacy_files() {
        let path = install_dir.join(name);
        if path.exists() {
            if let Err(e) = std::fs::remove_file(&path) {
                eprintln!("Warning: failed to remove legacy file {}: {}", name, e);
            }
        }
    }

    for rel in meta::legacy_dirs() {
        let path = install_dir.join(rel);
        if path.exists() {
            if let Err(e) = std::fs::remove_dir_all(&path) {
                eprintln!("Warning: failed to remove legacy dir {}: {}", rel, e);
            }
        }
    }
}
