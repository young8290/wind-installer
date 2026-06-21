use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;

use windows::core::PCSTR;
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

use crate::manifest::ImeInfo;
use crate::meta;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 取清单中的 ime 段；无则视为无输入法（各函数空操作）。
fn ime_cfg() -> Option<&'static ImeInfo> {
    meta::manifest().ime.as_ref()
}

/// 注册 COM 组件（regsvr32，无窗口）
pub fn register_com(install_dir: &Path) -> Result<(), String> {
    let Some(ime) = ime_cfg() else { return Ok(()); };

    let dll_path = install_dir.join(&ime.dll_x64);
    if dll_path.exists() {
        let output = Command::new("regsvr32")
            .args(["/s", &dll_path.to_string_lossy()])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|e| format!("Failed to run regsvr32: {}", e))?;

        if !output.status.success() {
            return Err(format!(
                "COM x64 registration failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }

    if !ime.dll_x86.is_empty() {
        let dll_x86_path = install_dir.join(&ime.dll_x86);
        if dll_x86_path.exists() {
            let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".to_string());
            let regsvr32_x86 = Path::new(&windir).join("SysWOW64").join("regsvr32.exe");

            let output = Command::new(&regsvr32_x86)
                .args(["/s", &dll_x86_path.to_string_lossy()])
                .creation_flags(CREATE_NO_WINDOW)
                .output()
                .map_err(|e| format!("Failed to run regsvr32 x86: {}", e))?;

            if !output.status.success() {
                eprintln!("Warning: COM x86 registration failed");
            }
        }
    }

    Ok(())
}

/// 反注册旧 COM 组件（无窗口）
pub fn unregister_old_com(install_dir: &Path) -> Result<(), String> {
    let Some(ime) = ime_cfg() else { return Ok(()); };

    let dll_path = install_dir.join(&ime.dll_x64);
    if dll_path.exists() {
        let _ = Command::new("regsvr32")
            .args(["/u", "/s", &dll_path.to_string_lossy()])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
    }

    if !ime.dll_x86.is_empty() {
        let dll_x86_path = install_dir.join(&ime.dll_x86);
        if dll_x86_path.exists() {
            let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".to_string());
            let regsvr32_x86 = Path::new(&windir).join("SysWOW64").join("regsvr32.exe");
            let _ = Command::new(&regsvr32_x86)
                .args(["/u", "/s", &dll_x86_path.to_string_lossy()])
                .creation_flags(CREATE_NO_WINDOW)
                .output();
        }
    }

    Ok(())
}

/// TSF profile 字符串：`<lang_id>:<clsid><profile_guid>`
fn profile_string(ime: &ImeInfo) -> String {
    format!("{}:{}{}", ime.lang_id, ime.clsid, ime.profile_guid)
}

/// 注册系统输入法 — 直接调用 input.dll!InstallLayoutOrTip，无 PowerShell 窗口
pub fn register_input_method() -> Result<(), String> {
    let Some(ime) = ime_cfg() else { return Ok(()); };
    call_install_layout_or_tip(&profile_string(ime), 0)
}

/// 反注册系统输入法
pub fn unregister_input_method() -> Result<(), String> {
    let Some(ime) = ime_cfg() else { return Ok(()); };
    call_install_layout_or_tip(&profile_string(ime), 0x0000_0001)
}

/// 直接通过 input.dll FFI 调用 InstallLayoutOrTip。
/// 等同于 NSIS `System::Call 'input::InstallLayoutOrTip(t, i)'`，无任何子进程/窗口开销。
fn call_install_layout_or_tip(profile: &str, flags: u32) -> Result<(), String> {
    type Fn = unsafe extern "system" fn(*const u16, u32) -> bool;

    unsafe {
        let lib_name: Vec<u16> = "input.dll\0".encode_utf16().collect();
        let hmod = LoadLibraryW(windows::core::PCWSTR(lib_name.as_ptr()))
            .map_err(|e| format!("Failed to load input.dll: {}", e))?;

        let proc = GetProcAddress(hmod, PCSTR(b"InstallLayoutOrTip\0".as_ptr()));
        match proc {
            Some(f) => {
                let f: Fn = std::mem::transmute(f);
                let wide: Vec<u16> = profile.encode_utf16().chain(std::iter::once(0)).collect();
                if !f(wide.as_ptr(), flags) {
                    eprintln!("Warning: InstallLayoutOrTip returned false for flags={:#010x}", flags);
                }
            }
            None => {
                return Err("InstallLayoutOrTip not found in input.dll".into());
            }
        }
        // input.dll 是系统 IME 核心库，保持加载到进程退出即可
    }
    Ok(())
}
