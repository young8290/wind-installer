use std::path::PathBuf;
use std::process::Command;

/// 自删除策略实现
///
/// 卸载器运行时：
/// 1. 复制自身到 %TEMP% 目录
/// 2. 启动新进程（带 /SELF_DELETE 标志）
/// 3. 退出当前进程
/// 4. 新进程执行清理后删除原卸载器

/// 检查是否在自删除模式下运行
pub fn is_self_delete_mode() -> bool {
    let args: Vec<String> = std::env::args().collect();
    args.iter().any(|a| a == "/SELF_DELETE")
}

/// 复制自身到临时目录并启动新进程
#[allow(dead_code)]
pub fn copy_and_restart() -> Result<(), String> {
    let current_exe = std::env::current_exe()
        .map_err(|e| format!("Failed to get current exe: {}", e))?;

    let temp_dir = std::env::temp_dir();
    let random_suffix = rand::random::<u32>();
    let temp_exe_name = format!("wind_uninstall_{:08x}.exe", random_suffix);
    let temp_exe = temp_dir.join(&temp_exe_name);

    // 复制自身到临时目录
    std::fs::copy(&current_exe, &temp_exe)
        .map_err(|e| format!("Failed to copy to temp: {}", e))?;

    // 启动新进程
    Command::new(&temp_exe)
        .arg("/SELF_DELETE")
        .arg(&current_exe)
        .spawn()
        .map_err(|e| format!("Failed to start new process: {}", e))?;

    // 退出当前进程
    std::process::exit(0);
}

/// 在自删除模式下执行清理
pub fn execute_self_delete(original_exe: &PathBuf) -> Result<(), String> {
    // 等待原进程退出
    std::thread::sleep(std::time::Duration::from_millis(500));

    // 删除原卸载器
    if original_exe.exists() {
        // 多次尝试删除
        for _ in 0..10 {
            if std::fs::remove_file(original_exe).is_ok() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
    }

    // 删除自身
    let current_exe = std::env::current_exe()
        .map_err(|e| format!("Failed to get current exe: {}", e))?;

    // 使用 cmd /c ping 延迟删除（Windows 上删除正在运行的 exe 需要技巧）
    let _ = Command::new("cmd")
        .args([
            "/c",
            "ping",
            "127.0.0.1",
            "-n",
            "2",
            ">nul",
            "&",
            "del",
            &current_exe.to_string_lossy(),
        ])
        .spawn();

    Ok(())
}
