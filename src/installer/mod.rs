pub mod config;
#[allow(dead_code)]
pub mod extract;
pub mod legacy;
pub mod registry;
pub mod userdata;
pub mod shortcut;
pub mod font;
pub mod acl;
pub mod process;
pub mod ime;

use config::InstallConfig;
use crate::archive::ArchiveReader;
use crate::meta;

/// 安装模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallMode {
    /// 标准安装 - 注册到系统
    Standard,
    /// 便携模式 - 仅解压文件
    Portable,
}

/// 安装结果
#[derive(Debug)]
#[allow(dead_code)]
pub struct InstallResult {
    pub success: bool,
    pub message: String,
    pub need_reboot: bool,
}

/// 执行完整安装流程
pub fn perform_install(config: &InstallConfig, mode: InstallMode) -> InstallResult {
    let mut need_reboot = false;

    // 标准模式：先写入 InstallerRunning 标志，防止 wind_tsf.dll 在安装期间重拉 wind_input.exe
    if mode == InstallMode::Standard {
        let _ = registry::set_installer_running();
    }

    // 1. 停止旧进程
    if mode == InstallMode::Standard {
        if let Err(e) = process::terminate_windinput_processes() {
            eprintln!("Warning: Failed to stop processes: {}", e);
        }
    }

    // 2. 反注册旧 COM（标准模式，仅当清单含 ime 段）
    if mode == InstallMode::Standard && meta::manifest().ime.is_some() {
        if let Err(e) = ime::unregister_old_com(&config.install_dir) {
            eprintln!("Warning: Failed to unregister old COM: {}", e);
        }
    }

    // 3. 清理旧版遗留文件（升级场景，新版已移除的条目）
    legacy::cleanup_legacy(&config.install_dir);

    // 4. 释放文件
    match ArchiveReader::open_current_exe() {
        Ok(mut archive) => {
            if let Err(e) = extract::extract_files(&mut archive, &config.install_dir) {
                let _ = registry::clear_installer_running();
                return InstallResult {
                    success: false,
                    message: format!("Failed to extract files: {}", e),
                    need_reboot,
                };
            }
            // 持久化清单 + logo 到安装目录，供卸载器（无附加归档的裸 stub）启动时读取
            if mode == InstallMode::Standard {
                let manifest_bytes = archive.manifest_bytes();
                if !manifest_bytes.is_empty() {
                    let _ = std::fs::write(
                        config.install_dir.join(meta::MANIFEST_FILE),
                        manifest_bytes,
                    );
                }
                let logo_bytes = archive.logo_bytes();
                if !logo_bytes.is_empty() {
                    let _ = std::fs::write(config.install_dir.join(meta::LOGO_FILE), logo_bytes);
                }
            }
        }
        Err(e) => {
            let _ = registry::clear_installer_running();
            return InstallResult {
                success: false,
                message: format!("Failed to open archive: {}", e),
                need_reboot,
            };
        }
    }

    // 5. 标准模式特有步骤
    if mode == InstallMode::Standard {
        // 设置 DLL 权限
        if let Err(e) = acl::set_dll_permissions(&config.install_dir) {
            eprintln!("Warning: Failed to set DLL permissions: {}", e);
        }

        // 安装字体（仅当清单含 font 段）
        if !meta::manifest().font.is_empty() {
            if let Err(e) = font::install_font(&config.install_dir) {
                eprintln!("Warning: Failed to install font: {}", e);
            }
        }

        // 注册输入法（仅当清单含 ime 段）
        if meta::manifest().ime.is_some() {
            // 注册 COM
            if let Err(e) = ime::register_com(&config.install_dir) {
                eprintln!("Warning: Failed to register COM: {}", e);
                need_reboot = true;
            }
            // 注册系统输入法
            if let Err(e) = ime::register_input_method() {
                eprintln!("Warning: Failed to register input method: {}", e);
            }
        }

        // 配置自启动（输入法必须自启）
        if let Err(e) = registry::set_auto_start(&config.install_dir) {
            eprintln!("Warning: Failed to set auto-start: {}", e);
        }

        // 注册 URL 协议（留空则跳过）
        if !meta::url_protocol().is_empty() {
            if let Err(e) = registry::register_url_protocol(&config.install_dir) {
                eprintln!("Warning: Failed to register URL protocol: {}", e);
            }
        }

        // 创建快捷方式
        if let Err(e) = shortcut::create_shortcuts(&config.install_dir) {
            eprintln!("Warning: Failed to create shortcuts: {}", e);
        }

        // 写入卸载信息
        if let Err(e) = registry::write_uninstall_info(config) {
            eprintln!("Warning: Failed to write uninstall info: {}", e);
        }

        // 预启动服务
        if let Err(e) = process::prestart_service(&config.install_dir) {
            eprintln!("Warning: Failed to prestart service: {}", e);
        }
    }

    // 6. 便携模式标记
    if mode == InstallMode::Portable {
        if let Err(e) = std::fs::write(config.install_dir.join(meta::portable_marker()), "portable=1\n") {
            eprintln!("Warning: Failed to create portable mode marker: {}", e);
        }
    }

    // 安装完成，清除 InstallerRunning 标志，允许 wind_tsf.dll 正常管理服务
    if mode == InstallMode::Standard {
        let _ = registry::clear_installer_running();
    }

    InstallResult {
        success: true,
        message: "Installation completed successfully".into(),
        need_reboot,
    }
}
