pub mod cleanup;
pub mod selfdelete;

use cleanup::CleanupOptions;

/// 卸载结果
#[derive(Debug)]
#[allow(dead_code)]
pub struct UninstallResult {
    pub success: bool,
    pub message: String,
    pub need_reboot: bool,
}

/// 执行完整卸载流程
pub fn perform_uninstall(options: &CleanupOptions) -> UninstallResult {
    let mut need_reboot = false;

    // 1. 设置安装器运行标记
    if let Err(e) = crate::installer::registry::set_installer_running() {
        eprintln!("Warning: {}", e);
    }

    // 2. 停止进程
    if let Err(e) = crate::installer::process::terminate_windinput_processes() {
        eprintln!("Warning: Failed to stop processes: {}", e);
    }

    // 3. 反注册输入法
    if let Err(e) = crate::installer::ime::unregister_input_method() {
        eprintln!("Warning: Failed to unregister input method: {}", e);
    }

    // 4. 反注册 COM
    if let Err(e) = crate::installer::ime::unregister_old_com(&options.install_dir) {
        eprintln!("Warning: Failed to unregister COM: {}", e);
    }

    // 5. 卸载字体
    if let Err(e) = crate::installer::font::uninstall_font() {
        eprintln!("Warning: Failed to uninstall font: {}", e);
    }

    // 6. 删除安装文件
    if let Err(e) = cleanup::delete_install_files(&options.install_dir) {
        eprintln!("Warning: Failed to delete some files: {}", e);
        need_reboot = true;
    }

    // 7. 删除快捷方式
    if let Err(e) = crate::installer::shortcut::delete_shortcuts() {
        eprintln!("Warning: Failed to delete shortcuts: {}", e);
    }

    // 8. 清理注册表
    cleanup::cleanup_registry();

    // 9. 清理用户数据
    if let Err(e) = cleanup::cleanup_user_data(options) {
        eprintln!("Warning: Failed to cleanup user data: {}", e);
    }

    // 10. 清除安装器运行标记
    let _ = crate::installer::registry::clear_installer_running();

    UninstallResult {
        success: true,
        message: "Uninstallation completed".into(),
        need_reboot,
    }
}
