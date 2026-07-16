use std::path::Path;

use crate::meta;

/// 将用户数据目录写入 `%LOCALAPPDATA%\{app.id}\{conf_file}`。
/// 仅在首次安装时调用；升级时跳过以保留旧配置。
pub fn write_datadir_conf(data_dir: &Path, conf_file: &str) -> Result<(), String> {
    let local_app_data = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| {
        let up = std::env::var("USERPROFILE").unwrap_or_default();
        format!(r"{}\AppData\Local", up)
    });
    let conf_dir = std::path::PathBuf::from(local_app_data).join(meta::app_id());
    std::fs::create_dir_all(&conf_dir)
        .map_err(|e| format!("Failed to create conf dir: {}", e))?;
    std::fs::write(
        conf_dir.join(conf_file),
        data_dir.to_string_lossy().as_bytes(),
    )
    .map_err(|e| format!("Failed to write {}: {}", conf_file, e))
}
