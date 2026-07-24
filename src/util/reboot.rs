//! 重启延迟文件操作 —— 处理「被占用、当前删不掉」的文件的通用兜底。
//!
//! Windows 允许对已打开的文件做同卷改名（只改目录项），却不允许删除被持锁进程占用的
//! 文件。标准做法是把删除**排进下次启动队列**：登记到
//! `HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\PendingFileRenameOperations`，
//! 由会话管理器（smss.exe）在开机极早期、文件尚未被任何进程加载前执行——那时删除必然成功。
//!
//! 这是 NSIS `Delete /REBOOTOK`、MSI `InstallFinalize` 背后的同一机制，属通用安装器能力，
//! 与具体应用无关。需要管理员权限（本安装/卸载器已通过 manifest 提权）。

use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows::core::PCWSTR;
use windows::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_DELAY_UNTIL_REBOOT};

/// 安排在下次系统启动时删除 `path`。
///
/// 用于锁定文件的兜底清理：当前删不掉就登记 `MoveFileExW(path, NULL, DELAY_UNTIL_REBOOT)`，
/// 下次开机由会话管理器删除。对空目录同样有效（可删除重启时已清空的残留目录）。
///
/// 调用方一般应据此提示用户「建议重启以完成清理」。失败（如权限不足、路径非法）返回 `Err`，
/// 由调用方决定是否降级为普通告警——本函数不 panic、不影响主流程。
pub fn schedule_delete_on_reboot(path: &Path) -> Result<(), String> {
    // 需以 NUL 结尾的宽字符串；空目标（第二参 NULL）表示「删除」而非「移动」。
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        MoveFileExW(
            PCWSTR(wide.as_ptr()),
            PCWSTR::null(),
            MOVEFILE_DELAY_UNTIL_REBOOT,
        )
    }
    .map_err(|e| format!("排定重启删除失败 {:?}: {}", path, e))
}
