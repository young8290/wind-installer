use std::process::Command;

/// 终止 WindInput 相关进程
pub fn terminate_windinput_processes() -> Result<(), String> {
    let processes = vec![
        "wind_setting",
        "wind_portable",
        "wind_input",
    ];

    for process_name in &processes {
        robust_kill(process_name)?;
    }

    Ok(())
}

/// 健壮的杀进程方法
/// 1. 尝试 taskkill /F（异步）
/// 2. 如果失败，使用 PowerShell Stop-Process
/// 3. 如果仍然失败，返回错误（由 REBOOTOK 兜底）
fn robust_kill(process_name: &str) -> Result<(), String> {
    let exe_name = format!("{}.exe", process_name);

    // 检查进程是否存在
    if !is_process_running(&exe_name) {
        return Ok(());
    }

    // 阶段 1: taskkill 异步重试
    for _attempt in 0..2 {
        let _ = Command::new("cmd")
            .args(["/c", "start", "/b", "", "taskkill", "/F", "/IM", &exe_name])
            .output();

        // 轮询等待进程退出
        for _ in 0..15 {
            std::thread::sleep(std::time::Duration::from_millis(200));
            if !is_process_running(&exe_name) {
                return Ok(());
            }
        }
    }

    // 阶段 2: PowerShell Stop-Process
    let _ = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &format!(
                "Get-Process -Name {} -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue",
                process_name
            ),
        ])
        .output();

    // 等待进程退出
    for _ in 0..30 {
        std::thread::sleep(std::time::Duration::from_millis(200));
        if !is_process_running(&exe_name) {
            return Ok(());
        }
    }

    // 进程仍然存在，返回警告
    eprintln!("Warning: Process {} could not be terminated", exe_name);
    Ok(())
}

/// 检查进程是否正在运行
fn is_process_running(exe_name: &str) -> bool {
    let output = Command::new("tasklist")
        .args(["/FI", &format!("IMAGENAME eq {}", exe_name), "/NH"])
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            stdout.to_lowercase().contains(&exe_name.to_lowercase())
        }
        Err(_) => false,
    }
}

/// 预启动输入法服务
pub fn prestart_service(install_dir: &Path) -> Result<(), String> {
    let exe_path = install_dir.join("wind_input.exe");
    if !exe_path.exists() {
        return Err("wind_input.exe not found".into());
    }

    Command::new(&exe_path)
        .spawn()
        .map_err(|e| format!("Failed to start service: {}", e))?;

    Ok(())
}

use std::path::Path;
