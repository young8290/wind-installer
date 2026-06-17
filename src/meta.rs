/// 来自 Cargo.toml [package] 和 [package.metadata.installer] 的编译期常量。
/// CI 只需修改 Cargo.toml，无需改动源代码。
pub const APP_DISPLAY_NAME: &str = env!("WIND_DISPLAY_NAME");
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const APP_PUBLISHER: &str = env!("WIND_PUBLISHER");
pub const APP_START_MENU_FOLDER: &str = env!("WIND_START_MENU_FOLDER");
pub const APP_WINDOW_TITLE: &str = env!("WIND_WINDOW_TITLE");
pub const APP_ID: &str = env!("WIND_APP_ID");
pub const MAIN_EXE: &str = env!("WIND_MAIN_EXE");
pub const SETTING_EXE: &str = env!("WIND_SETTING_EXE");
pub const URL_PROTOCOL: &str = env!("WIND_URL_PROTOCOL");
pub const PROCESS_NAMES_CSV: &str = env!("WIND_PROCESS_NAMES");
pub const ACL_DLLS_CSV: &str = env!("WIND_ACL_DLLS");
pub const BACKUP_DIR: &str = env!("WIND_BACKUP_DIR");
pub const PORTABLE_MARKER: &str = env!("WIND_PORTABLE_MARKER");

pub fn process_names() -> Vec<&'static str> {
    split_csv(PROCESS_NAMES_CSV)
}

pub fn acl_dlls() -> Vec<&'static str> {
    split_csv(ACL_DLLS_CSV)
}

pub fn setting_exe_stem() -> &'static str {
    SETTING_EXE.strip_suffix(".exe").unwrap_or(SETTING_EXE)
}

fn split_csv(s: &'static str) -> Vec<&'static str> {
    s.split(',').map(str::trim).filter(|s| !s.is_empty()).collect()
}
