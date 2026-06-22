use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::sync::mpsc;

use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, MessageBoxW, IDYES, MB_DEFBUTTON2, MB_ICONWARNING, MB_YESNO,
};
use windows::core::PCWSTR;
use windui::app::App;
use windui::core::EventCtx;
use windui::geometry::Color;
use windui::spec::Align;
use windui::ui::{Element, WindowButtonKind};

use crate::meta;
use super::theme;

const PAGE_CONFIRM: usize = 0;
const PAGE_PROGRESS: usize = 1;
const PAGE_FINISH: usize = 2;

const WIN_W: i32 = crate::meta::UNINSTALL_WIN_W;
#[cfg(feature = "frameless")]
const WIN_H: i32 = crate::meta::UNINSTALL_WIN_H;
#[cfg(not(feature = "frameless"))]
const WIN_H: i32 = crate::meta::UNINSTALL_WIN_H - 40;

enum UninstallMsg {
    Status(String),
    Finished(bool, String),
}

pub fn run_uninstall_wizard() {
    // ---- 状态 ----
    let current_page = Rc::new(Cell::new(PAGE_CONFIRM));
    let clean_roaming = Rc::new(Cell::new(false));
    let clean_cache = Rc::new(Cell::new(true));
    let confirmed = Rc::new(Cell::new(false));
    let finish_success = Rc::new(Cell::new(false));
    let finish_error = Rc::new(RefCell::new(String::new()));
    let status_text = Rc::new(RefCell::new(String::from("正在准备卸载...")));
    let install_dir = Rc::new(RefCell::new(detect_install_dir()));

    let rx: Arc<Mutex<Option<mpsc::Receiver<UninstallMsg>>>> = Arc::new(Mutex::new(None));

    // ---- 轮询闭包克隆 ----
    let rx_poll = rx.clone();
    let page_poll = current_page.clone();
    let status_poll = status_text.clone();
    let success_poll = finish_success.clone();
    let error_poll = finish_error.clone();

    // ---- 页面可见性克隆 ----
    let page_vis0 = current_page.clone();
    let page_vis1 = current_page.clone();
    let page_vis2 = current_page.clone();

    // ---- 标签引用克隆 ----
    let status_label = status_text.clone();
    let finish_error_label = finish_error.clone();
    let finish_success_ok = finish_success.clone();
    let finish_success_err = finish_success.clone();

    // ---- 卸载按钮克隆 ----
    let conf_enabled = confirmed.clone();
    let rx_btn = rx.clone();
    let conf_btn = confirmed.clone();
    let page_btn = current_page.clone();
    let cr_btn = clean_roaming.clone();
    let cc_btn = clean_cache.clone();
    let idir_btn = install_dir.clone();

    // ---- 完成按钮克隆 ----
    let idir_finish = install_dir.clone();

    // ============================================================
    //  品牌区（与安装器一致）
    // ============================================================
    let brand = Element::col()
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
                .text_align(Align::Center),
        )
        .child(
            Element::label(meta::APP_VERSION)
                .width_match()
                .font_size(11.0)
                .fg(Color::hex(theme::TEXT_MUTED))
                .text_align(Align::Center),
        );

    // ============================================================
    //  PAGE 0：确认页
    // ============================================================
    let page_confirm = Element::col()
        .fill()
        .weight(1.0)
        .padding_xy(40, 0)
        .spacing(12)
        .visible_when(move || page_vis0.get() == PAGE_CONFIRM)
        .child(
            Element::label(format!("即将从您的电脑中卸载 {}，请确认：", meta::APP_DISPLAY_NAME))
                .font_size(13.0)
                .fg(Color::hex(theme::TEXT_SECONDARY))
                .width_match(),
        )
        .child(Element::checkbox(
            &format!("删除用户词库和配置数据（%APPDATA%\\{}）", meta::APP_ID),
            clean_roaming.clone(),
        ))
        .child(Element::checkbox(
            &format!("清除本地词库缓存（%LOCALAPPDATA%\\{}\\cache）", meta::APP_ID),
            clean_cache.clone(),
        ))
        .child(Element::leaf().weight(1.0))
        .child(Element::checkbox("我已确认，继续卸载", confirmed.clone()))
        .child(
            Element::row()
                .width_match()
                .spacing(10)
                .child(Element::leaf().weight(1.0))
                .child(
                    Element::button("开始卸载")
                        .width(120)
                        .height(42)
                        .corner(8.0)
                        .bg(Color::hex(theme::ERROR))
                        .fg(Color::hex(0xFFFFFF))
                        .enabled(conf_enabled)
                        .on_click(move |_ctx: &mut EventCtx| {
                            let _ = conf_btn.get(); // enabled() 已做拦截

                            // 勾选删除用户数据时，Win32 弹框二次确认
                            if cr_btn.get() && !confirm_delete_user_data() {
                                return;
                            }

                            page_btn.set(PAGE_PROGRESS);

                            let options = crate::uninstaller::cleanup::CleanupOptions {
                                install_dir: idir_btn.borrow().clone(),
                                clean_roaming: cr_btn.get(),
                                clean_local_cache: cc_btn.get(),
                                keep_user_data: false,
                            };

                            let (tx, new_rx) = mpsc::channel::<UninstallMsg>();
                            if let Ok(mut g) = rx_btn.lock() {
                                *g = Some(new_rx);
                            }

                            std::thread::spawn(move || {
                                macro_rules! step {
                                    ($msg:expr) => {
                                        tx.send(UninstallMsg::Status($msg.into())).ok();
                                    };
                                }

                                let _ = crate::installer::registry::set_installer_running();

                                step!("正在停止相关进程...");
                                let _ = crate::installer::process::terminate_windinput_processes();

                                #[cfg(feature = "ime")]
                                {
                                    step!("正在反注册输入法...");
                                    let _ = crate::installer::ime::unregister_input_method();

                                    step!("正在反注册 COM 组件...");
                                    let _ = crate::installer::ime::unregister_old_com(&options.install_dir);
                                }

                                #[cfg(feature = "font")]
                                {
                                    step!("正在卸载字体...");
                                    let _ = crate::installer::font::uninstall_font();
                                }

                                step!("正在删除快捷方式...");
                                let _ = crate::installer::shortcut::delete_shortcuts();

                                step!("正在删除安装文件...");
                                let _ = crate::uninstaller::cleanup::delete_install_files(&options.install_dir);

                                step!("正在清理注册表...");
                                crate::uninstaller::cleanup::cleanup_registry();

                                step!("正在清理用户数据...");
                                let _ = crate::uninstaller::cleanup::cleanup_user_data(&options);

                                let _ = crate::installer::registry::clear_installer_running();

                                tx.send(UninstallMsg::Finished(true, String::new())).ok();
                            });
                        }),
                )
                .child(
                    Element::button("取消")
                        .width(80)
                        .height(42)
                        .corner(21.0)
                        .on_click(|_ctx: &mut EventCtx| {
                            std::process::exit(0);
                        }),
                ),
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
        .child(Element::leaf().weight(1.0))
        .child(
            Element::col()
                .width_match()
                .spacing(10)
                .cross(Align::Center)
                .child(
                    Element::label("正在卸载，请稍候...")
                        .font_size(14.0)
                        .fg(Color::hex(theme::TEXT_PRIMARY)),
                )
                .child(
                    Element::label_rc(status_label)
                        .font_size(12.0)
                        .fg(Color::hex(theme::TEXT_SECONDARY))
                        .width_match()
                        .text_align(Align::Center),
                ),
        )
        .child(Element::leaf().weight(1.0));

    // ============================================================
    //  PAGE 2：完成页
    // ============================================================
    let page_finish = Element::col()
        .fill()
        .weight(1.0)
        .padding_xy(48, 0)
        .visible_when(move || page_vis2.get() == PAGE_FINISH)
        .child(Element::leaf().weight(1.0))
        // 成功
        .child(
            Element::col()
                .width_match()
                .spacing(8)
                .cross(Align::Center)
                .visible_when(move || finish_success_ok.get())
                .child(
                    Element::label("✓")
                        .font_size(44.0)
                        .fg(Color::hex(theme::SUCCESS)),
                )
                .child(
                    Element::label("卸载完成")
                        .font_size(18.0)
                        .fg(Color::hex(theme::TEXT_PRIMARY)),
                )
                .child(
                    Element::label(format!("{} 已从您的电脑中移除", meta::APP_DISPLAY_NAME))
                        .font_size(13.0)
                        .fg(Color::hex(theme::TEXT_SECONDARY)),
                ),
        )
        // 失败
        .child(
            Element::col()
                .width_match()
                .spacing(8)
                .cross(Align::Center)
                .visible_when(move || !finish_success_err.get())
                .child(
                    Element::label("✗")
                        .font_size(44.0)
                        .fg(Color::hex(theme::ERROR)),
                )
                .child(
                    Element::label("卸载失败")
                        .font_size(18.0)
                        .fg(Color::hex(theme::TEXT_PRIMARY)),
                )
                .child(
                    Element::label_rc(finish_error_label)
                        .font_size(12.0)
                        .fg(Color::hex(theme::ERROR))
                        .width_match()
                        .text_align(Align::Center),
                ),
        )
        .child(Element::leaf().weight(1.0))
        .child(
            Element::button("完 成")
                .width(200)
                .height(48)
                .corner(8.0)
                .bg(Color::hex(theme::ACCENT))
                .fg(Color::hex(0xFFFFFF))
                .align(Align::Center)
                .on_click(move |_ctx: &mut EventCtx| {
                    let dir = idir_finish.borrow().clone();
                    // trigger_self_delete 内部调用 process::exit(0)，不返回
                    let _ = crate::uninstaller::selfdelete::trigger_self_delete(&dir);
                    // 自删除失败时直接退出
                    std::process::exit(0);
                }),
        )
        .child(Element::leaf().height(20));

    // ============================================================
    //  轮询叶节点（0×0，每帧执行进度消息收取）
    // ============================================================
    let poll_leaf = Element::leaf()
        .size(0, 0)
        .visible_when(move || {
            if let Ok(mut guard) = rx_poll.lock() {
                if let Some(ref rx) = *guard {
                    let mut done = false;
                    while let Ok(msg) = rx.try_recv() {
                        match msg {
                            UninstallMsg::Status(s) => {
                                *status_poll.borrow_mut() = s;
                            }
                            UninstallMsg::Finished(ok, detail) => {
                                success_poll.set(ok);
                                if !ok {
                                    *error_poll.borrow_mut() = detail;
                                }
                                page_poll.set(PAGE_FINISH);
                                done = true;
                            }
                        }
                    }
                    if done {
                        *guard = None;
                    }
                    windui::anim::request_repaint();
                }
            }
            false
        });

    // ============================================================
    //  无边框自定义标题栏
    // ============================================================
    #[cfg(feature = "frameless")]
    let title_bar = {
        let title_text = format!("{} 卸载程序", meta::APP_DISPLAY_NAME);
        Element::row()
            .width_match()
            .height(36)
            .cross(Align::Center)
            .bg(Color::hex(theme::BG_PRIMARY))
            .window_drag()
            .child(Element::leaf().width(14))
            .child(
                Element::label(title_text)
                    .font_size(12.0)
                    .fg(Color::hex(theme::TEXT_MUTED)),
            )
            .child(Element::leaf().weight(1.0))
            .child(Element::window_button(WindowButtonKind::Minimize).fg(Color::hex(theme::TEXT_SECONDARY)))
            .child(Element::window_button(WindowButtonKind::Close).fg(Color::hex(theme::TEXT_SECONDARY)))
    };

    // ============================================================
    //  根节点
    // ============================================================
    let root = Element::col()
        .size(WIN_W, WIN_H)
        .bg(Color::hex(theme::BG_PRIMARY));

    #[cfg(feature = "frameless")]
    let root = root.child(title_bar);

    let root = root
        .child(brand)
        .child(page_confirm)
        .child(page_progress)
        .child(page_finish)
        .child(poll_leaf);

    let app = App::new(
        format!("{} 卸载程序", meta::APP_DISPLAY_NAME),
        WIN_W,
        WIN_H,
    )
    .centered()
    .resizable(false)
    .bg(Color::hex(theme::BG_PRIMARY))
    .content(root);

    #[cfg(feature = "frameless")]
    let app = app.frameless();

    app.run();
}

/// 删除用户数据的 Win32 二次确认对话框；返回 true 表示用户选择继续。
fn confirm_delete_user_data() -> bool {
    let msg: Vec<u16> = format!(
        "此操作将永久删除 %APPDATA%\\{} 下的所有词库和配置数据，无法恢复。\n\n确定要继续吗？\0",
        crate::meta::APP_ID
    )
    .encode_utf16()
    .collect();
    let title: Vec<u16> = "确认删除用户数据\0".encode_utf16().collect();
    unsafe {
        MessageBoxW(
            GetForegroundWindow(),
            PCWSTR(msg.as_ptr()),
            PCWSTR(title.as_ptr()),
            MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2,
        ) == IDYES
    }
}

/// 从注册表读取安装目录；找不到时回退到默认路径
fn detect_install_dir() -> PathBuf {
    use winreg::enums::*;
    use winreg::RegKey;

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let key_path = format!(
        r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{}",
        meta::APP_DISPLAY_NAME
    );
    if let Ok(key) = hklm.open_subkey_with_flags(&key_path, KEY_READ) {
        if let Ok(dir) = key.get_value::<String, _>("InstallLocation") {
            if !dir.is_empty() {
                return PathBuf::from(dir);
            }
        }
    }
    let pf = std::env::var("ProgramFiles").unwrap_or_else(|_| r"C:\Program Files".to_string());
    PathBuf::from(pf).join(meta::APP_ID)
}
