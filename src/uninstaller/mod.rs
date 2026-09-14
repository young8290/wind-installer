pub mod cleanup;
pub mod plan;
pub mod selfdelete;
pub mod steps;

use cleanup::CleanupOptions;

use crate::installer::step::{run_plan, CliReporter, WarningCollector};
use crate::util::log::RunLogger;
use steps::UninstallCtx;

/// 卸载结果
#[derive(Debug)]
#[allow(dead_code)]
pub struct UninstallResult {
    /// 是否**干干净净**地卸完了。
    ///
    /// 注意这不是「卸载过程有没有跑完」——卸载计划无致命步骤，它总能跑到最后。
    /// 这里为假表示**有产物没清掉**（反注册失败、文件删不动、回执损坏……），
    /// 详情在 [`warnings`](Self::warnings) 与卸载日志里。
    pub success: bool,
    pub message: String,
    pub need_reboot: bool,
    /// 没干成的那些事，逐条原文。为空才等于 `success`。
    pub warnings: Vec<String>,
    /// 卸载日志路径，出问题时唯一能看的东西。
    pub log_path: String,
}

/// 执行完整卸载流程（静默/CLI 路径）。
///
/// GUI 路径见 `ui::uninstall_wizard`——两者共用 `plan::plan_uninstall` 生成的
/// 同一份步骤计划，仅 Reporter 实现不同。
pub fn perform_uninstall(options: &CleanupOptions) -> UninstallResult {
    let mut logger = RunLogger::uninstall();
    logger.log(&format!("安装目录: {:?}", options.install_dir));
    let log_path = logger.path.to_string_lossy().to_string();

    let plan = plan::plan_uninstall();
    let mut reporter = CliReporter;
    let mut collector = WarningCollector::new(&mut reporter);

    let mut ctx = UninstallCtx::new(options);

    let outcome = run_plan(&plan, &mut ctx, &mut collector);
    // 卸载计划无致命步骤：一路尽力而为，删不掉的东西转化为「需要重启」
    let need_reboot = ctx.need_reboot || outcome.as_ref().map(|o| o.need_reboot).unwrap_or(false);

    let mut warnings = collector.into_warnings();
    // 真有致命步骤时（目前一个都没有）也别把它吞掉。
    if let Err(e) = outcome {
        warnings.push(e);
    }

    let _ = crate::installer::registry::clear_installer_running();

    for w in &warnings {
        logger.log_error(w);
    }
    if need_reboot {
        logger.log("部分文件被占用，已排入重启清理队列");
    }
    logger.log(if warnings.is_empty() {
        "=== 卸载完成 ==="
    } else {
        "=== 卸载完成，但有项目未能清除 ==="
    });

    UninstallResult {
        success: warnings.is_empty(),
        message: if warnings.is_empty() {
            "Uninstallation completed".into()
        } else {
            format!("Uninstallation finished with {} problem(s)", warnings.len())
        },
        need_reboot,
        warnings,
        log_path,
    }
}
