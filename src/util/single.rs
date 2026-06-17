use std::sync::atomic::{AtomicBool, Ordering};

/// 全局单实例标志
static INSTANCE_RUNNING: AtomicBool = AtomicBool::new(false);

/// 检查是否已有实例在运行
pub fn is_another_instance_running() -> bool {
    // 简单的基于文件锁的实现
    let lock_file = get_lock_file_path();

    if lock_file.exists() {
        // 检查锁文件中的进程是否仍在运行
        if let Ok(content) = std::fs::read_to_string(&lock_file) {
            if let Ok(pid) = content.trim().parse::<u32>() {
                if is_process_alive(pid) {
                    return true;
                }
            }
        }
    }

    // 写入当前进程 ID
    let current_pid = std::process::id();
    let _ = std::fs::write(&lock_file, current_pid.to_string());
    INSTANCE_RUNNING.store(true, Ordering::SeqCst);

    false
}

/// 释放单实例锁
pub fn release_lock() {
    if INSTANCE_RUNNING.load(Ordering::SeqCst) {
        let lock_file = get_lock_file_path();
        let _ = std::fs::remove_file(lock_file);
        INSTANCE_RUNNING.store(false, Ordering::SeqCst);
    }
}

/// 获取锁文件路径
fn get_lock_file_path() -> std::path::PathBuf {
    let temp_dir = std::env::temp_dir();
    temp_dir.join("wind_installer.lock")
}

/// 检查进程是否存活
fn is_process_alive(pid: u32) -> bool {
    use std::process::Command;

    let output = Command::new("tasklist")
        .args(["/FI", &format!("PID eq {}", pid), "/NH"])
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            stdout.contains(&pid.to_string())
        }
        Err(_) => false,
    }
}

/// 程序退出时清理
#[allow(dead_code)]
pub fn setup_cleanup_on_exit() {
    ctrlc::set_handler(move || {
        release_lock();
        std::process::exit(0);
    })
    .ok();
}
