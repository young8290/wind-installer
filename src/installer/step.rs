//! 安装步骤抽象 —— 编排的唯一真相。
//!
//! 历史上 GUI 路径（`ui::install_wizard`）与静默路径（`installer::perform_install`）
//! 各自手写了一遍步骤序列，两者已漂移出缺陷（静默路径漏写 datadir.conf、GUI 路径
//! 不设 InstallerRunning 标志）。现在两条路径共用 [`plan_install`] 生成的同一份
//! [`Step`] 列表，差异收敛到 [`Reporter`]：GUI 传 channel 实现，CLI 传 stderr 实现。
//!
//! [`plan_install`]: super::plan::plan_install

use crate::archive::ArchiveReader;

use super::config::InstallConfig;
use super::InstallMode;

/// 步骤执行上下文。
pub struct StepCtx<'a> {
    pub config: &'a InstallConfig,
    pub mode: InstallMode,
    /// 是否首次安装（非升级）——决定是否写入用户数据目录配置。
    pub is_fresh_install: bool,
    /// 自身归档，供解压与卸载器 overlay 追加使用。
    pub archive: &'a mut ArchiveReader,
    /// 进度汇报出口，步骤内的细粒度进度经此上报。
    pub reporter: &'a mut dyn Reporter,
}

/// 一个安装/卸载动作。
///
/// 失败语义由 [`Step::fatal`] 决定：致命步骤（如解压）失败即中止安装，
/// 非致命步骤（注册类）失败仅记警告并继续——这保持了重构前的行为。
pub trait Step {
    /// 进度文案，如「正在注册 COM 组件...」。
    fn name(&self) -> String;

    fn run(&self, ctx: &mut StepCtx) -> Result<(), String>;

    /// 失败是否应中止整个安装流程。默认否（仅警告）。
    fn fatal(&self) -> bool {
        false
    }

    /// 本步骤是否要求重启才能完全生效——COM 注册失败时会置位。
    fn needs_reboot_on_failure(&self) -> bool {
        false
    }
}

/// 进度与日志出口。GUI / CLI 各自实现。
pub trait Reporter {
    /// 第 `index` 个步骤（共 `total` 个）开始执行。
    fn step_begin(&mut self, index: usize, total: usize, name: &str);
    /// 步骤内细粒度进度：`detail` 为当前子项，`fraction` ∈ [0,1] 为步骤内完成比例。
    fn step_progress(&mut self, detail: &str, fraction: f32);
    /// 普通日志，不影响进度。
    fn log(&mut self, msg: &str);
    /// 警告：非致命步骤失败。
    fn warn(&mut self, msg: &str);
}

/// 计划执行结果。
pub struct RunOutcome {
    pub need_reboot: bool,
}

/// 按序执行步骤计划。致命步骤失败立即返回 Err，非致命失败记警告后继续。
pub fn run_plan(
    plan: &[Box<dyn Step>],
    config: &InstallConfig,
    mode: InstallMode,
    is_fresh_install: bool,
    archive: &mut ArchiveReader,
    reporter: &mut dyn Reporter,
) -> Result<RunOutcome, String> {
    let total = plan.len();
    let mut need_reboot = false;

    for (i, step) in plan.iter().enumerate() {
        let name = step.name();
        reporter.step_begin(i, total, &name);

        let result = {
            let mut ctx = StepCtx {
                config,
                mode,
                is_fresh_install,
                archive: &mut *archive,
                reporter: &mut *reporter,
            };
            step.run(&mut ctx)
        };

        if let Err(e) = result {
            if step.fatal() {
                return Err(e);
            }
            if step.needs_reboot_on_failure() {
                need_reboot = true;
            }
            reporter.warn(&format!("{}: {}", name, e));
        }
    }

    Ok(RunOutcome { need_reboot })
}

/// 静默/CLI 路径的 Reporter：步骤名与警告写 stderr，细粒度进度丢弃。
pub struct CliReporter;

impl Reporter for CliReporter {
    fn step_begin(&mut self, index: usize, total: usize, name: &str) {
        eprintln!("[{}/{}] {}", index + 1, total, name);
    }
    fn step_progress(&mut self, _detail: &str, _fraction: f32) {}
    fn log(&mut self, msg: &str) {
        eprintln!("  {}", msg);
    }
    fn warn(&mut self, msg: &str) {
        eprintln!("Warning: {}", msg);
    }
}
