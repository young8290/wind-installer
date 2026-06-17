#![windows_subsystem = "windows"]

use std::path::PathBuf;

use clap::Parser;

mod archive;
mod installer;
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
    /// 打包模式（独立工具）
    Pack {
        /// 源目录
        #[arg(short, long)]
        source: PathBuf,

        /// 输出文件
        #[arg(short, long)]
        output: PathBuf,

        /// 压缩算法 (zstd/lzma)
        #[arg(short, long, default_value = "zstd")]
        compression: String,
    },
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
        Some(Mode::Pack { source, output, compression }) => {
            run_pack(source, output, compression);
        }
    }
}

/// 运行安装
fn run_install(args: Args) {
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
        let args: Vec<String> = std::env::args().collect();
        if let Some(original_exe) = args.get(1) {
            let original_path = PathBuf::from(original_exe);
            let _ = uninstaller::selfdelete::execute_self_delete(&original_path);
        }
        return;
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

/// 运行打包
fn run_pack(source: PathBuf, output: PathBuf, compression: String) {
    use archive::{ArchiveWriter, CompressionType};

    let compression_type = match compression.to_lowercase().as_str() {
        "lzma" => CompressionType::Lzma,
        _ => CompressionType::Zstd,
    };

    let mut writer = match ArchiveWriter::new(&output, compression_type) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("Failed to create archive writer: {}", e);
            std::process::exit(1);
        }
    };

    if let Err(e) = writer.add_directory(&source, "") {
        eprintln!("Failed to add directory: {}", e);
        std::process::exit(1);
    }

    let entry_count = writer.entry_count();

    match writer.finish() {
        Ok(header_offset) => {
            println!("Archive created: {} entries, header at {}", entry_count, header_offset);
        }
        Err(e) => {
            eprintln!("Failed to finish archive: {}", e);
            std::process::exit(1);
        }
    }
}
