//! 运行期安装配置访问层。
//!
//! 历史上这里是来自 Cargo.toml 的**编译期常量**，导致换一个应用就要重新编译。
//! 现在配置来自打包时嵌入归档头部的 [`AppManifest`]（见 `manifest.rs`），由
//! [`init`] 在启动时一次性载入全局，本模块提供只读访问器。**同一个预编译 stub
//! 通过更换 app.toml 即可为不同应用生成安装包。**
//!
//! 载入时机：
//! - 安装器：从自身追加的归档头部读取（`ArchiveReader::open_current_exe`）。
//! - 卸载器：从安装目录 `.manifest`（位于卸载器自身旁边）读取。

use std::sync::OnceLock;

use crate::manifest::AppManifest;

static MANIFEST: OnceLock<AppManifest> = OnceLock::new();

/// 安装目录中持久化的清单文件名（供卸载器读取）。
pub const MANIFEST_FILE: &str = ".manifest";

/// 载入清单（仅首次生效）。必须在任何访问器调用前完成。
pub fn init(manifest: AppManifest) {
    let _ = MANIFEST.set(manifest);
}

/// 自动载入清单：
/// 1. 安装器——从自身追加的归档头部读取；
/// 2. 卸载器——回退到安装目录（卸载器自身旁）的 `.manifest`。
///
/// 自删除临时副本既无归档也无 `.manifest`，但其流程不访问清单，故不应调用本函数。
pub fn bootstrap() -> Result<(), String> {
    // 1. 自身追加的归档（安装器场景）
    if let Ok(reader) = crate::archive::ArchiveReader::open_current_exe() {
        let bytes = reader.manifest_bytes();
        if !bytes.is_empty() {
            init(AppManifest::from_toml_bytes(bytes)?);
            return Ok(());
        }
    }

    // 2. 安装目录中的 .manifest（卸载器场景）
    let exe = std::env::current_exe().map_err(|e| format!("获取自身路径失败: {}", e))?;
    if let Some(dir) = exe.parent() {
        let path = dir.join(MANIFEST_FILE);
        if let Ok(bytes) = std::fs::read(&path) {
            init(AppManifest::from_toml_bytes(&bytes)?);
            return Ok(());
        }
    }

    Err("未找到安装清单：归档头部为空且 .manifest 不可读".into())
}

/// 是否已载入清单。
pub fn is_initialized() -> bool {
    MANIFEST.get().is_some()
}

/// 获取全局清单。未初始化即 panic——属于编程错误（启动时必须先 init）。
pub fn manifest() -> &'static AppManifest {
    MANIFEST
        .get()
        .expect("manifest 未初始化：启动时必须先调用 meta::init")
}

// ── 字符串访问器（'static，因清单存于全局 OnceLock）─────────────────────────

pub fn app_id() -> &'static str {
    &manifest().app.id
}
pub fn app_display_name() -> &'static str {
    &manifest().app.display_name
}
pub fn app_version() -> &'static str {
    &manifest().app.version
}
pub fn app_publisher() -> &'static str {
    &manifest().app.publisher
}
pub fn main_exe() -> &'static str {
    &manifest().app.main_exe
}
pub fn setting_exe() -> &'static str {
    &manifest().app.setting_exe
}
pub fn url_protocol() -> &'static str {
    &manifest().app.url_protocol
}
pub fn agreement_url() -> &'static str {
    &manifest().app.agreement_url
}
pub fn portable_marker() -> &'static str {
    &manifest().app.portable_marker
}
pub fn start_menu_folder() -> &'static str {
    manifest().start_menu_folder()
}
pub fn setting_exe_stem() -> &'static str {
    manifest().setting_exe_stem()
}

/// 卸载备份目录名（空则回退 "<id>_Backup"）。
pub fn backup_dir() -> String {
    manifest().backup_dir()
}

/// 向导窗口标题：清单留空则回退到 "<display_name> 安装向导"。
pub fn window_title() -> String {
    let t = manifest().app.window_title.trim();
    if t.is_empty() {
        format!("{} 安装向导", app_display_name())
    } else {
        t.to_string()
    }
}

// ── 列表访问器 ──────────────────────────────────────────────────────────────

pub fn process_names() -> &'static [String] {
    &manifest().app.process_names
}
pub fn acl_dlls() -> &'static [String] {
    &manifest().app.acl_dlls
}
pub fn legacy_files() -> &'static [String] {
    &manifest().app.legacy_files
}
pub fn legacy_dirs() -> &'static [String] {
    &manifest().app.legacy_dirs
}

// ── 窗口尺寸 ────────────────────────────────────────────────────────────────

pub fn install_win() -> (i32, i32) {
    let s = manifest().ui.install_win;
    (s.w, s.h)
}
pub fn uninstall_win() -> (i32, i32) {
    let s = manifest().ui.uninstall_win;
    (s.w, s.h)
}
