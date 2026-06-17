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
use crate::meta;
use super::theme;

const PAGE_CONFIG: usize = 0;
const PAGE_PROGRESS: usize = 1;
const PAGE_FINISH: usize = 2;

const WIN_W: i32 = 480;
const WIN_H: i32 = 400;

enum ProgressMsg {
    Status(String),
    Total(usize),
    Done(usize),
    Finished(bool, String),
}

pub fn run_install_wizard() {
    // ---- 状态 ----
    let is_fresh_install = crate::installer::registry::detect_installed_version().is_none();
    let current_page = Rc::new(Cell::new(PAGE_CONFIG));
    let install_mode = Rc::new(Cell::new(0usize));
    let install_dir = Rc::new(RefCell::new(default_install_dir()));
    let data_dir = Rc::new(RefCell::new(default_data_dir()));
    let use_custom_data_dir = Rc::new(Cell::new(false));
    let progress_text = Rc::new(RefCell::new(String::from("正在准备安装...")));
    let progress_value = Rc::new(Cell::new(0.0f32));
    let progress_total = Rc::new(Cell::new(1usize));
    let finish_success = Rc::new(Cell::new(false));
    let finish_error = Rc::new(RefCell::new(String::new()));
    let config_error = Rc::new(RefCell::new(String::new()));
    let agreed = Rc::new(Cell::new(false));
    let rx: Arc<Mutex<Option<mpsc::Receiver<ProgressMsg>>>> = Arc::new(Mutex::new(None));

    // ---- 进度轮询闭包所需克隆 ----
    let ptext_poll = progress_text.clone();
    let pval_poll = progress_value.clone();
    let ptotal_poll = progress_total.clone();
    let rx_poll = rx.clone();
    let success_poll = finish_success.clone();
    let error_poll = finish_error.clone();
    let page_poll = current_page.clone();

    // ---- 页面可见性克隆 ----
    let page_vis0 = current_page.clone();
    let page_vis1 = current_page.clone();
    let page_vis2 = current_page.clone();

    // ---- 安装按钮所需克隆 ----
    let rx_btn = rx.clone();
    let idir_btn = install_dir.clone();
    let mode_btn = install_mode.clone();
    let ddir_btn = data_dir.clone();
    let custom_btn = use_custom_data_dir.clone();
    let agreed_btn = agreed.clone();
    let page_btn = current_page.clone();
    let pval_btn = progress_value.clone();
    let cerr_btn = config_error.clone();

    // ---- UI 绑定克隆 ----
    let install_dir_input = install_dir.clone();
    let install_dir_browse = install_dir.clone();
    let data_dir_input = data_dir.clone();
    let data_dir_browse_btn = data_dir.clone();
    let progress_text_label = progress_text.clone();
    let config_error_label = config_error.clone();
    let finish_error_label = finish_error.clone();
    let finish_success_ok = finish_success.clone();
    let finish_success_err = finish_success.clone();

    // ============================================================
    //  顶部品牌区（各页共用）
    // ============================================================
    let brand_section = Element::col()
        .width_match()
        .padding_xy(0, 18)
        .spacing(8)
        .cross(Align::Center)
        .child(
            Element::image_bytes(include_bytes!("../../assets/logo.png"))
                .size(52, 52)
                .corner(13.0)
        )
        .child(
            Element::label(meta::APP_DISPLAY_NAME)
                .width_match()
                .font_size(17.0)
                .fg(Color::hex(theme::TEXT_PRIMARY))
                .text_align(Align::Center)
        )
        .child(
            Element::label(meta::APP_VERSION)
                .width_match()
                .font_size(11.0)
                .fg(Color::hex(theme::TEXT_MUTED))
                .text_align(Align::Center)
        );

    // ============================================================
    //  PAGE 0：配置页
    // ============================================================
    let page_config = Element::col()
        .fill()
        .weight(1.0)
        .padding_xy(40, 0)
        .spacing(14)
        .visible_when(move || page_vis0.get() == PAGE_CONFIG)
        // 安装路径
        .child(
            Element::col()
                .width_match()
                .spacing(5)
                .child(
                    Element::label("安装目录")
                        .font_size(12.0)
                        .fg(Color::hex(theme::TEXT_SECONDARY))
                )
                .child(
                    Element::row()
                        .width_match()
                        .spacing(8)
                        .cross(Align::Center)
                        .child(
                            Element::text_input(install_dir_input, "安装路径")
                                .weight(1.0)
                                .height(34)
                        )
                        .child(
                            Element::button("更改")
                                .height(34)
                                .on_click(move |_ctx: &mut EventCtx| {
                                    if let Some(path) = browse_folder("选择安装目录") {
                                        *install_dir_browse.borrow_mut() =
                                            path.to_string_lossy().to_string();
                                    }
                                })
                        )
                )
        )
        // 数据目录（仅首次安装显示；升级时沿用旧配置）
        .child(
            Element::col()
                .width_match()
                .spacing(5)
                .visible_when(move || is_fresh_install)
                .child(
                    Element::label("数据目录（词库、配置）")
                        .font_size(12.0)
                        .fg(Color::hex(theme::TEXT_SECONDARY))
                )
                .child(
                    Element::row()
                        .width_match()
                        .spacing(8)
                        .cross(Align::Center)
                        .child(
                            Element::text_input(data_dir_input, "数据目录路径")
                                .weight(1.0)
                                .height(34)
                        )
                        .child(
                            Element::button("更改")
                                .height(34)
                                .on_click(move |_ctx: &mut EventCtx| {
                                    if let Some(path) = browse_folder("选择数据目录") {
                                        *data_dir_browse_btn.borrow_mut() =
                                            path.to_string_lossy().to_string();
                                    }
                                })
                        )
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
                        .child(Element::radio("标准安装（推荐）", install_mode.clone(), 0))
                        .child(Element::radio("便捷模式", install_mode.clone(), 1))
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
            Element::checkbox("我已阅读并同意《用户服务协议》", agreed.clone())
        )
        // 弹性空白，将按钮推到底部
        .child(Element::leaf().weight(1.0))
        // 校验错误提示（仅未勾协议时可见）
        .child(
            Element::label_rc(config_error_label)
                .font_size(11.0)
                .fg(Color::hex(theme::ERROR))
                .align(Align::Center)
        )
        // 安装按钮（align=Center 使其在父 col 中水平居中）
        .child(
            Element::button("立即安装")
                .width(200)
                .height(42)
                .corner(21.0)
                .bg(Color::hex(theme::ACCENT))
                .fg(Color::hex(0xFFFFFF))
                .align(Align::Center)
                .enabled(agreed.clone())
                .on_click(move |_ctx: &mut EventCtx| {
                    let _ = agreed_btn.get(); // 保留克隆引用，enabled() 已做拦截
                    *cerr_btn.borrow_mut() = String::new();
                    pval_btn.set(0.0);

                    page_btn.set(PAGE_PROGRESS);

                    let install_dir_val = PathBuf::from(idir_btn.borrow().clone());
                    let data_dir_val = PathBuf::from(ddir_btn.borrow().clone());
                    let use_custom = custom_btn.get();
                    let install_mode = if mode_btn.get() == 0 {
                        InstallMode::Standard
                    } else {
                        InstallMode::Portable
                    };

                    let (tx, new_rx) = mpsc::channel::<ProgressMsg>();
                    if let Ok(mut guard) = rx_btn.lock() {
                        *guard = Some(new_rx);
                    }

                    std::thread::spawn(move || {
                        let mut log = crate::util::log::InstallLogger::new();
                        log.log(&format!("安装目录: {:?}", install_dir_val));
                        log.log(&format!("安装模式: {:?}", install_mode));

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

                        if install_mode == InstallMode::Standard {
                            step!("正在停止旧进程...");
                            if let Err(e) = crate::installer::process::terminate_windinput_processes() {
                                log.log(&format!("  警告: {}", e));
                            }
                        }

                        #[cfg(feature = "ime")]
                        if install_mode == InstallMode::Standard {
                            step!("正在反注册旧 COM...");
                            if let Err(e) = crate::installer::ime::unregister_old_com(&config.install_dir) {
                                log.log(&format!("  警告: {}", e));
                            }
                        }

                        if install_mode == InstallMode::Standard {
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

                        if install_mode == InstallMode::Standard {
                            step!("正在设置文件权限...");
                            if let Err(e) = crate::installer::acl::set_dll_permissions(&config.install_dir) {
                                log.log(&format!("  警告: {}", e));
                            }
                            #[cfg(feature = "font")]
                            {
                                step!("正在安装字体...");
                                if let Err(e) = crate::installer::font::install_font(&config.install_dir) {
                                    log.log(&format!("  警告: {}", e));
                                }
                            }
                            #[cfg(feature = "ime")]
                            {
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
                            if !crate::meta::URL_PROTOCOL.is_empty() {
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
                                config.install_dir.join(crate::meta::PORTABLE_MARKER),
                                "portable=1\n",
                            );
                        }

                        log.log("=== 安装完成 ===");
                        let log_path = log.path.to_string_lossy().to_string();
                        tx.send(ProgressMsg::Finished(true, log_path)).ok();
                    });
                })
        )
        .child(Element::leaf().height(20));

    // ============================================================
    //  PAGE 1：进度页
    // ============================================================
    let page_progress = Element::col()
        .fill()
        .weight(1.0)
        .padding_xy(56, 0)
        .visible_when(move || page_vis1.get() == PAGE_PROGRESS)
        // 上方弹性空白
        .child(Element::leaf().weight(1.0))
        // 进度内容（居中）
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
                    Element::progress(progress_value.clone())
                        .width_match()
                        .height(6)
                        .corner(3.0)
                )
                .child(
                    Element::label_rc(progress_text_label)
                        .font_size(12.0)
                        .fg(Color::hex(theme::TEXT_SECONDARY))
                        .width_match()
                        .text_align(Align::Center)
                )
        )
        // 下方弹性空白
        .child(Element::leaf().weight(1.0));

    // ============================================================
    //  PAGE 2：完成页
    // ============================================================
    let page_finish = Element::col()
        .fill()
        .weight(1.0)
        .padding_xy(48, 0)
        .visible_when(move || page_vis2.get() == PAGE_FINISH)
        // 上方弹性空白
        .child(Element::leaf().weight(1.0))
        // 安装成功内容
        .child(
            Element::col()
                .width_match()
                .spacing(8)
                .cross(Align::Center)
                .visible_when(move || finish_success_ok.get())
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
                    Element::label(format!("{} 已准备就绪，可以开始使用", meta::APP_DISPLAY_NAME))
                        .font_size(13.0)
                        .fg(Color::hex(theme::TEXT_SECONDARY))
                )
        )
        // 安装失败内容
        .child(
            Element::col()
                .width_match()
                .spacing(8)
                .cross(Align::Center)
                .visible_when(move || !finish_success_err.get())
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
                    Element::label_rc(finish_error_label)
                        .font_size(12.0)
                        .fg(Color::hex(theme::ERROR))
                        .width_match()
                        .text_align(Align::Center)
                )
        )
        // 下方弹性空白
        .child(Element::leaf().weight(1.0))
        // 完成按钮
        .child(
            Element::button("完 成")
                .width(180)
                .height(42)
                .corner(21.0)
                .bg(Color::hex(theme::ACCENT))
                .fg(Color::hex(0xFFFFFF))
                .align(Align::Center)
                .on_click(move |_ctx: &mut EventCtx| {
                    std::process::exit(0);
                })
        )
        .child(Element::leaf().height(20));

    // ============================================================
    //  组装根节点
    // ============================================================
    let root = Element::col()
        .size(WIN_W, WIN_H)
        .bg(Color::hex(theme::BG_PRIMARY))
        .child(brand_section)
        .child(Element::divider())
        .child(page_config)
        .child(page_progress)
        .child(page_finish)
        // 0×0 隐藏节点：每帧调用 vis_cond 轮询后台进度消息
        .child(
            Element::leaf()
                .size(0, 0)
                .visible_when(move || {
                    if let Ok(mut guard) = rx_poll.lock() {
                        if let Some(ref rx) = *guard {
                            let mut done = false;
                            while let Ok(msg) = rx.try_recv() {
                                match msg {
                                    ProgressMsg::Status(s) => {
                                        *ptext_poll.borrow_mut() = s;
                                    }
                                    ProgressMsg::Total(t) => {
                                        ptotal_poll.set(t.max(1));
                                    }
                                    ProgressMsg::Done(n) => {
                                        let total = ptotal_poll.get();
                                        pval_poll.set((n as f32 / total as f32).min(0.99));
                                    }
                                    ProgressMsg::Finished(success, detail) => {
                                        pval_poll.set(1.0);
                                        success_poll.set(success);
                                        if !success {
                                            *error_poll.borrow_mut() = detail;
                                        }
                                        page_poll.set(PAGE_FINISH);
                                        done = true;
                                    }
                                }
                            }
                            if done {
                                *guard = None;
                                windui::anim::request_repaint(); // 触发最后一帧显示完成页
                            } else {
                                windui::anim::request_repaint();
                            }
                        }
                    }
                    false
                })
        );

    App::new(meta::APP_WINDOW_TITLE, WIN_W, WIN_H)
        .centered()
        .resizable(false)
        .bg(Color::hex(theme::BG_PRIMARY))
        .content(root)
        .run();
}

fn default_install_dir() -> String {
    let program_files = std::env::var("ProgramFiles")
        .unwrap_or_else(|_| r"C:\Program Files".to_string());
    format!(r"{}\{}", program_files, crate::meta::APP_ID)
}

fn default_data_dir() -> String {
    let app_data = std::env::var("APPDATA").unwrap_or_else(|_| {
        let up = std::env::var("USERPROFILE").unwrap_or_default();
        format!(r"{}\AppData\Roaming", up)
    });
    format!(r"{}\{}", app_data, crate::meta::APP_ID)
}

fn browse_folder(_title: &str) -> Option<PathBuf> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::{FOS_PICKFOLDERS, FileOpenDialog, IFileDialog};
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    unsafe {
        let owner = GetForegroundWindow();
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let dialog: IFileDialog =
            match CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER) {
                Ok(d) => d,
                Err(_) => return None,
            };
        let _ = dialog.SetOptions(FOS_PICKFOLDERS);
        if dialog.Show(HWND(owner.0)).is_err() {
            return None;
        }
        match dialog.GetResult() {
            Ok(item) => {
                match item.GetDisplayName(windows::Win32::UI::Shell::SIGDN_FILESYSPATH) {
                    Ok(name) => {
                        let path = PathBuf::from(name.to_string().unwrap_or_default());
                        // 如果用户选择的目录名不是 APP_ID，自动追加子目录
                        if path.file_name().map(|n| n != crate::meta::APP_ID).unwrap_or(true) {
                            Some(path.join(crate::meta::APP_ID))
                        } else {
                            Some(path)
                        }
                    }
                    Err(_) => None,
                }
            }
            Err(_) => None,
        }
    }
}
