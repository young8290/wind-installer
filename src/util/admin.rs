use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

/// 检查当前进程是否以管理员（提升的令牌）运行。
/// 直接读取进程令牌的 TokenElevation，无任何子进程/窗口。
pub fn is_admin() -> bool {
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }

        let mut elevation = TOKEN_ELEVATION::default();
        let mut ret_len: u32 = 0;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut TOKEN_ELEVATION as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut ret_len,
        );
        let _ = CloseHandle(token);

        ok.is_ok() && elevation.TokenIsElevated != 0
    }
}

/// 用 ShellExecuteW "runas" 请求 UAC 提权并重新启动自身。
/// 不产生任何子进程窗口（标准 UAC 对话框由系统弹出，不是控制台）。
pub fn request_elevation() -> Result<(), String> {
    let current_exe =
        std::env::current_exe().map_err(|e| format!("Failed to get current exe: {}", e))?;

    // 转发原始命令行参数。不转发的话，提权重启后的实例拿不到 `--silent --dir ...`，
    // 应用内自动升级会静默退化成交互式向导（用户看到的是"升级时路径居然可以改"）。
    // 含空格的参数补引号；路径以反斜杠结尾的极端情况未处理（会转义掉结尾引号），
    // 但安装目录不会以反斜杠结尾，实际不构成问题。
    let params: String = std::env::args()
        .skip(1)
        .map(|a| {
            if a.contains(' ') && !a.starts_with('"') {
                format!("\"{a}\"")
            } else {
                a
            }
        })
        .collect::<Vec<_>>()
        .join(" ");

    let verb: Vec<u16> = "runas\0".encode_utf16().collect();
    let path: Vec<u16> = current_exe
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let params_w: Vec<u16> = params.encode_utf16().chain(std::iter::once(0)).collect();

    let result = unsafe {
        ShellExecuteW(
            None,
            windows::core::PCWSTR(verb.as_ptr()),
            windows::core::PCWSTR(path.as_ptr()),
            if params.is_empty() {
                windows::core::PCWSTR::null()
            } else {
                windows::core::PCWSTR(params_w.as_ptr())
            },
            None,
            SW_SHOWNORMAL,
        )
    };

    // ShellExecuteW 返回值 > 32 表示成功
    if result.0 as isize <= 32 {
        return Err(format!("ShellExecuteW runas failed: {}", result.0 as isize));
    }

    Ok(())
}
