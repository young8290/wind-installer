use std::process::Command;

/// 检查是否以管理员权限运行
pub fn is_admin() -> bool {
    // 尝试打开一个需要管理员权限的注册表键
    let output = Command::new("net")
        .args(["session"])
        .output();

    match output {
        Ok(out) => out.status.success(),
        Err(_) => false,
    }
}

/// 请求管理员权限重新启动程序
pub fn request_elevation() -> Result<(), String> {
    let current_exe = std::env::current_exe()
        .map_err(|e| format!("Failed to get current exe: {}", e))?;

    // 使用 PowerShell 的 Start-Process -Verb RunAs
    let output = Command::new("powershell")
        .args([
            "-Command",
            &format!(
                "Start-Process -FilePath '{}' -Verb RunAs",
                current_exe.to_string_lossy()
            ),
        ])
        .output()
        .map_err(|e| format!("Failed to request elevation: {}", e))?;

    if !output.status.success() {
        return Err("Failed to request elevation".into());
    }

    // 退出当前进程
    std::process::exit(0);
}
