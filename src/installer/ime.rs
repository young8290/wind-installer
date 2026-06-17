use std::path::Path;
use std::process::Command;

/// TSF Profile GUID
const PROFILE_GUID: &str = "{99C2EE31-5C57-45A2-9C63-FB54B34FD90A}";
/// TSF CLSID
const CLSID: &str = "{99C2EE30-5C57-45A2-9C63-FB54B34FD90A}";
/// 语言 ID (简体中文)
const LANG_ID: &str = "0804";

/// 注册 COM 组件
pub fn register_com(install_dir: &Path) -> Result<(), String> {
    // 注册 x64 DLL
    let dll_path = install_dir.join("wind_tsf.dll");
    if dll_path.exists() {
        let output = Command::new("regsvr32")
            .args(["/s", &dll_path.to_string_lossy()])
            .output()
            .map_err(|e| format!("Failed to run regsvr32: {}", e))?;

        if !output.status.success() {
            return Err(format!(
                "COM x64 registration failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }

    // 注册 x86 DLL（使用 SysWOW64 的 regsvr32）
    let dll_x86_path = install_dir.join("wind_tsf_x86.dll");
    if dll_x86_path.exists() {
        let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".to_string());
        let regsvr32_x86 = Path::new(&windir).join("SysWOW64").join("regsvr32.exe");

        let output = Command::new(&regsvr32_x86)
            .args(["/s", &dll_x86_path.to_string_lossy()])
            .output()
            .map_err(|e| format!("Failed to run regsvr32 x86: {}", e))?;

        if !output.status.success() {
            eprintln!("Warning: COM x86 registration failed");
        }
    }

    Ok(())
}

/// 反注册旧 COM 组件
pub fn unregister_old_com(install_dir: &Path) -> Result<(), String> {
    // 反注册 x64 DLL
    let dll_path = install_dir.join("wind_tsf.dll");
    if dll_path.exists() {
        let _ = Command::new("regsvr32")
            .args(["/u", "/s", &dll_path.to_string_lossy()])
            .output();
    }

    // 反注册 x86 DLL
    let dll_x86_path = install_dir.join("wind_tsf_x86.dll");
    if dll_x86_path.exists() {
        let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".to_string());
        let regsvr32_x86 = Path::new(&windir).join("SysWOW64").join("regsvr32.exe");
        let _ = Command::new(&regsvr32_x86)
            .args(["/u", "/s", &dll_x86_path.to_string_lossy()])
            .output();
    }

    Ok(())
}

/// 注册系统输入法（调用 InstallLayoutOrTip）
pub fn register_input_method() -> Result<(), String> {
    let profile_str = format!("{}:{}{}", LANG_ID, CLSID, PROFILE_GUID);

    // 调用 input.dll!InstallLayoutOrTip
    // 这需要通过 FFI 调用，这里使用 PowerShell 作为替代
    let ps_command = format!(
        r#"
        Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public class InputHelper {{
    [DllImport("input.dll", CharSet = CharSet.Unicode)]
    public static extern bool InstallLayoutOrTip(string profile, uint flags);
}}
"@
        [InputHelper]::InstallLayoutOrTip("{}", 0)
        "#,
        profile_str
    );

    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &ps_command])
        .output()
        .map_err(|e| format!("Failed to run PowerShell: {}", e))?;

    if !output.status.success() {
        eprintln!("Warning: InstallLayoutOrTip failed");
    }

    Ok(())
}

/// 反注册系统输入法
pub fn unregister_input_method() -> Result<(), String> {
    let profile_str = format!("{}:{}{}", LANG_ID, CLSID, PROFILE_GUID);

    let ps_command = format!(
        r#"
        Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public class InputHelper {{
    [DllImport("input.dll", CharSet = CharSet.Unicode)]
    public static extern bool InstallLayoutOrTip(string profile, uint flags);
}}
"@
        [InputHelper]::InstallLayoutOrTip("{}", 0x00000001)
        "#,
        profile_str
    );

    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &ps_command])
        .output()
        .map_err(|e| format!("Failed to run PowerShell: {}", e))?;

    if !output.status.success() {
        eprintln!("Warning: InstallLayoutOrTip uninstall failed");
    }

    Ok(())
}
