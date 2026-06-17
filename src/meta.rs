/// 来自 Cargo.toml [package] 和 [package.metadata.installer] 的编译期常量。
/// CI 只需修改 Cargo.toml，无需改动源代码。
pub const APP_DISPLAY_NAME: &str = env!("WIND_DISPLAY_NAME");
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const APP_PUBLISHER: &str = env!("WIND_PUBLISHER");
pub const APP_START_MENU_FOLDER: &str = env!("WIND_START_MENU_FOLDER");
pub const APP_WINDOW_TITLE: &str = env!("WIND_WINDOW_TITLE");
