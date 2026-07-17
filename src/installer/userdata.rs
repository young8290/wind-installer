use std::path::{Path, PathBuf};

use crate::meta;

/// 数据目录配置文件的绝对路径 `%LOCALAPPDATA%\{app.id}\{conf_file}`（纯计算，不落盘）。
///
/// 回执要记这个路径——无论本次是否真的写了它。升级时不重写（保留用户既有配置），
/// 但文件仍属本产品，卸载时该删；不记就会永久残留，并污染下次全新安装的数据目录解析。
pub fn datadir_conf_path(conf_file: &str) -> PathBuf {
    let local_app_data = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| {
        let up = std::env::var("USERPROFILE").unwrap_or_default();
        format!(r"{}\AppData\Local", up)
    });
    PathBuf::from(local_app_data)
        .join(meta::app_id())
        .join(conf_file)
}

/// 将用户数据目录写入配置文件，返回写入的绝对路径。
/// 仅在首次安装时调用；升级时跳过以保留旧配置。
pub fn write_datadir_conf(data_dir: &Path, conf_file: &str) -> Result<PathBuf, String> {
    let path = datadir_conf_path(conf_file);
    let conf_dir = path.parent().ok_or("配置路径无父目录")?;

    std::fs::create_dir_all(conf_dir)
        .map_err(|e| format!("Failed to create conf dir: {}", e))?;
    std::fs::write(&path, data_dir.to_string_lossy().as_bytes())
        .map_err(|e| format!("Failed to write {}: {}", conf_file, e))?;

    Ok(path)
}
