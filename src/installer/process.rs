use std::mem;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;

use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
    TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 终止 WindInput 相关进程
pub fn terminate_windinput_processes() -> Result<(), String> {
    let names = ["wind_setting", "wind_portable", "wind_input"];
    let exe_names: Vec<&str> = names.iter().map(|n| &n[..]).collect();

    // 轮询最多 3 次；每轮用单次 PowerShell 调用杀掉所有存活进程
    for round in 0..3 {
        let alive: Vec<&str> = exe_names
            .iter()
            .copied()
            .filter(|n| !find_pids(&format!("{}.exe", n)).is_empty())
            .collect();

        if alive.is_empty() {
            return Ok(());
        }

        // 单次 PowerShell Stop-Process 覆盖所有仍存活的进程（Win11 上比 taskkill 更可靠）
        stop_process_all(&alive);

        // 最后一轮再兜底一次 Win32 TerminateProcess
        if round == 2 {
            for name in &alive {
                for pid in find_pids(&format!("{}.exe", name)) {
                    force_kill_pid(pid);
                }
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    // 仍存活：警告后继续，BackupIfLocked 兜底文件锁问题
    for name in &exe_names {
        if !find_pids(&format!("{}.exe", name)).is_empty() {
            eprintln!("Warning: {}.exe could not be terminated, relying on BackupIfLocked", name);
        }
    }

    Ok(())
}

/// 单次 PowerShell Stop-Process 杀掉多个进程名（只启动一次 PowerShell，开销小）
fn stop_process_all(names: &[&str]) {
    // 格式: 'wind_input','wind_setting'
    let name_list = names
        .iter()
        .map(|n| format!("'{}'", n))
        .collect::<Vec<_>>()
        .join(",");

    let cmd = format!(
        "Get-Process -Name {} -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue",
        name_list
    );

    let _ = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            &cmd,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output(); // 阻塞直到 PowerShell 退出
}

/// Win32 TerminateProcess 强制终止单个 PID（不等待退出）
fn force_kill_pid(pid: u32) {
    unsafe {
        if let Ok(h) = OpenProcess(PROCESS_TERMINATE, false, pid) {
            let _ = TerminateProcess(h, 1);
            let _ = CloseHandle(h);
        }
    }
}

/// 用 CreateToolhelp32Snapshot 枚举指定名称（已 lowercase）的所有 PID
fn find_pids(exe_lower: &str) -> Vec<u32> {
    let mut pids = Vec::new();
    unsafe {
        let snap = match CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
            Ok(h) => h,
            Err(_) => return pids,
        };
        let mut entry: PROCESSENTRY32W = mem::zeroed();
        entry.dwSize = mem::size_of::<PROCESSENTRY32W>() as u32;

        if Process32FirstW(snap, &mut entry).is_ok() {
            loop {
                let len = entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len());
                let name = String::from_utf16_lossy(&entry.szExeFile[..len]).to_lowercase();
                if name == exe_lower {
                    pids.push(entry.th32ProcessID);
                }
                if Process32NextW(snap, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snap);
    }
    pids
}

/// 预启动输入法服务（DETACHED_PROCESS 独立于安装器进程组）
pub fn prestart_service(install_dir: &Path) -> Result<(), String> {
    const DETACHED_PROCESS: u32 = 0x0000_0008;

    let exe_path = install_dir.join("wind_input.exe");
    if !exe_path.exists() {
        return Err("wind_input.exe not found".into());
    }

    Command::new(&exe_path)
        .creation_flags(DETACHED_PROCESS)
        .spawn()
        .map_err(|e| format!("Failed to start service: {}", e))?;

    Ok(())
}
