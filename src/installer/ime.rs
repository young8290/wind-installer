use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
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

/// 注册结果：**已注册成功的 DLL** + 失败信息。
///
/// 不用 `Result<(), String>` 是因为回执必须记下「已经做成的部分」：x64 成功、x86 失败时
/// 若整体返回 Err 而丢掉 x64 那条，卸载后 x64 的 CLSID 就永久滞留、且指向已删除的 DLL。
pub struct ComRegistration {
    /// `(DLL 绝对路径, 是否用 SysWOW64 的 regsvr32)`
    pub registered: Vec<(PathBuf, bool)>,
    pub errors: Vec<String>,
}

/// 注册 COM 组件（regsvr32，无窗口）。逐个 DLL 尝试，返回实际成功的那些。
pub fn register_com(install_dir: &Path) -> ComRegistration {
    let mut out = ComRegistration {
        registered: Vec::new(),
        errors: Vec::new(),
    };

    let Some(ime) = ime_cfg() else {
        return out;
    };

    let x64 = install_dir.join(&ime.dll_x64);
    if x64.exists() {
        match run_regsvr32(&x64, false, false) {
            Ok(()) => out.registered.push((x64, false)),
            Err(e) => out.errors.push(e),
        }
    }

    if !ime.dll_x86.is_empty() {
        let x86 = install_dir.join(&ime.dll_x86);
        if x86.exists() {
            match run_regsvr32(&x86, true, false) {
                Ok(()) => out.registered.push((x86, true)),
                Err(e) => out.errors.push(e),
            }
        }
    }

    out
}

/// 调用 regsvr32 注册/反注册单个 DLL，检查退出码。
fn run_regsvr32(dll: &Path, wow64: bool, unregister: bool) -> Result<(), String> {
    let program = if wow64 {
        let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".to_string());
        Path::new(&windir).join("SysWOW64").join("regsvr32.exe")
    } else {
        PathBuf::from("regsvr32")
    };

    let mut args: Vec<&str> = Vec::new();
    if unregister {
        args.push("/u");
    }
    args.push("/s");
    let dll_str = dll.to_string_lossy();
    args.push(&dll_str);

    let output = Command::new(&program)
        .args(&args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("无法运行 {:?}: {}", program, e))?;

    if !output.status.success() {
        return Err(format!(
            "regsvr32{} 失败 {:?}: {}",
            if unregister { " /u" } else { "" },
            dll,
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(())
}

/// 反注册旧 COM 组件（无窗口）
pub fn unregister_old_com(install_dir: &Path) -> Result<(), String> {
    let Some(ime) = ime_cfg() else {
        return Ok(());
    };

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

// ── 回执支持：记录产物 / 按产物撤销（不查清单）──────────────────────────────

/// 当前清单的 TSF profile 字符串，供安装步骤写入回执。
pub fn profile_id() -> Option<String> {
    ime_cfg().map(profile_string)
}

/// 按绝对路径反注册 COM DLL。回执驱动——不读清单，故升级换了 DLL 名也能撤销旧的。
///
/// 检查 regsvr32 退出码：反注册失败必须让 `UndoReceipt` 汇报，否则用户以为卸载干净了，
/// 实际 COM 仍注册着。
pub fn unregister_com_path(dll: &Path, wow64: bool) -> Result<(), String> {
    if !dll.exists() {
        return Ok(());
    }
    run_regsvr32(dll, wow64, true)
}

/// 按 profile 字符串反注册输入法。回执驱动——不读清单。
pub fn unregister_profile(profile: &str) -> Result<(), String> {
    call_install_layout_or_tip(profile, 0x0000_0001)
}

/// 注册系统输入法 — 直接调用 input.dll!InstallLayoutOrTip，无 PowerShell 窗口
pub fn register_input_method() -> Result<(), String> {
    let Some(ime) = ime_cfg() else {
        return Ok(());
    };
    call_install_layout_or_tip(&profile_string(ime), 0)
}

// 反注册输入法请用 `unregister_profile`：它按回执记录的 profile 字符串撤销，
// 不依赖当前清单。

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
                    eprintln!(
                        "Warning: InstallLayoutOrTip returned false for flags={:#010x}",
                        flags
                    );
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
