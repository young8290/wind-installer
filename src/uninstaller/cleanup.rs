use std::path::PathBuf;

/// 清理选项
#[derive(Debug, Clone)]
pub struct CleanupOptions {
    /// 安装目录
    pub install_dir: PathBuf,
    /// 是否清除用户配置数据（%APPDATA%\WindInput）
    pub clean_roaming: bool,
    /// 是否清除本地缓存（%LOCALAPPDATA%\WindInput\cache）
    pub clean_local_cache: bool,
    /// 是否备份配置到桌面
    pub backup_to_desktop: bool,
    /// 静默模式下的保留用户数据标志
    pub keep_user_data: bool,
}

impl Default for CleanupOptions {
    fn default() -> Self {
        let program_files = std::env::var("ProgramFiles")
            .unwrap_or_else(|_| r"C:\Program Files".to_string());

        Self {
            install_dir: PathBuf::from(program_files).join("WindInput"),
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
            .join("WindInput")
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
        PathBuf::from(app_data).join("WindInput")
    }

    /// 获取本地缓存目录
    pub fn local_cache_dir(&self) -> PathBuf {
        let local_app_data = std::env::var("LOCALAPPDATA")
            .unwrap_or_else(|_| {
                let mut p = PathBuf::from(std::env::var("USERPROFILE").unwrap_or_default());
                p.push("AppData\\Local");
                p.to_string_lossy().to_string()
            });
        PathBuf::from(local_app_data).join("WindInput").join("cache")
    }
}

/// 删除安装文件
pub fn delete_install_files(install_dir: &PathBuf) -> Result<(), String> {
    // 删除二进制文件
    let binaries = vec![
        "wind_tsf.dll",
        "wind_tsf_x86.dll",
        "wind_input.exe",
        "wind_setting.exe",
        "wind_portable.exe",
        "wind_dwrite.dll",  // 旧版本遗留
    ];

    for binary in &binaries {
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

    // 备份配置到桌面（如果需要）
    if options.clean_roaming && options.backup_to_desktop {
        if user_data_dir.exists() {
            let desktop = get_desktop_path();
            let backup_dir = desktop.join("WindInput_Backup");
            if let Err(e) = copy_dir_all(&user_data_dir, &backup_dir) {
                eprintln!("Warning: Failed to backup user data: {}", e);
            }
        }
    }

    // 清除用户配置
    if options.clean_roaming && user_data_dir.exists() {
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
    let setting_cache = temp_dir.join("wind_setting");
    if setting_cache.exists() {
        let _ = std::fs::remove_dir_all(&setting_cache);
    }

    Ok(())
}

/// 获取桌面路径
fn get_desktop_path() -> PathBuf {
    let user_profile = std::env::var("USERPROFILE").unwrap_or_default();
    PathBuf::from(user_profile).join("Desktop")
}

/// 递归复制目录
fn copy_dir_all(src: &PathBuf, dst: &PathBuf) -> Result<(), std::io::Error> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dest = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dest)?;
        } else {
            std::fs::copy(entry.path(), dest)?;
        }
    }
    Ok(())
}
