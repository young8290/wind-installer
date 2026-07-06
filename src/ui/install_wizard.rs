use std::path::PathBuf;

use windui::app::App;
use windui::core::EventCtx;
use windui::geometry::Color;
use windui::platform::PickDialog;
use windui::signal::signal;
use windui::spec::Align;
use windui::ui::{Element, WindowButtonKind};

use crate::installer::InstallMode;
use crate::meta;
use super::theme;

const PAGE_CONFIG: usize = 0;
const PAGE_PROGRESS: usize = 1;
const PAGE_FINISH: usize = 2;

enum ProgressMsg {
    Status(String),
    Total(usize),
    Done(usize),
    Finished(bool, String),
}

pub fn run_install_wizard() {
    // ---- 运行期窗口尺寸 / 标题（来自清单）----
    let (win_w, win_h) = meta::install_win();
    let title = meta::window_title();

    // ---- 状态（Signal<T> 是 Copy 句柄，move 闭包自动复制，无需 clone 样板）----
    let is_fresh_install = crate::installer::registry::detect_installed_version().is_none();
    let current_page      = signal(PAGE_CONFIG);
    let install_mode      = signal(0usize);
    let install_dir       = signal(default_install_dir());
    let install_dir_portable = signal(default_portable_dir());
    let data_dir          = signal(default_data_dir());
    let use_custom_data_dir = signal(false);
    let progress_text     = signal(String::from("正在准备安装..."));
    let progress_value    = signal(0.0f32);
    let progress_total    = signal(1usize);
    let finish_success    = signal(false);
    let finish_error      = signal(String::new());
    let config_error      = signal(String::new());
    let agreed            = signal(false);

    // ---- 跨线程进度通道（on_message 在 UI 线程调用，可直接写 Signal）----
    let mut app = App::new(title.as_str(), win_w, win_h);
    let tx = app.channel::<ProgressMsg>(move |msg| match msg {
        ProgressMsg::Status(s) => progress_text.set(s),
        ProgressMsg::Total(t)  => progress_total.set(t.max(1)),
        ProgressMsg::Done(n)   => {
            let total = progress_total.get();
            progress_value.set((n as f32 / total as f32).min(0.99));
        }
        ProgressMsg::Finished(ok, detail) => {
            progress_value.set(1.0);
            finish_success.set(ok);
            if !ok {
                finish_error.set(detail);
            }
            current_page.set(PAGE_FINISH);
        }
    });

    // ============================================================
    //  PAGE 0：配置页
    // ============================================================
    let page_config = Element::col()
        .fill()
        .weight(1.0)
        .padding_xy(40, 0)
        .spacing(8)
        .visible_when(move || current_page.get() == PAGE_CONFIG)
        // ── 品牌区（Logo + 应用名 + 版本）─────────────────────────
        .child(Element::leaf().weight(1.0))
        .child(
            Element::col()
                .width_match()
                .spacing(6)
                .cross(Align::Center)
                .child(
                    Element::image_bytes(meta::logo())
                        .size(72, 72)
                        .corner(18.0),
                )
                .child(
                    Element::label(meta::app_display_name())
                        .width_match()
                        .font_size(22.0)
                        .fg(Color::hex(theme::TEXT_PRIMARY))
                        .text_align(Align::Center),
                )
                .child(
                    Element::label(meta::app_version())
                        .width_match()
                        .font_size(12.0)
                        .fg(Color::hex(theme::TEXT_MUTED))
                        .text_align(Align::Center),
                ),
        )
        .child(Element::leaf().height(10))
        // 安装目录（行高固定 34，内容随模式切换，避免布局抖动）
        .child(
            Element::row()
                .width_match()
                .height(34)
                .spacing(8)
                .cross(Align::Center)
                .child(
                    Element::label("安装目录")
                        .width(64)
                        .font_size(12.0)
                        .fg(Color::hex(theme::TEXT_SECONDARY))
                )
                .child(
                    Element::text_input(install_dir, "安装路径")
                        .weight(1.0)
                        .height(34)
                        .visible_when(move || install_mode.get() == 0)
                )
                .child(
                    Element::button("更改")
                        .height(34)
                        .visible_when(move || install_mode.get() == 0)
                        .on_click(move |ctx: &mut EventCtx| {
                            ctx.request_pick_folder(
                                folder_pick_dialog("选择安装目录", &install_dir.get()),
                                move |path| {
                                    if let Some(path) = path {
                                        install_dir.set(
                                            finalize_picked_folder(path)
                                                .to_string_lossy()
                                                .to_string(),
                                        );
                                    }
                                },
                            );
                        })
                )
                .child(
                    Element::text_input(install_dir_portable, "安装路径")
                        .weight(1.0)
                        .height(34)
                        .visible_when(move || install_mode.get() == 1)
                )
                .child(
                    Element::button("更改")
                        .height(34)
                        .visible_when(move || install_mode.get() == 1)
                        .on_click(move |ctx: &mut EventCtx| {
                            ctx.request_pick_folder(
                                folder_pick_dialog("选择安装目录", &install_dir_portable.get()),
                                move |path| {
                                    if let Some(path) = path {
                                        install_dir_portable.set(
                                            finalize_picked_folder(path)
                                                .to_string_lossy()
                                                .to_string(),
                                        );
                                    }
                                },
                            );
                        })
                )
        )
        // 数据目录（行高固定 34；便捷模式显示说明文字）
        .child(
            Element::row()
                .width_match()
                .height(34)
                .spacing(8)
                .cross(Align::Center)
                .child(
                    Element::label("数据目录")
                        .width(64)
                        .font_size(12.0)
                        .fg(Color::hex(theme::TEXT_SECONDARY))
                )
                .child(
                    Element::text_input(data_dir, "词库、配置路径")
                        .weight(1.0)
                        .height(34)
                        .enabled(signal(is_fresh_install))
                        .visible_when(move || install_mode.get() == 0)
                )
                .child(
                    Element::button("更改")
                        .height(34)
                        .enabled(signal(is_fresh_install))
                        .visible_when(move || install_mode.get() == 0)
                        .on_click(move |ctx: &mut EventCtx| {
                            ctx.request_pick_folder(
                                folder_pick_dialog("选择数据目录", &data_dir.get()),
                                move |path| {
                                    if let Some(path) = path {
                                        data_dir.set(
                                            finalize_picked_folder(path)
                                                .to_string_lossy()
                                                .to_string(),
                                        );
                                    }
                                },
                            );
                        })
                )
                .child(
                    Element::label("便捷模式不配置数据目录")
                        .weight(1.0)
                        .font_size(12.0)
                        .fg(Color::hex(theme::TEXT_MUTED))
                        .visible_when(move || install_mode.get() == 1)
                )
        )
        // 安装模式
        .child(
            Element::col()
                .width_match()
                .spacing(5)
                .child(
                    Element::row()
                        .width_match()
                        .spacing(16)
                        .child(Element::radio("标准安装（推荐）", install_mode, 0))
                        .child(Element::radio("便捷模式", install_mode, 1))
                )
                .child(
                    Element::label("标准安装注册输入法到系统；便捷模式仅解压文件，不修改系统")
                        .font_size(11.0)
                        .fg(Color::hex(theme::TEXT_MUTED))
                        .width_match()
                )
        )
        // 用户协议
        .child(
            Element::row()
                .cross(Align::Center)
                .spacing(4)
                .child(Element::checkbox("我已阅读并同意", agreed))
                .child(if meta::agreement_url().is_empty() {
                    Element::label("《用户服务协议》")
                        .font_size(13.0)
                        .fg(Color::hex(theme::TEXT_SECONDARY))
                } else {
                    Element::link("《用户服务协议》")
                        .url(meta::agreement_url())
                })
        )
        // 校验错误提示（仅未勾协议时可见）
        .child(
            Element::label_rc(config_error)
                .font_size(11.0)
                .fg(Color::hex(theme::ERROR))
                .align(Align::Center)
        )
        // 安装按钮（align=Center 使其在父 col 中水平居中）
        .child(
            Element::button("立即安装")
                .width(300)
                .height(48)
                .corner(8.0)
                .bg(Color::hex(theme::ACCENT))
                .fg(Color::hex(0xFFFFFF))
                .align(Align::Center)
                .enabled(agreed)
                .on_click(move |_ctx: &mut EventCtx| {
                    config_error.set(String::new());
                    progress_value.set(0.0);
                    current_page.set(PAGE_PROGRESS);

                    let install_dir_val = if install_mode.get() == 0 {
                        expand_env_path(&install_dir.get())
                    } else {
                        expand_env_path(&install_dir_portable.get())
                    };
                    let data_dir_val   = expand_env_path(&data_dir.get());
                    let use_custom     = use_custom_data_dir.get();
                    let mode           = if install_mode.get() == 0 {
                        InstallMode::Standard
                    } else {
                        InstallMode::Portable
                    };

                    let tx = tx.clone();
                    std::thread::spawn(move || {
                        let mut log = crate::util::log::InstallLogger::new();
                        log.log(&format!("安装目录: {:?}", install_dir_val));
                        log.log(&format!("安装模式: {:?}", mode));

                        macro_rules! step {
                            ($msg:expr) => {{
                                log.log($msg);
                                tx.send(ProgressMsg::Status($msg.into())).ok();
                            }};
                        }

                        let mut config = crate::installer::config::InstallConfig::default();
                        config.install_dir = install_dir_val;
                        if use_custom {
                            config.custom_data_dir = Some(data_dir_val.clone());
                            config.use_custom_data_dir = true;
                        }

                        if mode == InstallMode::Standard {
                            step!("正在停止旧进程...");
                            if let Err(e) = crate::installer::process::terminate_windinput_processes() {
                                log.log(&format!("  警告: {}", e));
                            }
                        }

                        if mode == InstallMode::Standard && crate::meta::manifest().ime.is_some() {
                            step!("正在反注册旧 COM...");
                            if let Err(e) = crate::installer::ime::unregister_old_com(&config.install_dir) {
                                log.log(&format!("  警告: {}", e));
                            }
                        }

                        if mode == InstallMode::Standard {
                            step!("正在清理旧版遗留文件...");
                            crate::installer::legacy::cleanup_legacy(&config.install_dir);
                        }

                        step!("正在读取安装数据...");
                        let archive = match crate::archive::ArchiveReader::open_current_exe() {
                            Ok(a) => a,
                            Err(e) => {
                                let msg = format!("无法打开安装数据: {}", e);
                                log.log_error(&msg);
                                tx.send(ProgressMsg::Finished(false, msg)).ok();
                                return;
                            }
                        };

                        let entries: Vec<crate::archive::format::ArchiveEntry> =
                            archive.entries().to_vec();
                        let total = entries.len();
                        log.log(&format!("归档共 {} 个文件", total));
                        let _ = tx.send(ProgressMsg::Total(total));

                        let exe_path = std::env::current_exe().unwrap();
                        let mut thread_archive = match crate::archive::ArchiveReader::open(&exe_path) {
                            Ok(a) => a,
                            Err(e) => {
                                let msg = format!("无法读取安装数据: {}", e);
                                log.log_error(&msg);
                                tx.send(ProgressMsg::Finished(false, msg)).ok();
                                return;
                            }
                        };

                        let _ = std::fs::create_dir_all(&config.install_dir);

                        step!("正在解压数据...");
                        if let Err(e) = thread_archive.prepare() {
                            let msg = format!("解压失败: {}", e);
                            log.log_error(&msg);
                            tx.send(ProgressMsg::Finished(false, msg)).ok();
                            return;
                        }

                        step!("正在释放文件...");
                        for (i, entry) in entries.iter().enumerate() {
                            let dest = config.install_dir.join(&entry.path);
                            log.log(&format!("  解压: {}", entry.path));
                            let _ = tx.send(ProgressMsg::Status(
                                format!("正在安装 {}", entry.path)
                            ));
                            if let Err(e) = thread_archive.extract_entry(entry, &dest) {
                                let msg = format!("解压失败 {}: {}", entry.path, e);
                                log.log_error(&msg);
                                tx.send(ProgressMsg::Finished(false, msg)).ok();
                                return;
                            }
                            let _ = tx.send(ProgressMsg::Done(i + 1));
                        }

                        if mode == InstallMode::Standard {
                            // 给卸载器追加清单 overlay，使其自包含——安装目录不留散落文件
                            let uninstaller = config.install_dir.join("uninstall.exe");
                            if uninstaller.exists() {
                                if let Err(e) = crate::archive::append_manifest_overlay(
                                    &uninstaller,
                                    thread_archive.manifest_bytes(),
                                    thread_archive.logo_bytes(),
                                ) {
                                    log.log(&format!("  警告: 写入卸载器清单失败: {}", e));
                                }
                            }
                            step!("正在设置文件权限...");
                            if let Err(e) = crate::installer::acl::set_dll_permissions(&config.install_dir) {
                                log.log(&format!("  警告: {}", e));
                            }
                            if !crate::meta::manifest().font.is_empty() {
                                step!("正在安装字体...");
                                if let Err(e) = crate::installer::font::install_font(&config.install_dir) {
                                    log.log(&format!("  警告: {}", e));
                                }
                            }
                            if crate::meta::manifest().ime.is_some() {
                                step!("正在注册 COM 组件...");
                                if let Err(e) = crate::installer::ime::register_com(&config.install_dir) {
                                    log.log(&format!("  警告: {}", e));
                                }
                                step!("正在注册系统输入法...");
                                if let Err(e) = crate::installer::ime::register_input_method() {
                                    log.log(&format!("  警告: {}", e));
                                }
                            }
                            step!("正在配置开机自启动...");
                            if let Err(e) = crate::installer::registry::set_auto_start(&config.install_dir) {
                                log.log(&format!("  警告: {}", e));
                            }
                            if !crate::meta::url_protocol().is_empty() {
                                step!("正在注册协议...");
                                if let Err(e) = crate::installer::registry::register_url_protocol(&config.install_dir) {
                                    log.log(&format!("  警告: {}", e));
                                }
                            }
                            step!("正在创建快捷方式...");
                            if let Err(e) = crate::installer::shortcut::create_shortcuts(&config.install_dir) {
                                log.log(&format!("  警告: {}", e));
                            }
                            step!("正在写入卸载信息...");
                            if let Err(e) = crate::installer::registry::write_uninstall_info(&config) {
                                log.log(&format!("  警告: {}", e));
                            }
                            // 首次安装：写入用户数据目录配置
                            if is_fresh_install {
                                if let Err(e) = crate::installer::userdata::write_datadir_conf(&data_dir_val) {
                                    log.log(&format!("  警告: {}", e));
                                }
                            }
                            step!("正在启动服务...");
                            if let Err(e) = crate::installer::process::prestart_service(&config.install_dir) {
                                log.log(&format!("  警告: {}", e));
                            }
                        } else {
                            let _ = std::fs::write(
                                config.install_dir.join(crate::meta::portable_marker()),
                                "portable=1\n",
                            );
                        }

                        log.log("=== 安装完成 ===");
                        let log_path = log.path.to_string_lossy().to_string();
                        tx.send(ProgressMsg::Finished(true, log_path)).ok();
                    });
                })
        )
        .child(Element::leaf().weight(1.0));

    // ============================================================
    //  PAGE 1：进度页
    // ============================================================
    let page_progress = Element::col()
        .fill()
        .weight(1.0)
        .padding_xy(56, 0)
        .visible_when(move || current_page.get() == PAGE_PROGRESS)
        .child(Element::leaf().weight(1.0))
        .child(
            Element::col()
                .width_match()
                .spacing(10)
                .cross(Align::Center)
                .child(
                    Element::label("正在安装中，请稍候...")
                        .font_size(14.0)
                        .fg(Color::hex(theme::TEXT_PRIMARY))
                )
                .child(
                    Element::progress(progress_value)
                        .width_match()
                        .height(6)
                        .corner(3.0)
                )
                .child(
                    Element::label_rc(progress_text)
                        .font_size(12.0)
                        .fg(Color::hex(theme::TEXT_SECONDARY))
                        .width_match()
                        .text_align(Align::Center)
                )
        )
        .child(Element::leaf().weight(1.0));

    // ============================================================
    //  PAGE 2：完成页
    // ============================================================
    let page_finish = Element::col()
        .fill()
        .weight(1.0)
        .padding_xy(48, 0)
        .visible_when(move || current_page.get() == PAGE_FINISH)
        .child(Element::leaf().weight(1.0))
        // 安装成功
        .child(
            Element::col()
                .width_match()
                .spacing(8)
                .cross(Align::Center)
                .visible_when(move || finish_success.get())
                .child(
                    Element::label("✓")
                        .font_size(44.0)
                        .fg(Color::hex(theme::SUCCESS))
                )
                .child(
                    Element::label("安装完成")
                        .font_size(18.0)
                        .fg(Color::hex(theme::TEXT_PRIMARY))
                )
                .child(
                    Element::label(format!("{} 已准备就绪，可以开始使用", meta::app_display_name()))
                        .font_size(13.0)
                        .fg(Color::hex(theme::TEXT_SECONDARY))
                        .width_match()
                        .text_align(Align::Center)
                )
        )
        // 安装失败
        .child(
            Element::col()
                .width_match()
                .spacing(8)
                .cross(Align::Center)
                .visible_when(move || !finish_success.get())
                .child(
                    Element::label("✗")
                        .font_size(44.0)
                        .fg(Color::hex(theme::ERROR))
                )
                .child(
                    Element::label("安装失败")
                        .font_size(18.0)
                        .fg(Color::hex(theme::TEXT_PRIMARY))
                )
                .child(
                    Element::label_rc(finish_error)
                        .font_size(12.0)
                        .fg(Color::hex(theme::ERROR))
                        .width_match()
                        .text_align(Align::Center)
                )
        )
        .child(Element::leaf().weight(1.0))
        // 完成按钮
        .child(
            Element::button("完 成")
                .width(200)
                .height(48)
                .corner(8.0)
                .bg(Color::hex(theme::ACCENT))
                .fg(Color::hex(0xFFFFFF))
                .align(Align::Center)
                .on_click(move |_ctx: &mut EventCtx| {
                    std::process::exit(0);
                })
        )
        .child(Element::leaf().height(20));

    // ============================================================
    //  无边框自定义标题栏：浅色，与窗口背景同色，左侧标题右侧按钮
    // ============================================================
    #[cfg(feature = "frameless")]
    let title_bar = Element::row()
        .width_match()
        .height(36)
        .cross(Align::Center)
        .bg(Color::hex(theme::BG_PRIMARY))
        .window_drag()
        .child(Element::leaf().width(14))
        .child(
            Element::label(title.clone())
                .font_size(12.0)
                .fg(Color::hex(theme::TEXT_SECONDARY)),
        )
        .child(Element::leaf().weight(1.0))
        .child(Element::window_button(WindowButtonKind::Minimize).fg(Color::hex(theme::TEXT_SECONDARY)))
        .child(Element::window_button(WindowButtonKind::Close).fg(Color::hex(theme::TEXT_SECONDARY)));

    // ============================================================
    //  组装根节点
    // ============================================================
    let root = Element::col()
        .size(win_w, win_h)
        .bg(Color::hex(theme::BG_PRIMARY));

    #[cfg(feature = "frameless")]
    let root = root.child(title_bar);

    let root = root
        .child(page_config)
        .child(page_progress)
        .child(page_finish);

    let app = app
        .centered()
        .resizable(false)
        .accelerated(super::is_accelerated())
        .bg(Color::hex(theme::BG_PRIMARY))
        .content(root);

    #[cfg(feature = "frameless")]
    let app = app.frameless();

    app.run();
}

fn default_install_dir() -> String {
    use winreg::enums::*;
    use winreg::RegKey;
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let key_path = format!(
        r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{}",
        crate::meta::app_display_name()
    );
    if let Ok(key) = hklm.open_subkey_with_flags(&key_path, KEY_READ) {
        if let Ok(dir) = key.get_value::<String, _>("InstallLocation") {
            if !dir.is_empty() {
                return dir;
            }
        }
    }
    format!(r"%ProgramFiles%\{}", crate::meta::app_id())
}

fn default_portable_dir() -> String {
    format!(r"%USERPROFILE%\{}", crate::meta::app_id())
}

fn default_data_dir() -> String {
    format!(r"%APPDATA%\{}", crate::meta::app_id())
}

/// 展开路径中的 %VAR% 环境变量占位符。
fn expand_env_path(s: &str) -> std::path::PathBuf {
    let mut result = s.to_string();
    for (var, fallback) in &[
        ("ProgramFiles", r"C:\Program Files"),
        ("APPDATA",      r"C:\Users\Default\AppData\Roaming"),
        ("LOCALAPPDATA", r"C:\Users\Default\AppData\Local"),
        ("USERPROFILE",  r"C:\Users\Default"),
    ] {
        let token = format!("%{}%", var);
        if result.contains(&token) {
            let val = std::env::var(var).unwrap_or_else(|_| fallback.to_string());
            result = result.replace(&token, &val);
        }
    }
    std::path::PathBuf::from(result)
}

/// 构建选目录用的 `PickDialog`：起始目录取当前配置路径（`%VAR%` 展开后）最近的
/// 已存在祖先目录，而不是让 Shell 用它自己记住的上次访问位置——那个位置完全不
/// 可控（可能是网络共享/已拔出的移动盘/云盘同步目录），既是"偶发打开卡顿"的
/// 诱因之一，也会让"更改"按钮的默认落点跟用户已经填好的路径对不上。
fn folder_pick_dialog(title: &str, current_path: &str) -> PickDialog {
    let mut dialog = PickDialog::new().title(title);
    let expanded = expand_env_path(current_path);
    let mut probe = Some(expanded.as_path());
    while let Some(p) = probe {
        if p.exists() {
            dialog = dialog.directory(p);
            break;
        }
        probe = p.parent();
    }
    dialog
}

/// 用户选完目录后的收尾：若选的目录名不是 APP_ID，自动追加子目录。
fn finalize_picked_folder(path: PathBuf) -> PathBuf {
    if path.file_name().map(|n| n != crate::meta::app_id()).unwrap_or(true) {
        path.join(crate::meta::app_id())
    } else {
        path
    }
}
