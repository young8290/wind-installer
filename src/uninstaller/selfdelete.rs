use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::meta;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 检查是否在自删除模式下运行
pub fn is_self_delete_mode() -> bool {
    std::env::args().any(|a| a == "--self-delete")
}

/// 获取自删除目标目录（`--self-delete <dir>` 后面的参数）
pub fn self_delete_target() -> Option<PathBuf> {
    let args: Vec<String> = std::env::args().collect();
    let pos = args.iter().position(|a| a == "--self-delete")?;
    args.get(pos + 1).map(PathBuf::from)
}

/// 卸载完成后调用：将自身复制到 %TEMP%，以 `--self-delete <install_dir>` 启动副本，
/// 然后立即退出当前进程（不返回）。副本负责删除安装目录并自我清除。
pub fn trigger_self_delete(install_dir: &Path) -> Result<(), String> {
    let current_exe = std::env::current_exe()
        .map_err(|e| format!("获取自身路径失败: {}", e))?;

    let temp_dir = std::env::temp_dir();
    let suffix = std::process::id().wrapping_mul(2654435761);
    let temp_exe = temp_dir.join(format!("{}_uninst_{:08x}.exe", meta::app_id().to_lowercase(), suffix));

    std::fs::copy(&current_exe, &temp_exe)
        .map_err(|e| format!("复制到临时目录失败: {}", e))?;

    Command::new(&temp_exe)
        .arg("--self-delete")
        .arg(install_dir)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("启动清理进程失败: {}", e))?;

    std::process::exit(0);
}

/// 自删除模式执行体（在 %TEMP% 副本中运行）：
/// 等待原进程退出 → 删除安装目录 → 用 cmd 延时删除自身。
pub fn execute_self_delete(install_dir: &Path) {
    // 等待原 uninstall.exe 完全退出（它已调用 exit(0)）
    std::thread::sleep(std::time::Duration::from_millis(800));

    // 安装目录（含原 uninstall.exe）此时不再被占用，可安全删除
    let _ = std::fs::remove_dir_all(install_dir);

    // cmd /c ping 作延时计时器，等当前进程退出后再 del 自身（无窗口）
    if let Ok(self_exe) = std::env::current_exe() {
        let cmd = format!(
            "ping 127.0.0.1 -n 3 >nul & del /f /q \"{}\"",
            self_exe.display()
        );
        let _ = Command::new("cmd")
            .args(["/c", &cmd])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
    }
}
