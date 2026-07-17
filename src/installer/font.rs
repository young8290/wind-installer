use std::path::Path;

use winreg::enums::*;
use winreg::RegKey;

use crate::manifest::FontInfo;

/// 系统字体注册表路径
const FONTS_KEY: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts";

/// 安装单个系统字体。
///
/// 调用方（`steps::InstallFonts`）逐个字体调用并按成功与否写回执。安装回执取代了
/// 此前的「InstalledFont_* 跟踪值」机制：两者作用相同（证明字体是我们装的），
/// 但回执更精确——装成几个记几条，不会因中途失败而让已装的字体漏记。
pub fn install_one(install_dir: &Path, font: &FontInfo) -> Result<(), String> {
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

/// 按回执记录的文件名与显示名卸载单个字体。
///
/// 不读清单——回执的存在本身即证明这个字体是我们装的，故升级后清单里已删除的
/// 字体仍能被上一版回执正确清掉。
pub fn uninstall_font_file(file: &str, display_name: &str) -> Result<(), String> {
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".to_string());
    let font_path = Path::new(&windir).join("Fonts").join(file);

    if font_path.exists() {
        std::fs::remove_file(&font_path)
            .map_err(|e| format!("删除字体文件失败 {:?}: {}", font_path, e))?;
    }

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    if let Ok(fonts_key) = hklm.open_subkey_with_flags(FONTS_KEY, KEY_WRITE) {
        let _ = fonts_key.delete_value(display_name);
    }

    Ok(())
}
