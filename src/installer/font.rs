use std::path::Path;

use winreg::enums::*;
use winreg::RegKey;

/// 字体文件名
const FONT_FILE: &str = "HeiTiZiGen.ttf";
/// 字体显示名称
const FONT_DISPLAY_NAME: &str = "黑体字根 (TrueType)";
/// 字体跟踪注册表路径
const FONT_TRACKING_KEY: &str = r"SOFTWARE\WindInput";

/// 安装系统字体
pub fn install_font(install_dir: &Path) -> Result<(), String> {
    let font_source = install_dir
        .join("data")
        .join("schemas")
        .join("wubi86")
        .join(FONT_FILE);

    if !font_source.exists() {
        return Err(format!("Font file not found: {:?}", font_source));
    }

    let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".to_string());
    let font_dest = Path::new(&windir).join("Fonts").join(FONT_FILE);

    // 检查字体是否已存在且一致
    if font_dest.exists() {
        if let (Ok(src_meta), Ok(dst_meta)) = (
            std::fs::metadata(&font_source),
            std::fs::metadata(&font_dest),
        ) {
            if src_meta.len() == dst_meta.len() {
                // 文件大小一致，跳过复制
                // 注册字体（确保注册表正确）
                register_font_in_registry()?;
                set_font_tracking()?;
                return Ok(());
            }
        }
    }

    // 复制字体文件
    std::fs::copy(&font_source, &font_dest)
        .map_err(|e| format!("Failed to copy font file: {}", e))?;

    // 注册字体
    register_font_in_registry()?;

    // 设置跟踪标记
    set_font_tracking()?;

    Ok(())
}

/// 在注册表中注册字体
fn register_font_in_registry() -> Result<(), String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let fonts_key = hklm
        .open_subkey_with_flags(
            r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts",
            KEY_WRITE,
        )
        .map_err(|e| format!("Failed to open Fonts key: {}", e))?;

    fonts_key
        .set_value(FONT_DISPLAY_NAME, &FONT_FILE)
        .map_err(|e| format!("Failed to register font: {}", e))?;

    Ok(())
}

/// 设置字体安装跟踪标记
fn set_font_tracking() -> Result<(), String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let (tracking_key, _) = hklm
        .create_subkey(FONT_TRACKING_KEY)
        .map_err(|e| format!("Failed to create tracking key: {}", e))?;

    tracking_key
        .set_value("InstalledFont_HeiTiZiGen", &"1")
        .map_err(|e| format!("Failed to set font tracking: {}", e))?;

    Ok(())
}

/// 卸载系统字体
pub fn uninstall_font() -> Result<(), String> {
    // 检查是否是我们安装的字体
    if !is_font_installed_by_us() {
        return Ok(());
    }

    let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".to_string());
    let font_path = Path::new(&windir).join("Fonts").join(FONT_FILE);

    // 删除字体文件
    if font_path.exists() {
        std::fs::remove_file(&font_path)
            .map_err(|e| format!("Failed to delete font file: {}", e))?;
    }

    // 从注册表移除字体
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    if let Ok(fonts_key) = hklm.open_subkey_with_flags(
        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts",
        KEY_WRITE,
    ) {
        let _ = fonts_key.delete_value(FONT_DISPLAY_NAME);
    }

    // 清除跟踪标记
    if let Ok(tracking_key) = hklm.open_subkey_with_flags(FONT_TRACKING_KEY, KEY_WRITE) {
        let _ = tracking_key.delete_value("InstalledFont_HeiTiZiGen");
    }

    Ok(())
}

/// 检查字体是否是我们安装的
pub fn is_font_installed_by_us() -> bool {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    if let Ok(key) = hklm.open_subkey_with_flags(FONT_TRACKING_KEY, KEY_READ) {
        if let Ok(value) = key.get_value::<String, _>("InstalledFont_HeiTiZiGen") {
            return value == "1";
        }
    }
    false
}
