#![windows_subsystem = "windows"]
#![allow(dead_code, unused_imports)]

mod archive;
mod installer;
mod manifest;
mod meta;
mod ui;
mod uninstaller;
mod util;

fn main() {
    // 自删除模式：由临时副本调用，不需要 UI 也不访问清单，直接清理后退出
    if uninstaller::selfdelete::is_self_delete_mode() {
        if let Some(dir) = uninstaller::selfdelete::self_delete_target() {
            uninstaller::selfdelete::execute_self_delete(&dir);
        }
        return;
    }

    if !util::admin::is_admin() {
        util::admin::request_elevation().ok();
        std::process::exit(0);
    }

    // 载入清单（来自安装目录的 .manifest，位于卸载器自身旁）
    if let Err(e) = meta::bootstrap() {
        eprintln!("无法载入卸载清单: {}", e);
        std::process::exit(1);
    }

    // 检查单实例。必须排在 bootstrap 之后：被挡住时要报出应用名，清单没载入就会 panic
    // ——而 UninstallString 指的正是本程序，那会让「控制面板点卸载」变成一次无提示崩溃。
    //
    // `--silent` 在这里单独认一次：本程序整体还不解析参数（QuietUninstallString 里的
    // --silent 从来没生效过，是个已知的相邻缺陷），但「被锁挡住」这条路是新加的，
    // 不该让它对着一个明确要求静默的调用方弹模态框——那会把批量卸载挂在那里等人点。
    let silent = std::env::args().any(|a| a == "--silent");
    if let Some(pid) = util::single::another_instance_pid() {
        util::single::report_busy_and_exit(pid, silent);
    }

    ui::uninstall_wizard::run_uninstall_wizard();
    util::single::release_lock();
}
