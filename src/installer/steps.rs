//! [`Step`] 的具体实现 —— 每个能力一个类型，由 `plan::plan_install` 按清单装配。
//!
//! 新增一种安装能力 = 在此加一个 `impl Step` + 在 planner 里加一行门控，
//! 不再需要同时修改 GUI 与静默两条编排链。

use crate::manifest::{AutoStartInfo, DataDirInfo, ShortcutInfo, StartupInfo};
use crate::meta;

use super::step::{Step, StepCtx};
use super::{acl, font, ime, is_uninstaller_entry, legacy, process, registry, shortcut, userdata};
use super::{InstallMode, UNINSTALLER_NAME};

// ── 安装环境标志 ────────────────────────────────────────────────────────────

/// 写入 InstallerRunning 标志，防止宿主进程在安装期间被其他组件重新拉起。
pub struct SetInstallerRunning;

impl Step for SetInstallerRunning {
    fn name(&self) -> String {
        "正在准备安装环境...".into()
    }
    fn run(&self, _ctx: &mut StepCtx) -> Result<(), String> {
        registry::set_installer_running()
    }
}

/// 清除 InstallerRunning 标志。
pub struct ClearInstallerRunning;

impl Step for ClearInstallerRunning {
    fn name(&self) -> String {
        "正在完成安装...".into()
    }
    fn run(&self, _ctx: &mut StepCtx) -> Result<(), String> {
        registry::clear_installer_running()
    }
}

// ── 前置清理 ────────────────────────────────────────────────────────────────

/// 终止 app.process_names 声明的进程。
pub struct TerminateProcesses;

impl Step for TerminateProcesses {
    fn name(&self) -> String {
        "正在停止旧进程...".into()
    }
    fn run(&self, _ctx: &mut StepCtx) -> Result<(), String> {
        process::terminate_app_processes()
    }
}

/// 反注册上一版本的 COM 组件（升级场景）。
pub struct UnregisterOldCom;

impl Step for UnregisterOldCom {
    fn name(&self) -> String {
        "正在反注册旧 COM...".into()
    }
    fn run(&self, ctx: &mut StepCtx) -> Result<(), String> {
        ime::unregister_old_com(&ctx.config.install_dir)
    }
}

/// 删除新版已移除的旧版遗留文件/目录。
pub struct CleanupLegacy;

impl Step for CleanupLegacy {
    fn name(&self) -> String {
        "正在清理旧版遗留文件...".into()
    }
    fn run(&self, ctx: &mut StepCtx) -> Result<(), String> {
        legacy::cleanup_legacy(&ctx.config.install_dir);
        Ok(())
    }
}

// ── 文件释放 ────────────────────────────────────────────────────────────────

/// 将整个压缩块解压到内存。与释放分开成步，使 UI 能单独显示解压阶段。
pub struct PrepareArchive;

impl Step for PrepareArchive {
    fn name(&self) -> String {
        "正在解压数据...".into()
    }
    fn fatal(&self) -> bool {
        true
    }
    fn run(&self, ctx: &mut StepCtx) -> Result<(), String> {
        std::fs::create_dir_all(&ctx.config.install_dir)
            .map_err(|e| format!("无法创建安装目录: {}", e))?;
        ctx.archive.prepare()
    }
}

/// 逐文件写盘。便携模式跳过卸载器——外部程序以该文件的存在判定为安装版。
pub struct ExtractFiles;

impl Step for ExtractFiles {
    fn name(&self) -> String {
        "正在释放文件...".into()
    }
    fn fatal(&self) -> bool {
        true
    }
    fn run(&self, ctx: &mut StepCtx) -> Result<(), String> {
        let entries: Vec<_> = ctx
            .archive
            .entries()
            .iter()
            .filter(|e| ctx.mode == InstallMode::Standard || !is_uninstaller_entry(&e.path))
            .cloned()
            .collect();

        let total = entries.len();
        for (i, entry) in entries.iter().enumerate() {
            let dest = ctx.config.install_dir.join(&entry.path);
            ctx.reporter
                .step_progress(&entry.path, (i + 1) as f32 / total.max(1) as f32);
            ctx.archive
                .extract_entry(entry, &dest)
                .map_err(|e| format!("释放 {} 失败: {}", entry.path, e))?;
        }
        Ok(())
    }
}

/// 给解压出的卸载器追加清单 overlay，使其自包含——安装目录不留散落文件。
pub struct AppendUninstallerOverlay;

impl Step for AppendUninstallerOverlay {
    fn name(&self) -> String {
        "正在写入卸载器清单...".into()
    }
    fn run(&self, ctx: &mut StepCtx) -> Result<(), String> {
        let uninstaller = ctx.config.install_dir.join(UNINSTALLER_NAME);
        if !uninstaller.exists() {
            return Ok(());
        }
        crate::archive::append_manifest_overlay(
            &uninstaller,
            ctx.archive.manifest_bytes(),
            ctx.archive.logo_bytes(),
        )
    }
}

/// 写入便携模式标记文件。
pub struct WritePortableMarker;

impl Step for WritePortableMarker {
    fn name(&self) -> String {
        "正在写入便携模式标记...".into()
    }
    fn run(&self, ctx: &mut StepCtx) -> Result<(), String> {
        std::fs::write(
            ctx.config.install_dir.join(meta::portable_marker()),
            "portable=1\n",
        )
        .map_err(|e| format!("无法写入便携标记: {}", e))
    }
}

// ── 系统集成 ────────────────────────────────────────────────────────────────

/// 给 app.acl_dlls 授予 ALL APPLICATION PACKAGES 读取权限。
pub struct SetDllAcl;

impl Step for SetDllAcl {
    fn name(&self) -> String {
        "正在设置文件权限...".into()
    }
    fn run(&self, ctx: &mut StepCtx) -> Result<(), String> {
        acl::set_dll_permissions(&ctx.config.install_dir)
    }
}

/// 安装 [[font]] 声明的字体到系统。
pub struct InstallFonts;

impl Step for InstallFonts {
    fn name(&self) -> String {
        "正在安装字体...".into()
    }
    fn run(&self, ctx: &mut StepCtx) -> Result<(), String> {
        font::install_font(&ctx.config.install_dir)
    }
}

/// 注册 TSF COM 组件。
pub struct RegisterCom;

impl Step for RegisterCom {
    fn name(&self) -> String {
        "正在注册 COM 组件...".into()
    }
    fn needs_reboot_on_failure(&self) -> bool {
        true
    }
    fn run(&self, ctx: &mut StepCtx) -> Result<(), String> {
        ime::register_com(&ctx.config.install_dir)
    }
}

/// 注册为系统输入法（TSF profile）。
pub struct RegisterInputMethod;

impl Step for RegisterInputMethod {
    fn name(&self) -> String {
        "正在注册系统输入法...".into()
    }
    fn run(&self, _ctx: &mut StepCtx) -> Result<(), String> {
        ime::register_input_method()
    }
}

/// 注册开机自启动。
pub struct SetAutoStart {
    pub info: AutoStartInfo,
}

impl Step for SetAutoStart {
    fn name(&self) -> String {
        "正在配置开机自启动...".into()
    }
    fn run(&self, ctx: &mut StepCtx) -> Result<(), String> {
        registry::set_auto_start(&ctx.config.install_dir, &self.info)
    }
}

/// 注册自定义 URL 协议。
pub struct RegisterUrlProtocol;

impl Step for RegisterUrlProtocol {
    fn name(&self) -> String {
        "正在注册协议...".into()
    }
    fn run(&self, ctx: &mut StepCtx) -> Result<(), String> {
        registry::register_url_protocol(&ctx.config.install_dir)
    }
}

/// 创建 [[shortcut]] 声明的快捷方式。
pub struct CreateShortcuts {
    pub items: Vec<ShortcutInfo>,
}

impl Step for CreateShortcuts {
    fn name(&self) -> String {
        "正在创建快捷方式...".into()
    }
    fn run(&self, ctx: &mut StepCtx) -> Result<(), String> {
        shortcut::create_shortcuts(&ctx.config.install_dir, &self.items)
    }
}

/// 写入 Add/Remove Programs 卸载信息。
pub struct WriteUninstallInfo;

impl Step for WriteUninstallInfo {
    fn name(&self) -> String {
        "正在写入卸载信息...".into()
    }
    fn run(&self, ctx: &mut StepCtx) -> Result<(), String> {
        registry::write_uninstall_info(ctx.config)
    }
}

/// 首次安装时写入用户数据目录配置；升级时跳过以保留旧配置。
pub struct WriteDataDirConf {
    pub info: DataDirInfo,
}

impl Step for WriteDataDirConf {
    fn name(&self) -> String {
        "正在写入数据目录配置...".into()
    }
    fn run(&self, ctx: &mut StepCtx) -> Result<(), String> {
        if !ctx.is_fresh_install {
            return Ok(());
        }
        userdata::write_datadir_conf(ctx.config.effective_data_dir(), &self.info.conf_file)
    }
}

/// 安装完成后启动主程序。
pub struct PrestartApp {
    pub info: StartupInfo,
}

impl Step for PrestartApp {
    fn name(&self) -> String {
        "正在启动服务...".into()
    }
    fn run(&self, ctx: &mut StepCtx) -> Result<(), String> {
        process::prestart_app(&ctx.config.install_dir, self.info.exe_or(meta::main_exe()))
    }
}
