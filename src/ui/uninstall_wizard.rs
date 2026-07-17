use std::path::PathBuf;

use windui::app::App;
use windui::core::EventCtx;
use windui::geometry::Color;
use windui::signal::signal;
use windui::spec::Align;
use windui::ui::{Element, WindowButtonKind};

use crate::meta;
use super::theme;

const PAGE_CONFIRM: usize = 0;
const PAGE_PROGRESS: usize = 1;
const PAGE_FINISH: usize = 2;

enum UninstallMsg {
    Status(String),
    Finished(bool, String),
}

pub fn run_uninstall_wizard() {
    // ---- 运行期窗口尺寸（来自清单；非无边框模式高度 -40 补偿系统标题栏）----
    let (win_w, base_h) = meta::uninstall_win();
    let win_h = if cfg!(feature = "frameless") { base_h } else { base_h - 40 };
    let title = format!("{} 卸载程序", meta::app_display_name());

    // ---- 状态（Signal<T> 是 Copy 句柄，move 闭包自动复制，无需 clone 样板）----
    let current_page      = signal(PAGE_CONFIRM);
    let clean_roaming     = signal(false);
    let clean_cache       = signal(true);
    let backup_to_desktop = signal(true);
    let confirmed         = signal(false);
    let finish_success    = signal(false);
    let finish_error      = signal(String::new());
    let status_text       = signal(String::from("正在准备卸载..."));
    let install_dir       = signal(detect_install_dir());
    let show_delete_confirm = signal(false);

    // ---- 跨线程进度通道（on_message 在 UI 线程调用，可直接写 Signal）----
    let mut app = App::new(title.clone(), win_w, win_h);
    let tx = app.channel::<UninstallMsg>(move |msg| match msg {
        UninstallMsg::Status(s) => status_text.set(s),
        UninstallMsg::Finished(ok, detail) => {
            finish_success.set(ok);
            if !ok {
                finish_error.set(detail);
            }
            current_page.set(PAGE_FINISH);
        }
    });

    // ============================================================
    //  品牌区（与安装器一致）
    // ============================================================
    let brand = Element::col()
        .width_match()
        .padding_xy(0, 18)
        .spacing(8)
        .cross(Align::Center)
        .child(
            Element::image_bytes(meta::logo())
                .size(52, 52)
                .corner(13.0)
        )
        .child(
            Element::label(meta::app_display_name())
                .width_match()
                .font_size(17.0)
                .fg(Color::hex(theme::text_primary()))
                .text_align(Align::Center),
        )
        .child(
            Element::label(meta::app_version())
                .width_match()
                .font_size(11.0)
                .fg(Color::hex(theme::text_muted()))
                .text_align(Align::Center),
        );

    // 删除用户数据：危险勾选行（受控 on_toggle：未勾时弹应用内确认对话框，已勾时直接取消）
    let delete_data_row = Element::checkbox(
        format!(
            "{}（%APPDATA%\\{}）",
            meta::s_user_data_label(),
            meta::app_id()
        ),
        clean_roaming,
    )
    .danger()
    .on_toggle(move |_ctx: &mut EventCtx| {
        if clean_roaming.get() {
            clean_roaming.set(false); // 已勾 → 直接取消
        } else {
            show_delete_confirm.set(true); // 未勾 → 弹确认，确认后才勾
        }
    });

    // ============================================================
    //  PAGE 0：确认页
    // ============================================================
    let page_confirm = Element::col()
        .fill()
        .weight(1.0)
        .padding_xy(40, 0)
        .spacing(12)
        .visible_when(move || current_page.get() == PAGE_CONFIRM)
        .child(
            Element::label(format!("即将从您的电脑中卸载 {}，请确认：", meta::app_display_name()))
                .font_size(13.0)
                .fg(Color::hex(theme::text_secondary()))
                .width_match(),
        )
        .child(delete_data_row)
        .child(
            Element::checkbox("删除前备份配置数据到桌面（推荐）", backup_to_desktop)
                .enabled(clean_roaming),
        )
        .child(Element::checkbox(
            &format!(
                "{}（%LOCALAPPDATA%\\{}\\cache）",
                meta::s_cache_label(),
                meta::app_id()
            ),
            clean_cache,
        ))
        .child(Element::leaf().weight(1.0))
        .child(Element::checkbox("我已确认，继续卸载", confirmed))
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
                        .bg(Color::hex(theme::error()))
                        .fg(Color::hex(0xFFFFFF))
                        .enabled(confirmed)
                        .on_click(move |_ctx: &mut EventCtx| {
                            current_page.set(PAGE_PROGRESS);

                            let options = crate::uninstaller::cleanup::CleanupOptions {
                                install_dir:      install_dir.get(),
                                clean_roaming:    clean_roaming.get(),
                                clean_local_cache: clean_cache.get(),
                                backup_to_desktop: backup_to_desktop.get(),
                                keep_user_data:   false,
                            };

                            let tx = tx.clone();
                            std::thread::spawn(move || {
                                macro_rules! step {
                                    ($msg:expr) => {
                                        tx.send(UninstallMsg::Status($msg.into())).ok();
                                    };
                                }

                                let _ = crate::installer::registry::set_installer_running();

                                step!("正在停止相关进程...");
                                let _ = crate::installer::process::terminate_app_processes();

                                if crate::meta::manifest().ime.is_some() {
                                    step!("正在反注册输入法...");
                                    let _ = crate::installer::ime::unregister_input_method();

                                    step!("正在反注册 COM 组件...");
                                    let _ = crate::installer::ime::unregister_old_com(&options.install_dir);
                                }

                                if !crate::meta::manifest().font.is_empty() {
                                    step!("正在卸载字体...");
                                    let _ = crate::installer::font::uninstall_font();
                                }

                                step!("正在删除快捷方式...");
                                let _ = crate::installer::shortcut::delete_shortcuts(
                                    &crate::meta::manifest().shortcut,
                                );

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
        .visible_when(move || current_page.get() == PAGE_PROGRESS)
        .child(Element::leaf().weight(1.0))
        .child(
            Element::col()
                .width_match()
                .spacing(10)
                .cross(Align::Center)
                .child(
                    Element::label("正在卸载，请稍候...")
                        .font_size(14.0)
                        .fg(Color::hex(theme::text_primary())),
                )
                .child(
                    Element::label_rc(status_text)
                        .font_size(12.0)
                        .fg(Color::hex(theme::text_secondary()))
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
        .visible_when(move || current_page.get() == PAGE_FINISH)
        .child(Element::leaf().weight(1.0))
        // 成功
        .child(
            Element::col()
                .width_match()
                .spacing(8)
                .cross(Align::Center)
                .visible_when(move || finish_success.get())
                .child(
                    Element::label("✓")
                        .font_size(44.0)
                        .fg(Color::hex(theme::success())),
                )
                .child(
                    Element::label("卸载完成")
                        .font_size(18.0)
                        .fg(Color::hex(theme::text_primary())),
                )
                .child(
                    Element::label(format!("{} 已从您的电脑中移除", meta::app_display_name()))
                        .font_size(13.0)
                        .fg(Color::hex(theme::text_secondary())),
                ),
        )
        // 失败
        .child(
            Element::col()
                .width_match()
                .spacing(8)
                .cross(Align::Center)
                .visible_when(move || !finish_success.get())
                .child(
                    Element::label("✗")
                        .font_size(44.0)
                        .fg(Color::hex(theme::error())),
                )
                .child(
                    Element::label("卸载失败")
                        .font_size(18.0)
                        .fg(Color::hex(theme::text_primary())),
                )
                .child(
                    Element::label_rc(finish_error)
                        .font_size(12.0)
                        .fg(Color::hex(theme::error()))
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
                .bg(Color::hex(theme::accent()))
                .fg(Color::hex(0xFFFFFF))
                .align(Align::Center)
                .on_click(move |_ctx: &mut EventCtx| {
                    let dir = install_dir.get();
                    // trigger_self_delete 内部调用 process::exit(0)，不返回
                    let _ = crate::uninstaller::selfdelete::trigger_self_delete(&dir);
                    std::process::exit(0);
                }),
        )
        .child(Element::leaf().height(20));

    // ============================================================
    //  无边框自定义标题栏
    // ============================================================
    #[cfg(feature = "frameless")]
    let title_bar = {
        let title_text = title.clone();
        Element::row()
            .width_match()
            .height(36)
            .cross(Align::Center)
            .bg(Color::hex(theme::bg_primary()))
            .window_drag()
            .child(Element::leaf().width(14))
            .child(
                Element::label(title_text)
                    .font_size(12.0)
                    .fg(Color::hex(theme::text_muted())),
            )
            .child(Element::leaf().weight(1.0))
            .child(Element::window_button(WindowButtonKind::Minimize).fg(Color::hex(theme::text_secondary())))
            .child(Element::window_button(WindowButtonKind::Close).fg(Color::hex(theme::text_secondary())))
    };

    // ============================================================
    //  根节点
    // ============================================================
    // 删除用户数据二次确认对话框（windui 应用内模态；确认后才真正勾选）
    let delete_dialog = {
        Element::dialog(
            show_delete_confirm,
            Element::col()
                .width(360)
                .bg(Color::hex(theme::bg_primary()))
                .corner(14.0)
                .padding(22)
                .spacing(14)
                .child(
                    Element::label("删除用户数据")
                        .font_size(17.0)
                        .fg(Color::hex(theme::text_primary()))
                        .width_match(),
                )
                .child(
                    Element::label(meta::s_delete_data_confirm(&format!(
                        "%APPDATA%\\{}",
                        meta::app_id()
                    )))
                    .font_size(13.0)
                    .fg(Color::hex(theme::text_secondary()))
                    .width_match(),
                )
                .child(
                    Element::row()
                        .width_match()
                        .spacing(10)
                        .child(Element::leaf().weight(1.0))
                        .child(
                            Element::button("取消")
                                .width(80)
                                .height(38)
                                .corner(8.0)
                                .on_click(move |_ctx: &mut EventCtx| {
                                    show_delete_confirm.set(false);
                                }),
                        )
                        .child(
                            Element::button("确定删除")
                                .width(100)
                                .height(38)
                                .corner(8.0)
                                .bg(Color::hex(theme::error()))
                                .fg(Color::hex(0xFFFFFF))
                                .on_click(move |_ctx: &mut EventCtx| {
                                    clean_roaming.set(true); // 确认后才真正勾选
                                    show_delete_confirm.set(false);
                                }),
                        ),
                ),
        )
    };

    let content = Element::col()
        .size(win_w, win_h)
        .bg(Color::hex(theme::bg_primary()));

    #[cfg(feature = "frameless")]
    let content = content.child(title_bar);

    let content = content
        .child(brand)
        .child(page_confirm)
        .child(page_progress)
        .child(page_finish);

    // 用 stack 叠加模态对话框（显示时覆盖全窗）
    let root = Element::stack()
        .size(win_w, win_h)
        .child(content)
        .child(delete_dialog);

    let app = app
        .centered()
        .resizable(false)
        .accelerated(super::is_accelerated())
        .bg(Color::hex(theme::bg_primary()))
        .content(root);

    #[cfg(feature = "frameless")]
    let app = app.frameless();

    app.run();
}

/// 从注册表读取安装目录；找不到时回退到默认路径。
fn detect_install_dir() -> PathBuf {
    use winreg::enums::*;
    use winreg::RegKey;

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let key_path = format!(
        r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{}",
        meta::app_display_name()
    );
    if let Ok(key) = hklm.open_subkey_with_flags(&key_path, KEY_READ) {
        if let Ok(dir) = key.get_value::<String, _>("InstallLocation") {
            if !dir.is_empty() {
                return PathBuf::from(dir);
            }
        }
    }
    let pf = std::env::var("ProgramFiles").unwrap_or_else(|_| r"C:\Program Files".to_string());
    PathBuf::from(pf).join(meta::app_id())
}
