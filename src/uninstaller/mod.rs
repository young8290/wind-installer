pub mod cleanup;
pub mod plan;
pub mod selfdelete;
pub mod steps;

use cleanup::CleanupOptions;

use crate::installer::step::{run_plan, CliReporter};
use steps::UninstallCtx;

/// 卸载结果
#[derive(Debug)]
#[allow(dead_code)]
pub struct UninstallResult {
    pub success: bool,
    pub message: String,
    pub need_reboot: bool,
}

/// 执行完整卸载流程（静默/CLI 路径）。
///
/// GUI 路径见 `ui::uninstall_wizard`——两者共用 `plan::plan_uninstall` 生成的
/// 同一份步骤计划，仅 Reporter 实现不同。
pub fn perform_uninstall(options: &CleanupOptions) -> UninstallResult {
    let plan = plan::plan_uninstall();
    let mut reporter = CliReporter;

    let mut ctx = UninstallCtx::new(options);

    let outcome = run_plan(&plan, &mut ctx, &mut reporter);
    // 卸载计划无致命步骤：一路尽力而为，删不掉的东西转化为「需要重启」
    let need_reboot = ctx.need_reboot || outcome.map(|o| o.need_reboot).unwrap_or(false);

    let _ = crate::installer::registry::clear_installer_running();

    UninstallResult {
        success: true,
        message: "Uninstallation completed".into(),
        need_reboot,
    }
}
