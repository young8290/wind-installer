#![windows_subsystem = "windows"]
#![allow(dead_code, unused_imports)]

mod archive;
mod installer;
mod manifest;
mod meta;
mod ui;
mod uninstaller;
mod util;

use util::exitcode;

fn main() {
    // 自删除模式：由临时副本调用，不需要 UI 也不访问清单，直接清理后退出
    if uninstaller::selfdelete::is_self_delete_mode() {
        if let Some(dir) = uninstaller::selfdelete::self_delete_target() {
            uninstaller::selfdelete::execute_self_delete(&dir);
        }
        return;
    }

    // 参数解析必须排在最前（自删除那条除外——它有自己的一套 flag，且跑在临时副本里）：
    // 下面每一步的收场方式都取决于「这次是不是静默调用」。
    let args = uninstaller::args::from_env();
    log_startup(&args);

    if !util::admin::is_admin() {
        uninstaller::require_admin_or_exit(args.silent);
    }

    // 载入清单（来自安装目录的 .manifest，位于卸载器自身旁）
    if let Err(e) = meta::bootstrap() {
        eprintln!("无法载入卸载清单: {}", e);
        std::process::exit(exitcode::FAILURE);
    }

    // 检查单实例。必须排在 bootstrap 之后：被挡住时要报出应用名，清单没载入就会 panic
    // ——而 UninstallString 指的正是本程序，那会让「控制面板点卸载」变成一次无提示崩溃。
    if let Some(pid) = util::single::another_instance_pid() {
        util::single::report_busy_and_exit(pid, args.silent);
    }

    if args.silent {
        let code = uninstaller::run_silent(args.keep_user_data);
        util::single::release_lock();
        std::process::exit(code);
    }

    ui::uninstall_wizard::run_uninstall_wizard();
    util::single::release_lock();
}

/// 启动决策日志，追加到 `%TEMP%\wind_installer_args.log`。
///
/// 与安装器 `main.rs` 的 `log_startup` 是同一件事、同一个文件。卸载器此前**一行都不写**：
/// 它是 `windows_subsystem = "windows"` 的无控制台进程，`eprintln!` 等于丢弃；而
/// 「为什么点了静默卸载还是弹出向导」这种问题发生在向导起来之前，那时 `RunLogger`
/// 都还没建，出了事查无可查 —— 那个缺陷正是这么在 ARP 里躺了很久没人发现的。
///
/// 记 `argv` 而不只记解析结果：调用方传了什么、解析成了什么，两边都在，才判得出
/// 到底是调用方写错了参数，还是这边没认出来。
fn log_startup(args: &uninstaller::args::UninstallArgs) {
    util::log::append_startup_line(&format!(
        "uninstall-start: silent={} keep_user_data={} admin={} argv={:?}\n",
        args.silent,
        args.keep_user_data,
        util::admin::is_admin(),
        std::env::args_os().collect::<Vec<_>>(),
    ));
}
