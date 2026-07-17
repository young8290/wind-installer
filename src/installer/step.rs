//! 步骤抽象 —— 编排的唯一真相，安装与卸载共用。
//!
//! 历史上 GUI 路径与静默路径各自手写了一遍步骤链（安装、卸载各一对），已漂移出缺陷：
//! 静默安装漏写 datadir.conf，GUI 安装不设 InstallerRunning 标志。现在四条路径共用
//! [`run_plan`]，差异收敛到两处：
//! - **装什么** → `plan::plan_install` / `uninstaller::plan::plan_uninstall`
//! - **怎么汇报** → [`Reporter`]（GUI 传 channel 实现，CLI 传 stderr 实现）
//!
//! [`Step`] 对上下文泛型：安装用 [`InstallCtx`]，卸载用 `uninstaller::steps::UninstallCtx`。
//! 两者的执行语义（失败是否致命、进度如何汇报）完全一致，故 [`run_plan`] 只需一份。

use crate::archive::ArchiveReader;

use super::config::InstallConfig;
use super::receipt::Receipt;
use super::InstallMode;

/// 安装步骤的上下文。
pub struct InstallCtx<'a> {
    pub config: &'a InstallConfig,
    pub mode: InstallMode,
    /// 是否首次安装（非升级）——决定是否写入用户数据目录配置。
    pub is_fresh_install: bool,
    /// 自身归档，供解压与卸载器 overlay 追加使用。
    pub archive: &'a mut ArchiveReader,
    /// 步骤把做成的产物记在这里，卸载时反向回放。
    pub receipt: &'a mut Receipt,
}

/// 一个安装/卸载动作。
///
/// 失败语义由 [`Step::fatal`] 决定：致命步骤（如解压）失败即中止，
/// 非致命步骤（注册类）失败仅记警告并继续。
pub trait Step<C> {
    /// 进度文案，如「正在注册 COM 组件...」。
    fn name(&self) -> String;

    fn run(&self, ctx: &mut C, reporter: &mut dyn Reporter) -> Result<(), String>;

    /// 失败是否应中止整个流程。默认否（仅警告）。
    fn fatal(&self) -> bool {
        false
    }

    /// 本步骤失败是否意味着需要重启才能完全生效。
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
pub fn run_plan<C>(
    plan: &[Box<dyn Step<C>>],
    ctx: &mut C,
    reporter: &mut dyn Reporter,
) -> Result<RunOutcome, String> {
    let total = plan.len();
    let mut need_reboot = false;

    for (i, step) in plan.iter().enumerate() {
        let name = step.name();
        reporter.step_begin(i, total, &name);

        if let Err(e) = step.run(ctx, reporter) {
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
