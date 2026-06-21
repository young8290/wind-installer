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
struct Args {
    /// 运行模式
    #[command(subcommand)]
    mode: Option<Mode>,

    /// 静默安装（跳过 GUI）
    #[arg(long)]
    silent: bool,

    /// 安装目录
    #[arg(long)]
    dir: Option<PathBuf>,

    /// 数据目录
    #[arg(long)]
    datadir: Option<PathBuf>,

    /// 保留用户数据（卸载时）
    #[arg(long)]
    keep_user_data: bool,
}

#[derive(Parser, Debug)]
enum Mode {
    /// 安装模式
    Install,
    /// 卸载模式
    Uninstall,
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

/// 运行安装
fn run_install(args: Args) {
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
    } else {
        ui::install_wizard::run_install_wizard();
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
    } else {
        ui::uninstall_wizard::run_uninstall_wizard();
    }

    util::single::release_lock();
}
