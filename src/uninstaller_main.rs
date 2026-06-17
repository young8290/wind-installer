#![windows_subsystem = "windows"]
#![allow(dead_code, unused_imports)]

mod archive;
mod installer;
mod meta;
mod uninstaller;
mod ui;
mod util;

fn main() {
    // 自删除模式：由临时副本调用，不需要 UI，直接清理后退出
    if uninstaller::selfdelete::is_self_delete_mode() {
        if let Some(dir) = uninstaller::selfdelete::self_delete_target() {
            uninstaller::selfdelete::execute_self_delete(&dir);
        }
        return;
    }

    if util::single::is_another_instance_running() {
        std::process::exit(0);
    }

    if !util::admin::is_admin() {
        util::admin::request_elevation().ok();
        std::process::exit(0);
    }

    ui::uninstall_wizard::run_uninstall_wizard();
    util::single::release_lock();
}
