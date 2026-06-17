use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

/// 安装过程日志，写入 %TEMP%\WindInput-install.log
pub struct InstallLogger {
    file: Option<File>,
    pub path: PathBuf,
}

impl InstallLogger {
    pub fn new() -> Self {
        let path = std::env::temp_dir().join(format!("{}-install.log", crate::meta::APP_ID));
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .ok();
        let mut logger = Self { file, path };
        logger.log("=== 安装开始 ===");
        logger
    }

    /// 写入一行日志（立即 flush，确保崩溃时不丢日志）
    pub fn log(&mut self, msg: &str) {
        let timestamp = utc_hms();
        if let Some(f) = &mut self.file {
            let _ = writeln!(f, "[{}] {}", timestamp, msg);
            let _ = f.flush();
        }
    }

    pub fn log_error(&mut self, msg: &str) {
        self.log(&format!("ERROR: {}", msg));
    }
}

/// 从 SystemTime 提取 UTC HH:MM:SS
fn utc_hms() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let h = (secs % 86400) / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{:02}:{:02}:{:02}", h, m, s)
}
