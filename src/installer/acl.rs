use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;

use crate::meta;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// ALL APPLICATION PACKAGES SID
const APP_PACKAGES_SID: &str = "*S-1-15-2-1";

/// 设置 DLL 权限（ALL APPLICATION PACKAGES 读取执行）
pub fn set_dll_permissions(install_dir: &Path) -> Result<(), String> {
    let dlls: Vec<_> = meta::acl_dlls()
        .iter()
        .map(|n| install_dir.join(n))
        .collect();

    for dll_path in &dlls {
        if dll_path.exists() {
            set_file_acl(dll_path, APP_PACKAGES_SID, "RX")?;
        }
    }

    Ok(())
}

/// 设置文件 ACL（无窗口）
fn set_file_acl(path: &Path, sid: &str, permission: &str) -> Result<(), String> {
    let path_str = path.to_string_lossy();
    let grant_arg = format!("{}:({})", sid, permission);

    let output = Command::new("icacls")
        .args([&*path_str, "/grant", &grant_arg, "/c"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("Failed to run icacls: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("icacls failed for {}: {}", path_str, stderr));
    }

    Ok(())
}
