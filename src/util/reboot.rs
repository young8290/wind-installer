//! 重启延迟文件操作 —— 处理「被占用、当前删不掉」的文件的通用兜底。
//!
//! Windows 允许对已打开的文件做同卷改名（只改目录项），却不允许删除被持锁进程占用的
//! 文件。标准做法是把删除**排进下次启动队列**：登记到
//! `HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\PendingFileRenameOperations`，
//! 由会话管理器（smss.exe）在开机极早期、文件尚未被任何进程加载前执行——那时删除必然成功。
//!
//! 这是 NSIS `Delete /REBOOTOK`、MSI `InstallFinalize` 背后的同一机制，属通用安装器能力，
//! 与具体应用无关。需要管理员权限（本安装/卸载器已通过 manifest 提权）。
//!
//! # 为什么有一本进程级账本
//!
//! 「有旧文件删不掉」这个事实产生在最底层的 IO 处（解压时的 `create_or_backup`、
//! 旧版遗留清理、卸载时的文件删除），那里既没有 `Step` 上下文也没有 `Reporter`；
//! 而需要它的是最顶层——完成页要提示用户重启、quiet 模式要据此**放弃自动退出**。
//! 逐层改签名会把 `need_reboot` 穿进 `extract_entry`/`extract_all` 等与之无关的 API。
//!
//! `MoveFileExW(DELAY_UNTIL_REBOOT)` 本身就是写进程外全局状态的操作，故这里用
//! [`LEDGER`] 记账只是把它已有的全局性显式化，[`run_plan`] 在收尾处一次性读取汇总。
//!
//! [`run_plan`]: crate::installer::step::run_plan

use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use windows::core::PCWSTR;
use windows::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_DELAY_UNTIL_REBOOT};

/// 一条「当前删不掉、留待重启处理」的记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingItem {
    /// 被推迟的路径（升级场景下通常是改名后的 `.old_xxxxxxxx`）。
    pub path: PathBuf,
    /// 是否成功登记到 `PendingFileRenameOperations`。
    ///
    /// `false` 表示连排队都失败了——重启也不会自动清理，属于更严重的残留，
    /// 但对用户的建议是一样的（重启后进程不再持锁，可手工删或由下次安装清掉）。
    pub scheduled: bool,
}

fn ledger() -> &'static Mutex<Vec<PendingItem>> {
    static LEDGER: OnceLock<Mutex<Vec<PendingItem>>> = OnceLock::new();
    LEDGER.get_or_init(|| Mutex::new(Vec::new()))
}

/// 取账本；忽略中毒（记账线程 panic 不该让后续记账全部失败）。
fn ledger_lock() -> std::sync::MutexGuard<'static, Vec<PendingItem>> {
    ledger().lock().unwrap_or_else(|e| e.into_inner())
}

/// 安排在下次系统启动时删除 `path`，并记入本次运行的账本。
///
/// 用于锁定文件的兜底清理：当前删不掉就登记 `MoveFileExW(path, NULL, DELAY_UNTIL_REBOOT)`，
/// 下次开机由会话管理器删除。对空目录同样有效（可删除重启时已清空的残留目录）。
///
/// 无论成功与否都会记账：调用方只需 best-effort 调用，「是否该提示用户重启」由
/// [`is_reboot_pending`] 统一回答。失败（如权限不足、路径非法）返回 `Err`，
/// 由调用方决定是否降级为普通告警——本函数不 panic、不影响主流程。
pub fn schedule_delete_on_reboot(path: &Path) -> Result<(), String> {
    // 需以 NUL 结尾的宽字符串；空目标（第二参 NULL）表示「删除」而非「移动」。
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let result = unsafe {
        MoveFileExW(
            PCWSTR(wide.as_ptr()),
            PCWSTR::null(),
            MOVEFILE_DELAY_UNTIL_REBOOT,
        )
    }
    .map_err(|e| format!("排定重启删除失败 {:?}: {}", path, e));

    record_pending(path, result.is_ok());
    result
}

/// 直接记一笔「当前处理不掉的残留」而不尝试排队。
///
/// 用于连改名让路都做不到、或路径本就不该进 `PendingFileRenameOperations` 的场景：
/// 事实仍需被计入「建议重启」的依据。
pub fn record_pending(path: &Path, scheduled: bool) {
    let item = PendingItem {
        path: path.to_path_buf(),
        scheduled,
    };
    let mut guard = ledger_lock();
    // 同一路径可能被多次尝试（改名失败后重试）；只保留一条，成功状态取「曾经成功过」。
    if let Some(existing) = guard.iter_mut().find(|i| i.path == item.path) {
        existing.scheduled |= scheduled;
        return;
    }
    guard.push(item);
}

/// 本次运行是否留下了需要重启才能清理干净的东西。
pub fn is_reboot_pending() -> bool {
    !ledger_lock().is_empty()
}

/// 账本快照，供日志与诊断使用。
pub fn pending_items() -> Vec<PendingItem> {
    ledger_lock().clone()
}

/// 一行人类可读摘要，如 `3 个文件待重启后清理（其中 1 个未能排队）`。
/// 账本为空时返回空串。
pub fn pending_summary() -> String {
    let items = pending_items();
    if items.is_empty() {
        return String::new();
    }
    let unscheduled = items.iter().filter(|i| !i.scheduled).count();
    if unscheduled == 0 {
        format!("{} 个文件待重启后清理", items.len())
    } else {
        format!(
            "{} 个文件待重启后清理（其中 {} 个未能排入删除队列）",
            items.len(),
            unscheduled
        )
    }
}

/// 清空账本。**仅供测试**——一次真实运行内不存在「重新开始计数」的正当场景。
///
/// 之所以是 `pub` 而非 `#[cfg(test)]`：本 crate 的 `--lib` 单测二进制名含 "install"，
/// 会命中 Windows UAC 安装器检测启发式而无法在普通权限下启动（见 `ui::theme` 末注），
/// 账本的测试只能放在 `tests/` 集成测试里，那里链接的是非 test 编译的 lib。
#[doc(hidden)]
#[allow(dead_code)] // 只被 tests/ 引用；bin target 看不到那边的用法
pub fn reset_ledger() {
    ledger_lock().clear();
}
