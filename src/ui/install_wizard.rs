use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::sync::mpsc;

use windui::app::App;
use windui::core::EventCtx;
use windui::geometry::Color;
use windui::spec::Align;
use windui::ui::Element;

use crate::installer::InstallMode;

const PAGE_CONFIG: usize = 0;
const PAGE_PROGRESS: usize = 1;
const PAGE_FINISH: usize = 2;

/// 从后台线程发送给 UI 的进度消息。
enum ProgressMsg {
    /// 当前步骤描述。
    Status(String),
    /// 总文件数（已知时发送）。
    #[allow(dead_code)]
    Total(usize),
    /// 已完成文件数。
    Done(usize),
    /// 全部完成（成功或失败）。
    Finished(bool, String),
}

/// 安装向导入口。
pub fn run_install_wizard() {
    let current_page = Rc::new(Cell::new(PAGE_CONFIG));

    // 安装模式：0=标准安装，1=便捷模式
    let install_mode = Rc::new(Cell::new(0usize));

    // 路径输入
    let install_dir = Rc::new(RefCell::new(default_install_dir()));
    let data_dir = Rc::new(RefCell::new(default_data_dir()));
    let use_custom_data_dir = Rc::new(Cell::new(false));

    // 进度状态
    let progress_text = Rc::new(RefCell::new(String::from("正在准备安装...")));
    let progress_value = Rc::new(Cell::new(0.0f32));
    let finish_text = Rc::new(RefCell::new(String::from("清风输入法 已成功安装到您的计算机。")));

    // 后台线程消息通道
    let rx: Arc<Mutex<Option<mpsc::Receiver<ProgressMsg>>>> =
        Arc::new(Mutex::new(None));

    // 克隆给各闭包
    let page_config = current_page.clone();
    let page_progress = current_page.clone();
    let page_finish = current_page.clone();

    let mode_for_radio = install_mode.clone();
    let install_dir2 = install_dir.clone();

    let ptext_poll = progress_text.clone();
    let pval_poll = progress_value.clone();
    let rx_poll = rx.clone();
    let ftext_poll = finish_text.clone();
    let page_poll = current_page.clone();

    let ptext_btn = progress_text.clone();
    let rx_btn = rx.clone();

    let install_dir3 = install_dir.clone();
    let mode_for_install = install_mode.clone();
    let data_dir3 = data_dir.clone();
    let custom3 = use_custom_data_dir.clone();

    let install_dir_browse = install_dir.clone();

    let page_for_vis0 = current_page.clone();
    let page_for_vis1 = current_page.clone();
    let page_for_vis2 = current_page.clone();

    let root = Element::col()
        .size(520, 420)
        .bg(Color::hex(0xFFFFFF))
        .child(
            // ===== 顶部标题栏 =====
            Element::col()
                .width_match()
                .padding_xy(24, 20)
                .bg(Color::hex(0xF5F5F5))
                .child(Element::label("安装清风输入法").font_size(18.0).fg(Color::hex(0x1A1A1A)))
                .child(Element::label("按照安装向导的提示完成安装").font_size(12.0).fg(Color::hex(0x888888)).margin(4))
        )
        .child(Element::divider())
        .child(
            // ===== PAGE 0: 配置页 =====
            Element::col()
                .fill()
                .weight(1.0)
                .padding(24)
                .spacing(16)
                .visible_when({
                    let p = page_for_vis0.clone();
                    move || p.get() == PAGE_CONFIG
                })
                // 安装模式选择
                .child(
                    Element::col()
                        .width_match()
                        .spacing(6)
                        .child(Element::label("安装方式").font_size(13.0).fg(Color::hex(0x333333)))
                        .child(
                            Element::row()
                                .width_match()
                                .spacing(24)
                                .child(Element::radio("标准安装（推荐）", mode_for_radio.clone(), 0))
                                .child(Element::radio("便捷模式", mode_for_radio.clone(), 1))
                        )
                        .child(
                            Element::label("标准安装注册输入法到系统，开机自动启动；便捷模式仅解压文件，不修改系统")
                                .font_size(11.0)
                                .fg(Color::hex(0x999999))
                        )
                )
                // 安装目录
                .child(
                    Element::col()
                        .width_match()
                        .spacing(6)
                        .child(Element::label("安装目录").font_size(13.0).fg(Color::hex(0x333333)))
                        .child(
                            Element::row()
                                .width_match()
                                .spacing(8)
                                .cross(Align::Center)
                                .child(
                                    Element::text_input(install_dir2.clone(), "安装路径")
                                        .weight(1.0)
                                        .height(32)
                                )
                                .child(
                                    Element::button("浏览...")
                                        .on_click({
                                            let dir = install_dir_browse.clone();
                                            move |_ctx: &mut EventCtx| {
                                                if let Some(path) = browse_folder("选择安装目录") {
                                                    *dir.borrow_mut() = path.to_string_lossy().to_string();
                                                }
                                            }
                                        })
                                )
                        )
                )
                // 数据目录说明
                .child(
                    Element::col()
                        .width_match()
                        .spacing(6)
                        .visible_when({
                            let m = install_mode.clone();
                            move || m.get() == 0
                        })
                        .child(Element::label("数据位置").font_size(13.0).fg(Color::hex(0x333333)))
                        .child(
                            Element::label("用户词库、配置等数据将存储在 %APPDATA%\\WindInput")
                                .font_size(11.0)
                                .fg(Color::hex(0x999999))
                        )
                )
        )
        .child(
            // ===== PAGE 1: 进度页 =====
            Element::col()
                .fill()
                .weight(1.0)
                .padding(24)
                .spacing(12)
                .align(Align::Center)
                .visible_when({
                    let p = page_for_vis1.clone();
                    move || p.get() == PAGE_PROGRESS
                })
                .child(
                    Element::label("正在安装 清风输入法")
                        .font_size(16.0)
                        .fg(Color::hex(0x1A1A1A))
                )
                .child(
                    Element::progress(progress_value.clone())
                        .width_match()
                        .height(8)
                )
                .child(
                    // 使用 text_input 展示动态进度文本（只读）
                    Element::text_input(progress_text.clone(), "")
                        .width_match()
                        .height(28)
                )
        )
        .child(
            // ===== PAGE 2: 完成页 =====
            Element::col()
                .fill()
                .weight(1.0)
                .padding(24)
                .spacing(12)
                .align(Align::Center)
                .visible_when({
                    let p = page_for_vis2.clone();
                    move || p.get() == PAGE_FINISH
                })
                .child(
                    Element::label("安装完成")
                        .font_size(16.0)
                        .fg(Color::hex(0x1A1A1A))
                )
                .child(
                    Element::text_input(finish_text.clone(), "")
                        .width_match()
                        .height(28)
                )
        )
        .child(Element::divider())
        .child(
            // ===== 底部按钮栏 =====
            Element::row()
                .width_match()
                .padding_xy(24, 16)
                .cross(Align::Center)
                // 完成页：完成按钮
                .child(
                    Element::button("完成")
                        .align(Align::End)
                        .on_click({
                            move |_ctx: &mut EventCtx| {
                                std::process::exit(0);
                            }
                        })
                        .visible_when({
                            let p = page_finish.clone();
                            move || p.get() == PAGE_FINISH
                        })
                )
                // 配置页：安装按钮
                .child(
                    Element::button("立即安装")
                        .align(Align::End)
                        .on_click({
                            let page = page_progress.clone();
                            let ptext = ptext_btn.clone();
                            let rx_shared = rx_btn.clone();
                            let idir = install_dir3.clone();
                            let mode = mode_for_install.clone();
                            let ddir = data_dir3.clone();
                            let custom = custom3.clone();
                            move |_ctx: &mut EventCtx| {
                                page.set(PAGE_PROGRESS);
                                *ptext.borrow_mut() = "正在准备安装...".to_string();

                                let install_dir_val = PathBuf::from(idir.borrow().clone());
                                let data_dir_val = PathBuf::from(ddir.borrow().clone());
                                let use_custom = custom.get();
                                let install_mode = if mode.get() == 0 {
                                    InstallMode::Standard
                                } else {
                                    InstallMode::Portable
                                };

                                // 创建通道
                                let (tx, new_rx) = mpsc::channel::<ProgressMsg>();

                                // 替换全局 receiver
                                if let Ok(mut guard) = rx_shared.lock() {
                                    *guard = Some(new_rx);
                                }

                                // 后台线程执行安装
                                std::thread::spawn(move || {
                                    tx.send(ProgressMsg::Status("正在停止旧进程...".into())).ok();

                                    let mut config = crate::installer::config::InstallConfig::default();
                                    config.install_dir = install_dir_val;
                                    if use_custom {
                                        config.custom_data_dir = Some(data_dir_val);
                                        config.use_custom_data_dir = true;
                                    }

                                    // 停止旧进程
                                    if install_mode == InstallMode::Standard {
                                        let _ = crate::installer::process::terminate_windinput_processes();
                                    }

                                    // 反注册旧 COM
                                    if install_mode == InstallMode::Standard {
                                        let _ = crate::installer::ime::unregister_old_com(&config.install_dir);
                                    }

                                    // 打开归档
                                    tx.send(ProgressMsg::Status("正在解压文件...".into())).ok();

                                    let archive = match crate::archive::ArchiveReader::open_current_exe() {
                                        Ok(a) => a,
                                        Err(e) => {
                                            tx.send(ProgressMsg::Finished(false, format!("无法打开安装数据: {}", e))).ok();
                                            return;
                                        }
                                    };

                                    let entries: Vec<crate::archive::format::ArchiveEntry> = archive.entries().to_vec();
                                    let total = entries.len();
                                    let _ = tx.send(ProgressMsg::Total(total));

                                    // 在新线程中重新打开文件（ArchiveReader 的 File 不是 Send）
                                    let exe_path = std::env::current_exe().unwrap();
                                    let mut thread_archive = match crate::archive::ArchiveReader::open(&exe_path) {
                                        Ok(a) => a,
                                        Err(e) => {
                                            tx.send(ProgressMsg::Finished(false, format!("无法读取安装数据: {}", e))).ok();
                                            return;
                                        }
                                    };

                                    let _ = std::fs::create_dir_all(&config.install_dir);

                                    for (i, entry) in entries.iter().enumerate() {
                                        let dest = config.install_dir.join(&entry.path);
                                        let short = entry.path.clone();
                                        let _ = tx.send(ProgressMsg::Status(format!("解压: {}", short)));

                                        if let Err(e) = thread_archive.extract_entry(entry, &dest) {
                                            tx.send(ProgressMsg::Finished(false, format!("解压失败 {}: {}", entry.path, e))).ok();
                                            return;
                                        }
                                        let _ = tx.send(ProgressMsg::Done(i + 1));
                                    }

                                    // 标准模式：注册系统组件
                                    if install_mode == InstallMode::Standard {
                                        let _ = tx.send(ProgressMsg::Status("正在设置文件权限...".into()));
                                        let _ = crate::installer::acl::set_dll_permissions(&config.install_dir);

                                        let _ = tx.send(ProgressMsg::Status("正在安装字体...".into()));
                                        let _ = crate::installer::font::install_font(&config.install_dir);

                                        let _ = tx.send(ProgressMsg::Status("正在注册 COM 组件...".into()));
                                        let _ = crate::installer::ime::register_com(&config.install_dir);

                                        let _ = tx.send(ProgressMsg::Status("正在注册系统输入法...".into()));
                                        let _ = crate::installer::ime::register_input_method();

                                        let _ = tx.send(ProgressMsg::Status("正在配置开机自启动...".into()));
                                        let _ = crate::installer::registry::set_auto_start(&config.install_dir);

                                        let _ = tx.send(ProgressMsg::Status("正在注册协议...".into()));
                                        let _ = crate::installer::registry::register_url_protocol(&config.install_dir);

                                        let _ = tx.send(ProgressMsg::Status("正在创建快捷方式...".into()));
                                        let _ = crate::installer::shortcut::create_shortcuts(&config.install_dir);

                                        let _ = tx.send(ProgressMsg::Status("正在写入卸载信息...".into()));
                                        let _ = crate::installer::registry::write_uninstall_info(&config);

                                        let _ = tx.send(ProgressMsg::Status("正在启动服务...".into()));
                                        let _ = crate::installer::process::prestart_service(&config.install_dir);
                                    } else {
                                        // 便携模式标记
                                        let _ = std::fs::write(
                                            config.install_dir.join("wind_portable_mode"),
                                            "wind_portable=1\n",
                                        );
                                    }

                                    tx.send(ProgressMsg::Finished(true, String::new())).ok();
                                });
                            }
                        })
                        .visible_when({
                            let p = page_config.clone();
                            move || p.get() == PAGE_CONFIG
                        })
                )
        )
        // 轮询进度消息的隐藏节点
        .child(
            Element::leaf()
                .size(0, 0)
                .visible_when(move || {
                    if let Ok(guard) = rx_poll.lock() {
                        if let Some(ref rx) = *guard {
                            while let Ok(msg) = rx.try_recv() {
                                match msg {
                                    ProgressMsg::Status(s) => {
                                        *ptext_poll.borrow_mut() = s;
                                    }
                                    ProgressMsg::Total(_) => {}
                                    ProgressMsg::Done(n) => {
                                        pval_poll.set((n as f32 / 100.0).min(0.99));
                                    }
                                    ProgressMsg::Finished(success, err) => {
                                        pval_poll.set(1.0);
                                        if success {
                                            *ftext_poll.borrow_mut() = "清风输入法 已成功安装到您的计算机。".to_string();
                                        } else {
                                            *ftext_poll.borrow_mut() = format!("安装失败: {}", err);
                                        }
                                        page_poll.set(PAGE_FINISH);
                                    }
                                }
                            }
                        }
                    }
                    false // 始终不可见，仅用于触发副作用
                })
        );

    App::new("清风输入法 安装向导", 520, 420)
        .centered()
        .content(root)
        .run();
}

/// 默认安装目录
fn default_install_dir() -> String {
    let program_files = std::env::var("ProgramFiles")
        .unwrap_or_else(|_| r"C:\Program Files".to_string());
    format!(r"{}\WindInput", program_files)
}

/// 默认数据目录
fn default_data_dir() -> String {
    let app_data = std::env::var("APPDATA")
        .unwrap_or_else(|_| {
            let up = std::env::var("USERPROFILE").unwrap_or_default();
            format!(r"{}\AppData\Roaming", up)
        });
    format!(r"{}\WindInput", app_data)
}

/// 使用 Windows Shell API 选择文件夹。
fn browse_folder(title: &str) -> Option<PathBuf> {
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
    use windows::Win32::UI::Shell::{IFileDialog, FOS_PICKFOLDERS, FileOpenDialog};

    let _title = title; // 保留参数供未来使用

    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        let dialog: IFileDialog = match CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER) {
            Ok(d) => d,
            Err(_) => return None,
        };

        let _ = dialog.SetOptions(FOS_PICKFOLDERS);

        if dialog.Show(None).is_err() {
            return None;
        }

        match dialog.GetResult() {
            Ok(item) => {
                match item.GetDisplayName(windows::Win32::UI::Shell::SIGDN_FILESYSPATH) {
                    Ok(name) => Some(PathBuf::from(name.to_string().unwrap_or_default())),
                    Err(_) => None,
                }
            }
            Err(_) => None,
        }
    }
}
