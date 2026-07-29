#![windows_subsystem = "windows"]

use std::path::PathBuf;

use clap::Parser;

mod archive;
mod installer;
mod manifest;
mod meta;
mod uninstaller;
mod ui;
mod util;

/// Wind Installer - 轻量级 Windows 安装管理器
#[derive(Parser, Debug)]
#[command(name = "wind-installer")]
#[command(about = "Lightweight Windows installer for WindInput")]
// 容忍未知参数：未来版本的安装器可能向已安装的旧版（卸载器）传入新 flag，
// 旧版不应因不认识的参数而报错退出，只解析自己认识的、忽略其余。
#[command(ignore_errors = true)]
struct Args {
    /// 运行模式
    #[command(subcommand)]
    mode: Option<Mode>,

    /// 完全静默安装（无任何界面）
    #[arg(long)]
    silent: bool,

    /// 带界面的静默安装：跳过配置页直接安装，显示进度，完成后自动退出。
    /// 应用内自动升级用这个 —— 用户需要看到进度，但不该被要求再确认一次安装路径。
    #[arg(long)]
    quiet: bool,

    /// 安装目录
    #[arg(long)]
    dir: Option<PathBuf>,

    /// 数据目录
    #[arg(long)]
    datadir: Option<PathBuf>,

    /// 保留用户数据（卸载时）
    #[arg(long)]
    keep_user_data: bool,

    /// 强制软渲染（禁用 Direct2D 硬件加速）；等效于设置环境变量 WIND_SOFT_RENDER=1
    #[arg(long)]
    soft_render: bool,
}

#[derive(Parser, Debug)]
enum Mode {
    /// 安装模式
    Install,
    /// 卸载模式
    Uninstall,
}

/// 操作成功，但有文件被占用、需重启系统才能清理干净。
///
/// 取值沿用 Windows 的 `ERROR_SUCCESS_REBOOT_REQUIRED`——MSI 与 NSIS 都用它表达
/// 「装成功了，但请重启」。仅用于完全无界面的 `--silent`：那条路径没有窗口可以
/// 留给用户看提示，退出码是唯一能把这个事实交给调用方的通道。
///
/// **调用方须知**：把 `3010` 当作成功而非失败处理，再自行提示用户重启。
const EXIT_REBOOT_REQUIRED: i32 = 3010;

#[cfg(test)]
mod arg_tests {
    use super::*;

    /// 应用内自动升级传入的正是这组参数；解析失败会静默退化成交互式向导
    /// （`ignore_errors = true` 让 clap 返回全默认值，silent 变回 false）。
    #[test]
    fn silent_install_with_quoted_dir() {
        let args = Args::parse_from([
            "wind-installer",
            "--silent",
            "--dir",
            r"C:\Program Files\WindInputDev",
        ]);
        assert!(args.silent, "--silent 未被识别");
        assert_eq!(
            args.dir,
            Some(PathBuf::from(r"C:\Program Files\WindInputDev"))
        );
    }

    #[test]
    fn silent_alone() {
        let args = Args::parse_from(["wind-installer", "--silent"]);
        assert!(args.silent, "--silent 单独传入也未被识别");
    }

    /// 未知参数应被忽略而不影响已知参数 —— 这是 ignore_errors 的本意。
    #[test]
    fn unknown_flag_does_not_swallow_known_ones() {
        let args = Args::parse_from(["wind-installer", "--silent", "--future-flag"]);
        assert!(args.silent, "未知参数把 --silent 一起吞掉了");
    }
}

fn main() {
    let args = Args::parse();

    // 检测运行模式
    let exe_name = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_stem().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or_default();

    match args.mode {
        Some(Mode::Install) | None => {
            if exe_name.contains("uninstall") {
                run_uninstall(args);
            } else {
                run_install(args);
            }
        }
        Some(Mode::Uninstall) => {
            run_uninstall(args);
        }
    }
}

/// 启动诊断：把解析到的参数与提权状态追加到 %TEMP%\wind_installer_args.log。
///
/// 安装器是 `windows_subsystem = "windows"` 的 GUI 进程，没有控制台，`eprintln!` 的
/// 输出无处可见。排查「为何没有静默安装」时，这个文件是唯一能看到真相的地方 ——
/// 尤其能暴露 `request_elevation` 重启自身后参数丢失：日志里会出现两条记录，
/// 第一条 `silent=true admin=false`，第二条 `silent=false admin=true`。
fn log_startup(tag: &str, args: &Args) {
    use std::io::Write;
    let line = format!(
        "{tag}: silent={} dir={:?} datadir={:?} admin={} argv={:?}\n",
        args.silent,
        args.dir,
        args.datadir,
        util::admin::is_admin(),
        std::env::args().collect::<Vec<_>>(),
    );
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(std::env::temp_dir().join("wind_installer_args.log"))
        .and_then(|mut f| f.write_all(line.as_bytes()));
}

/// 记录「以 3010 退出」的原因。
///
/// 静默模式退出码非 0 时，调用方唯一能查的就是这个文件；不落盘的话，
/// 用户只会看到「安装器返回了 3010」而无从知道是哪些文件卡住了。
fn log_reboot_required(tag: &str) {
    use std::io::Write;
    let line = format!(
        "{tag}: exit={EXIT_REBOOT_REQUIRED} reboot_required {}\n",
        util::reboot::pending_summary()
    );
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(std::env::temp_dir().join("wind_installer_args.log"))
        .and_then(|mut f| f.write_all(line.as_bytes()));
}

/// 运行安装
fn run_install(args: Args) {
    log_startup("install", &args);
    // 载入清单（来自自身追加的归档）
    if let Err(e) = meta::bootstrap() {
        eprintln!("无法载入安装清单: {}", e);
        std::process::exit(1);
    }

    // 检查单实例
    if util::single::is_another_instance_running() {
        std::process::exit(0);
    }

    // 检查管理员权限
    if !util::admin::is_admin() {
        util::admin::request_elevation().ok();
        std::process::exit(0);
    }

    // 检查 64 位系统
    if !util::path::is_64bit_system() {
        // TODO: 显示错误对话框
        std::process::exit(1);
    }

    if args.silent {
        let mut config = installer::config::InstallConfig::default();
        if let Some(dir) = args.dir {
            config.install_dir = dir;
        }
        if let Some(datadir) = args.datadir {
            config.custom_data_dir = Some(datadir);
            config.use_custom_data_dir = true;
        }

        let result = installer::perform_install(&config, installer::InstallMode::Standard);
        if !result.success {
            std::process::exit(1);
        }
        if result.need_reboot {
            // 无界面模式没有「让用户看到提示再关闭」的余地，只能靠退出码传信。
            log_reboot_required("install");
            util::single::release_lock();
            std::process::exit(EXIT_REBOOT_REQUIRED);
        }
    } else {
        ui::install_wizard::run_install_wizard(ui::install_wizard::WizardOptions {
            quiet: args.quiet,
            install_dir: args.dir,
        });
    }

    util::single::release_lock();
}

/// 运行卸载
fn run_uninstall(args: Args) {
    if util::single::is_another_instance_running() {
        std::process::exit(0);
    }

    if !util::admin::is_admin() {
        util::admin::request_elevation().ok();
        std::process::exit(0);
    }

    if uninstaller::selfdelete::is_self_delete_mode() {
        if let Some(dir) = uninstaller::selfdelete::self_delete_target() {
            uninstaller::selfdelete::execute_self_delete(&dir);
        }
        return;
    }

    // 载入清单（来自安装目录的 .manifest）
    if let Err(e) = meta::bootstrap() {
        eprintln!("无法载入卸载清单: {}", e);
        std::process::exit(1);
    }

    if args.silent {
        let mut options = uninstaller::cleanup::CleanupOptions::default();
        options.keep_user_data = args.keep_user_data;

        let result = uninstaller::perform_uninstall(&options);
        if !result.success {
            std::process::exit(1);
        }
        if result.need_reboot {
            log_reboot_required("uninstall");
            util::single::release_lock();
            std::process::exit(EXIT_REBOOT_REQUIRED);
        }
    } else {
        ui::uninstall_wizard::run_uninstall_wizard();
    }

    util::single::release_lock();
}
