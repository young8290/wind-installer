//! 应用清单（AppManifest）—— 运行期安装配置的唯一来源。
//!
//! 设计目标：让 wind-installer 成为「通用安装器生成器」。应用身份、安装行为、
//! 输入法/字体等专属逻辑全部由打包时提供的 `app.toml` 描述，序列化进归档头部，
//! 由运行期读取。**同一个预编译 stub 通过更换 app.toml 即可为不同应用生成安装包，
//! 无需重新编译。**
//!
//! 三类结构：
//! - [`AppManifest`]：进归档、运行期读取的纯配置（无任何文件路径）。
//! - [`ProjectConfig`]：`app.toml` 的完整映射 = 清单 + `[package]` 打包参数。
//! - [`PackageConfig`]：仅打包期使用（压缩、源目录、logo/icon 路径等）。

// ProjectConfig/PackageConfig 及多个序列化/默认值辅助仅 wind-packer 使用，
// 在 installer/uninstaller 二进制中不构造，故模块级允许 dead_code。
#![allow(dead_code)]

use serde::{Deserialize, Serialize};

/// 运行期清单：序列化为 TOML 文本存入归档头部，安装/卸载器启动时读取。
///
/// 不包含任何宿主机文件路径——logo 字节单独存于归档头部，icon 由 rcedit 写入 PE。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppManifest {
    pub app: AppInfo,
    #[serde(default)]
    pub ui: UiInfo,
    /// 输入法（TSF）注册信息。整段缺省 = 跳过输入法注册。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ime: Option<ImeInfo>,
    /// 需要安装到系统的字体。空 = 跳过字体安装。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub font: Vec<FontInfo>,
}

/// 应用身份与安装行为。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppInfo {
    /// 应用标识：注册表键名、默认安装目录名等。
    pub id: String,
    /// 显示名称（向导标题、Add/Remove Programs 等）。
    pub display_name: String,
    /// 版本号。
    pub version: String,
    /// 发布者。
    pub publisher: String,
    #[serde(default)]
    pub description: String,
    /// 主程序可执行文件名（相对安装目录）。
    pub main_exe: String,
    /// 设置程序可执行文件名（相对安装目录），可空。
    #[serde(default)]
    pub setting_exe: String,
    /// 开始菜单文件夹名，空则回退到 display_name。
    #[serde(default)]
    pub start_menu_folder: String,
    /// 向导窗口标题，空则回退到 "<display_name> 安装向导"。
    #[serde(default)]
    pub window_title: String,
    /// URL 协议名，空则跳过协议注册。
    #[serde(default)]
    pub url_protocol: String,
    /// 用户协议链接，空则不显示。
    #[serde(default)]
    pub agreement_url: String,
    /// 便携模式标记文件名。
    #[serde(default = "default_portable_marker")]
    pub portable_marker: String,
    /// 安装/卸载前需终止的进程名（不含 .exe）。
    #[serde(default)]
    pub process_names: Vec<String>,
    /// 需要 ALL APPLICATION PACKAGES 读取权限的 DLL（相对安装目录）。
    #[serde(default)]
    pub acl_dlls: Vec<String>,
    /// 升级时清理的旧版遗留文件（相对安装目录）。
    #[serde(default)]
    pub legacy_files: Vec<String>,
    /// 升级时清理的旧版遗留目录（相对安装目录）。
    #[serde(default)]
    pub legacy_dirs: Vec<String>,
}

/// UI 尺寸（纯运行期配置）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiInfo {
    #[serde(default = "default_install_win")]
    pub install_win: WinSize,
    #[serde(default = "default_uninstall_win")]
    pub uninstall_win: WinSize,
}

impl Default for UiInfo {
    fn default() -> Self {
        Self {
            install_win: default_install_win(),
            uninstall_win: default_uninstall_win(),
        }
    }
}

/// 窗口尺寸（像素）。
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct WinSize {
    pub w: i32,
    pub h: i32,
}

/// 输入法（TSF）注册信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImeInfo {
    /// TSF CLSID，如 "{99C2EE30-...}"。
    pub clsid: String,
    /// TSF Profile GUID，如 "{99C2EE31-...}"。
    pub profile_guid: String,
    /// 语言 ID，如 "0804"（简体中文）。
    pub lang_id: String,
    /// 64 位 TSF DLL 文件名（相对安装目录）。
    pub dll_x64: String,
    /// 32 位 TSF DLL 文件名（相对安装目录），可空。
    #[serde(default)]
    pub dll_x86: String,
}

/// 字体安装信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FontInfo {
    /// 字体文件名（如 "HeiTiZiGen.ttf"），同时是写入 %WINDIR%\Fonts 的目标名。
    pub file: String,
    /// 注册表中的字体显示名（如 "黑体字根 (TrueType)"）。
    pub display_name: String,
    /// 字体源路径，相对安装目录（如 "data/schemas/wubi86/HeiTiZiGen.ttf"）。
    pub source_rel: String,
}

// ── app.toml 完整映射（打包期）─────────────────────────────────────────────

/// `app.toml` 的完整结构 = 运行期清单 + 打包参数。
#[derive(Debug, Clone, Deserialize)]
pub struct ProjectConfig {
    #[serde(flatten)]
    pub manifest: AppManifest,
    pub package: PackageConfig,
}

/// 仅打包期使用的参数，不进归档。
#[derive(Debug, Clone, Deserialize)]
pub struct PackageConfig {
    /// 压缩算法 "zstd" / "lzma"。
    #[serde(default = "default_compression")]
    pub compression: String,
    /// 源目录（待打包的应用构建产物）。
    pub source_dir: String,
    /// 输出文件名（不含扩展名与版本号）。
    #[serde(default)]
    pub output_name: String,
    /// 输出目录。
    #[serde(default = "default_output_dir")]
    pub output_dir: String,
    /// UI 中显示的 logo 图片路径（PNG），字节会被嵌入归档头部。
    #[serde(default)]
    pub logo: String,
    /// 安装器 EXE 图标路径（.ico），由 rcedit 写入 PE 资源。
    #[serde(default)]
    pub icon: String,
}

// ── 序列化 / 反序列化 ──────────────────────────────────────────────────────

impl AppManifest {
    /// 序列化为 TOML 文本字节（存入归档头部）。
    pub fn to_toml_bytes(&self) -> Result<Vec<u8>, String> {
        toml::to_string(self)
            .map(String::into_bytes)
            .map_err(|e| format!("Failed to serialize manifest: {}", e))
    }

    /// 从 TOML 文本字节反序列化（从归档头部 / 安装目录 .manifest 读取）。
    pub fn from_toml_bytes(bytes: &[u8]) -> Result<Self, String> {
        let text = std::str::from_utf8(bytes)
            .map_err(|e| format!("Manifest is not valid UTF-8: {}", e))?;
        toml::from_str(text).map_err(|e| format!("Failed to parse manifest: {}", e))
    }

    // ── 带回退的访问器（消除调用点的空值判断）─────────────────────────────

    /// 开始菜单文件夹名，空则回退到 display_name。
    pub fn start_menu_folder(&self) -> &str {
        non_empty(&self.app.start_menu_folder).unwrap_or(&self.app.display_name)
    }

    /// 设置程序文件名去掉 .exe 后缀（进程名）。
    pub fn setting_exe_stem(&self) -> &str {
        self.app
            .setting_exe
            .strip_suffix(".exe")
            .unwrap_or(&self.app.setting_exe)
    }
}

impl ProjectConfig {
    /// 从 app.toml 文本解析。
    pub fn from_toml_str(text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|e| format!("Failed to parse app.toml: {}", e))
    }
}

fn non_empty(s: &str) -> Option<&str> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t)
    }
}

fn default_portable_marker() -> String {
    "portable_mode".to_string()
}
fn default_install_win() -> WinSize {
    WinSize { w: 520, h: 490 }
}
fn default_uninstall_win() -> WinSize {
    WinSize { w: 480, h: 440 }
}
fn default_compression() -> String {
    "zstd".to_string()
}
fn default_output_dir() -> String {
    "./dist".to_string()
}
