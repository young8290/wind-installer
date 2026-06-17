fn main() {
    // Cargo.toml 变动时重新运行
    println!("cargo:rerun-if-changed=Cargo.toml");

    // 读取 [package.metadata.installer] 并作为编译期环境变量输出
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let cargo_toml_path = format!("{}/Cargo.toml", manifest_dir);
    let raw = std::fs::read_to_string(&cargo_toml_path)
        .unwrap_or_default();
    let doc: toml::Value = toml::from_str(&raw).unwrap_or(toml::Value::Table(Default::default()));

    let meta = doc
        .get("package")
        .and_then(|p| p.get("metadata"))
        .and_then(|m| m.get("installer"));

    let get = |key: &str, default: &str| -> String {
        meta.and_then(|m| m.get(key))
            .and_then(|v| v.as_str())
            .unwrap_or(default)
            .to_string()
    };

    println!("cargo:rustc-env=WIND_DISPLAY_NAME={}", get("display_name", "清风输入法"));
    println!("cargo:rustc-env=WIND_PUBLISHER={}", get("publisher", "清风输入法 项目"));
    println!("cargo:rustc-env=WIND_START_MENU_FOLDER={}", get("start_menu_folder", "清风输入法"));
    println!("cargo:rustc-env=WIND_WINDOW_TITLE={}", get("window_title", "清风输入法 安装向导"));
    println!("cargo:rustc-env=WIND_APP_ID={}", get("app_id", "App"));
    println!("cargo:rustc-env=WIND_MAIN_EXE={}", get("main_exe", "app.exe"));
    println!("cargo:rustc-env=WIND_SETTING_EXE={}", get("setting_exe", ""));
    println!("cargo:rustc-env=WIND_URL_PROTOCOL={}", get("url_protocol", ""));
    println!("cargo:rustc-env=WIND_PROCESS_NAMES={}", get("process_names", ""));
    println!("cargo:rustc-env=WIND_ACL_DLLS={}", get("acl_dlls", ""));
    println!("cargo:rustc-env=WIND_BACKUP_DIR={}", get("backup_dir", ""));
    println!("cargo:rustc-env=WIND_PORTABLE_MARKER={}", get("portable_marker", "portable_mode"));

    // UAC Manifest + 图标
    embed_resource::compile_for("assets/app.rc", ["wind-installer", "wind-uninstaller"], embed_resource::NONE);
}
