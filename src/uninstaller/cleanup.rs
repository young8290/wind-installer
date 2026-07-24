use std::path::{Path, PathBuf};

use crate::meta;
use crate::util::reboot;

/// 清理选项
#[derive(Debug, Clone)]
pub struct CleanupOptions {
    /// 安装目录
    pub install_dir: PathBuf,
    /// 是否清除用户配置数据（%APPDATA%\AppID）
    pub clean_roaming: bool,
    /// 是否清除本地缓存（%LOCALAPPDATA%\AppID\cache）
    pub clean_local_cache: bool,
    /// 清除用户配置数据前是否先备份到桌面（仅在 clean_roaming 时生效）
    pub backup_to_desktop: bool,
    /// 静默模式下的保留用户数据标志
    pub keep_user_data: bool,
}

impl Default for CleanupOptions {
    fn default() -> Self {
        let program_files = std::env::var("ProgramFiles")
            .unwrap_or_else(|_| r"C:\Program Files".to_string());

        Self {
            install_dir: PathBuf::from(program_files).join(meta::app_id()),
            clean_roaming: false,
            clean_local_cache: true,
            backup_to_desktop: true,
            keep_user_data: false,
        }
    }
}

impl CleanupOptions {
    /// 解析用户数据目录：优先读清单 `[datadir]` 声明的配置文件，其次用默认位置。
    ///
    /// **必须在 `UndoReceipt` 之前调用**——撤销会把这个配置文件删掉。调用方应在
    /// 卸载开始时解析一次并带着结果走（见 `steps::UninstallCtx::new`）。
    pub fn user_data_dir(&self) -> PathBuf {
        let local_app_data = std::env::var("LOCALAPPDATA")
            .unwrap_or_else(|_| {
                let mut p = PathBuf::from(std::env::var("USERPROFILE").unwrap_or_default());
                p.push("AppData\\Local");
                p.to_string_lossy().to_string()
            });

        // 清单未声明 [datadir] 段则没有配置文件可读，直接用默认位置
        if let Some(datadir) = meta::manifest().datadir.as_ref() {
            let conf = PathBuf::from(&local_app_data)
                .join(meta::app_id())
                .join(&datadir.conf_file);

            if let Ok(content) = std::fs::read_to_string(&conf) {
                let content = content.trim();
                if !content.is_empty() {
                    return PathBuf::from(content);
                }
            }
        }

        // 默认位置
        let app_data = std::env::var("APPDATA")
            .unwrap_or_else(|_| {
                let mut p = PathBuf::from(std::env::var("USERPROFILE").unwrap_or_default());
                p.push("AppData\\Roaming");
                p.to_string_lossy().to_string()
            });
        PathBuf::from(app_data).join(meta::app_id())
    }

    /// 获取本地缓存目录
    pub fn local_cache_dir(&self) -> PathBuf {
        let local_app_data = std::env::var("LOCALAPPDATA")
            .unwrap_or_else(|_| {
                let mut p = PathBuf::from(std::env::var("USERPROFILE").unwrap_or_default());
                p.push("AppData\\Local");
                p.to_string_lossy().to_string()
            });
        PathBuf::from(local_app_data).join(meta::app_id()).join("cache")
    }
}

/// 删除安装文件
pub fn delete_install_files(install_dir: &PathBuf) -> Result<(), String> {
    // 从 meta 动态构建二进制文件列表：进程名→.exe + ACL DLL 列表
    let mut binaries: Vec<String> = meta::process_names()
        .iter()
        .map(|n| format!("{}.exe", n))
        .collect();
    for dll in meta::acl_dlls() {
        if !binaries.iter().any(|b| b == dll) {
            binaries.push(dll.to_string());
        }
    }

    for binary in &binaries {
        let binary = binary.as_str();
        let path = install_dir.join(binary);
        if path.exists() && std::fs::remove_file(&path).is_err() {
            // 被锁定删不掉：改名让路 + 排重启删。改名成功就删改名后的，失败就直接排原路径。
            let random_suffix = rand::random::<u32>();
            let old_name = format!("{}.old_{:08x}", binary, random_suffix);
            let old_path = install_dir.join(&old_name);
            let target = if std::fs::rename(&path, &old_path).is_ok() {
                old_path
            } else {
                path
            };
            let _ = reboot::schedule_delete_on_reboot(&target);
        }
    }

    // 删除数据目录
    let data_dir = install_dir.join("data");
    if data_dir.exists() {
        if let Err(_) = std::fs::remove_dir_all(&data_dir) {
            eprintln!("Warning: Could not delete data directory");
        }
    }

    // 删除卸载程序
    let uninstall_exe = install_dir.join("uninstall.exe");
    if uninstall_exe.exists() {
        let _ = std::fs::remove_file(&uninstall_exe);
    }

    // 清理备份文件（含本次改名让路产生的 .old_）：仍锁定删不掉的排重启删。
    if let Ok(entries) = std::fs::read_dir(install_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !(name.contains(".old_") || name.ends_with(".bak")) {
                continue;
            }
            let p = entry.path();
            if std::fs::remove_file(&p).is_err() {
                let _ = reboot::schedule_delete_on_reboot(&p);
            }
        }
    }

    // 尝试删除安装目录：删不掉（尚有锁定文件）就排重启删——上面各文件已单独排队，
    // 目录会在它们清空后于重启时删除。
    if std::fs::remove_dir_all(install_dir).is_err()
        && reboot::schedule_delete_on_reboot(install_dir).is_err()
    {
        eprintln!("Warning: Could not delete install directory");
    }

    Ok(())
}

// 注册表清理已由 `steps::UndoReceipt` 按安装回执反向回放接管——它撤销的是安装时
// **实际写成**的键，而非按当前清单猜测的键，故升级后仍能清掉上一版留下的东西。

/// 清理用户数据。
///
/// `user_data_dir` 由调用方在卸载开始时解析并传入，而非在此现算——`UndoReceipt`
/// 会删掉数据目录配置文件，现算就只能拿到默认位置，用户自定义的数据目录会被漏掉。
pub fn cleanup_user_data(options: &CleanupOptions, user_data_dir: &Path) -> Result<(), String> {
    if options.keep_user_data {
        return Ok(());
    }

    let local_cache_dir = options.local_cache_dir();

    // 清除用户配置（删除前可选备份到桌面）
    if options.clean_roaming && user_data_dir.exists() {
        if options.backup_to_desktop {
            // 目录名带本地时间戳，每次卸载生成唯一目录，避免覆盖历史备份
            let backup_dir = desktop_dir()
                .join(format!("{}_Backup_{}", meta::app_id(), local_timestamp()));
            if let Err(e) = copy_dir_all(user_data_dir, &backup_dir) {
                eprintln!("Warning: Failed to backup user data to desktop: {}", e);
            }
        }
        if let Err(e) = std::fs::remove_dir_all(user_data_dir) {
            eprintln!("Warning: Failed to remove user data: {}", e);
        }
    }

    // 清除本地缓存
    if options.clean_local_cache && local_cache_dir.exists() {
        if let Err(e) = std::fs::remove_dir_all(&local_cache_dir) {
            eprintln!("Warning: Failed to remove local cache: {}", e);
        }
    }

    // 始终清理 WebView2 缓存
    let temp_dir = std::env::temp_dir();
    let setting_cache = temp_dir.join(meta::setting_exe_stem());
    if setting_cache.exists() {
        let _ = std::fs::remove_dir_all(&setting_cache);
    }

    Ok(())
}

/// 当前用户桌面目录（%USERPROFILE%\Desktop）
fn desktop_dir() -> PathBuf {
    let user_profile = std::env::var("USERPROFILE").unwrap_or_default();
    PathBuf::from(user_profile).join("Desktop")
}

/// 本地时间戳 YYYYMMDD_HHMMSS（用于备份目录名，使每次备份唯一、不覆盖历史）
fn local_timestamp() -> String {
    use windows::Win32::System::SystemInformation::GetLocalTime;
    let st = unsafe { GetLocalTime() };
    format!(
        "{:04}{:02}{:02}_{:02}{:02}{:02}",
        st.wYear, st.wMonth, st.wDay, st.wHour, st.wMinute, st.wSecond
    )
}

/// 递归复制目录（用于卸载前备份用户数据到桌面）
fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let dest = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_all(&entry.path(), &dest)?;
        } else {
            std::fs::copy(entry.path(), &dest)?;
        }
    }
    Ok(())
}

