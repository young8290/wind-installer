pub mod config;
#[allow(dead_code)]
pub mod extract;
pub mod legacy;
pub mod plan;
pub mod registry;
pub mod step;
pub mod steps;
pub mod userdata;
pub mod shortcut;
pub mod font;
pub mod acl;
pub mod process;
pub mod ime;

use config::InstallConfig;
use crate::archive::ArchiveReader;
use crate::meta;
use step::{run_plan, CliReporter};

/// 卸载器文件名（安装目录内）
pub const UNINSTALLER_NAME: &str = "uninstall.exe";

/// 判断归档条目是否为卸载器。便携模式不释放它——外部程序以该文件的存在判定为安装版。
pub fn is_uninstaller_entry(path: &str) -> bool {
    path.eq_ignore_ascii_case(UNINSTALLER_NAME)
}

/// 安装模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallMode {
    /// 标准安装 - 注册到系统
    Standard,
    /// 便携模式 - 仅解压文件
    Portable,
}

/// 安装结果
#[derive(Debug)]
#[allow(dead_code)]
pub struct InstallResult {
    pub success: bool,
    pub message: String,
    pub need_reboot: bool,
}

/// 执行完整安装流程（静默/CLI 路径）。
///
/// GUI 路径见 `ui::install_wizard`——两者共用 `plan::plan_install` 生成的同一份
/// 步骤计划，仅 Reporter 实现不同。
pub fn perform_install(config: &InstallConfig, mode: InstallMode) -> InstallResult {
    let mut archive = match ArchiveReader::open_current_exe() {
        Ok(a) => a,
        Err(e) => {
            return InstallResult {
                success: false,
                message: format!("Failed to open archive: {}", e),
                need_reboot: false,
            }
        }
    };

    // 与 GUI 路径同一判定：datadir.conf 写在 %LOCALAPPDATA% 是机器全局的，
    // 故「是否首装」也必须按机器判定，不能按 install_dir 是否存在卸载器判定——
    // 否则装到新目录会覆盖老用户已有的数据目录配置。
    let is_fresh_install = registry::detect_installed_version().is_none();
    let plan = plan::plan_install(meta::manifest(), mode);
    let mut reporter = CliReporter;

    match run_plan(
        &plan,
        config,
        mode,
        is_fresh_install,
        &mut archive,
        &mut reporter,
    ) {
        Ok(outcome) => InstallResult {
            success: true,
            message: "Installation completed successfully".into(),
            need_reboot: outcome.need_reboot,
        },
        Err(e) => {
            // 致命失败：确保不残留 InstallerRunning 标志，否则宿主进程将永久停摆
            let _ = registry::clear_installer_running();
            InstallResult {
                success: false,
                message: e,
                need_reboot: false,
            }
        }
    }
}
