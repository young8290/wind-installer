use std::path::{Path, PathBuf};

use crate::meta;

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
    /// 获取用户数据目录
    pub fn user_data_dir(&self) -> PathBuf {
        // 检查是否有自定义数据目录
        let local_app_data = std::env::var("LOCALAPPDATA")
            .unwrap_or_else(|_| {
                let mut p = PathBuf::from(std::env::var("USERPROFILE").unwrap_or_default());
                p.push("AppData\\Local");
                p.to_string_lossy().to_string()
            });
        let datadir_conf = PathBuf::from(&local_app_data)
            .join(meta::app_id())
            .join("datadir.conf");

        if datadir_conf.exists() {
            if let Ok(content) = std::fs::read_to_string(&datadir_conf) {
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
        if path.exists() {
            if let Err(_) = std::fs::remove_file(&path) {
                // 文件被锁定，尝试重命名
                let random_suffix = rand::random::<u32>();
                let old_name = format!("{}.old_{:08x}", binary, random_suffix);
                let old_path = install_dir.join(&old_name);
                let _ = std::fs::rename(&path, &old_path);
            }
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

    // 清理备份文件
    if let Ok(entries) = std::fs::read_dir(install_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.contains(".old_") || name.ends_with(".bak") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    // 尝试删除安装目录
    if let Err(_) = std::fs::remove_dir_all(install_dir) {
        eprintln!("Warning: Could not delete install directory");
    }

    Ok(())
}

/// 清理注册表
pub fn cleanup_registry() {
    // 移除自启动
    let _ = crate::installer::registry::remove_auto_start();

    // 移除 URL 协议
    let _ = crate::installer::registry::unregister_url_protocol();

    // 移除卸载信息
    let _ = crate::installer::registry::remove_uninstall_info();
}

/// 清理用户数据
pub fn cleanup_user_data(options: &CleanupOptions) -> Result<(), String> {
    if options.keep_user_data {
        return Ok(());
    }

    let user_data_dir = options.user_data_dir();
    let local_cache_dir = options.local_cache_dir();

    // 清除用户配置（删除前可选备份到桌面）
    if options.clean_roaming && user_data_dir.exists() {
        if options.backup_to_desktop {
            // 目录名带本地时间戳，每次卸载生成唯一目录，避免覆盖历史备份
            let backup_dir = desktop_dir()
                .join(format!("{}_Backup_{}", meta::app_id(), local_timestamp()));
            if let Err(e) = copy_dir_all(&user_data_dir, &backup_dir) {
                eprintln!("Warning: Failed to backup user data to desktop: {}", e);
            }
        }
        if let Err(e) = std::fs::remove_dir_all(&user_data_dir) {
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

