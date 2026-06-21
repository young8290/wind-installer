use std::path::Path;

use winreg::enums::*;
use winreg::RegKey;

use crate::manifest::FontInfo;
use crate::meta;

/// 系统字体注册表路径
const FONTS_KEY: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts";

/// 字体跟踪注册表路径（运行时构造）
fn font_tracking_key() -> String {
    format!(r"SOFTWARE\{}", meta::app_id())
}

/// 单个字体的跟踪值名（按文件名区分，支持多字体）
fn tracking_value_name(font: &FontInfo) -> String {
    format!("InstalledFont_{}", font.file)
}

/// 安装清单中声明的所有系统字体
pub fn install_font(install_dir: &Path) -> Result<(), String> {
    for font in &meta::manifest().font {
        install_one(install_dir, font)?;
    }
    Ok(())
}

fn install_one(install_dir: &Path, font: &FontInfo) -> Result<(), String> {
    let font_source = install_dir.join(&font.source_rel);
    if !font_source.exists() {
        return Err(format!("Font file not found: {:?}", font_source));
    }

    let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".to_string());
    let font_dest = Path::new(&windir).join("Fonts").join(&font.file);

    // 字体已存在且大小一致则跳过复制，仅确保注册表正确
    let same_size = font_dest.exists()
        && matches!(
            (std::fs::metadata(&font_source), std::fs::metadata(&font_dest)),
            (Ok(s), Ok(d)) if s.len() == d.len()
        );

    if !same_size {
        std::fs::copy(&font_source, &font_dest)
            .map_err(|e| format!("Failed to copy font file: {}", e))?;
    }

    register_font_in_registry(font)?;
    set_font_tracking(font)?;
    Ok(())
}

/// 在注册表中注册字体
fn register_font_in_registry(font: &FontInfo) -> Result<(), String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let fonts_key = hklm
        .open_subkey_with_flags(FONTS_KEY, KEY_WRITE)
        .map_err(|e| format!("Failed to open Fonts key: {}", e))?;

    fonts_key
        .set_value(&font.display_name, &font.file)
        .map_err(|e| format!("Failed to register font: {}", e))?;

    Ok(())
}

/// 设置字体安装跟踪标记
fn set_font_tracking(font: &FontInfo) -> Result<(), String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let (tracking_key, _) = hklm
        .create_subkey(font_tracking_key())
        .map_err(|e| format!("Failed to create tracking key: {}", e))?;

    tracking_key
        .set_value(tracking_value_name(font), &"1")
        .map_err(|e| format!("Failed to set font tracking: {}", e))?;

    Ok(())
}

/// 卸载清单中声明的、由本安装器安装的所有系统字体
pub fn uninstall_font() -> Result<(), String> {
    for font in &meta::manifest().font {
        uninstall_one(font);
    }
    Ok(())
}

fn uninstall_one(font: &FontInfo) {
    if !is_font_installed_by_us(font) {
        return;
    }

    let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".to_string());
    let font_path = Path::new(&windir).join("Fonts").join(&font.file);

    if font_path.exists() {
        let _ = std::fs::remove_file(&font_path);
    }

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    if let Ok(fonts_key) = hklm.open_subkey_with_flags(FONTS_KEY, KEY_WRITE) {
        let _ = fonts_key.delete_value(&font.display_name);
    }

    if let Ok(tracking_key) = hklm.open_subkey_with_flags(&font_tracking_key(), KEY_WRITE) {
        let _ = tracking_key.delete_value(tracking_value_name(font));
    }
}

/// 检查指定字体是否由本安装器安装
pub fn is_font_installed_by_us(font: &FontInfo) -> bool {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    if let Ok(key) = hklm.open_subkey_with_flags(&font_tracking_key(), KEY_READ) {
        if let Ok(value) = key.get_value::<String, _>(tracking_value_name(font)) {
            return value == "1";
        }
    }
    false
}
