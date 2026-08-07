use std::path::Path;

use crate::meta;
use crate::util::reboot;

use super::step::Reporter;

/// 清理旧版遗留文件和目录（新版已移除、旧版可能残留的条目）。
/// 在解压新文件之前调用，避免旧文件干扰。
///
/// 删不掉的一律排进重启删除队列并记入 [`reboot`] 账本——旧版遗留文件被占用是
/// 升级时最常见的「清不干净」来源（如上一版的 DLL 仍被 ctfmon 加载）。此前这里
/// 只 `eprintln!`，而安装器是无控制台的 GUI 进程，等于把事实彻底丢弃。
pub fn cleanup_legacy(install_dir: &Path, r: &mut dyn Reporter) {
    for name in meta::legacy_files() {
        let path = install_dir.join(name);
        if !path.exists() {
            continue;
        }
        if let Err(e) = std::fs::remove_file(&path) {
            r.warn(&format!(
                "旧版文件 {} 删除失败（将于重启后清理）: {}",
                name, e
            ));
            let _ = reboot::schedule_delete_on_reboot(&path);
        }
    }

    for rel in meta::legacy_dirs() {
        let path = install_dir.join(rel);
        if !path.exists() {
            continue;
        }
        if let Err(e) = std::fs::remove_dir_all(&path) {
            r.warn(&format!(
                "旧版目录 {} 删除失败（将于重启后清理）: {}",
                rel, e
            ));
            schedule_dir_on_reboot(&path);
        }
    }
}

/// 把一棵删不掉的目录树排进重启删除队列。
///
/// `MoveFileExW` 对**非空**目录无效，故必须自底向上逐项排队：先文件、再子目录、
/// 最后目录自身。只排目录一条的话，重启时它仍非空，删除会静默失败。
fn schedule_dir_on_reboot(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        // 连列目录都做不到（权限/句柄问题）：至少把目录本身记一笔，
        // 让「需要重启」的结论不会因为这里读不到而丢失。
        reboot::record_pending(dir, false);
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            schedule_dir_on_reboot(&path);
        } else if std::fs::remove_file(&path).is_err() {
            let _ = reboot::schedule_delete_on_reboot(&path);
        }
    }

    let _ = reboot::schedule_delete_on_reboot(dir);
}
