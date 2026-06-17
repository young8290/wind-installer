use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Threading::{
    GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};

/// 全局单实例标志
static INSTANCE_RUNNING: AtomicBool = AtomicBool::new(false);

/// 检查是否已有实例在运行（基于 %TEMP% 锁文件 + PID 校验）
pub fn is_another_instance_running() -> bool {
    let lock_file = get_lock_file_path();

    if lock_file.exists() {
        if let Ok(content) = std::fs::read_to_string(&lock_file) {
            if let Ok(pid) = content.trim().parse::<u32>() {
                if is_process_alive(pid) {
                    return true;
                }
            }
        }
    }

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

fn get_lock_file_path() -> std::path::PathBuf {
    std::env::temp_dir().join("wind_installer.lock")
}

/// 通过 OpenProcess + GetExitCodeProcess 判断 PID 是否仍在运行，无子进程/窗口。
fn is_process_alive(pid: u32) -> bool {
    unsafe {
        match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(handle) => {
                let mut exit_code: u32 = 0;
                let _ = GetExitCodeProcess(handle, &mut exit_code);
                let _ = CloseHandle(handle);
                exit_code == 259 // STILL_ACTIVE
            }
            Err(_) => false,
        }
    }
}

/// 程序退出时清理（Ctrl-C）
#[allow(dead_code)]
pub fn setup_cleanup_on_exit() {
    ctrlc::set_handler(move || {
        release_lock();
        std::process::exit(0);
    })
    .ok();
}
