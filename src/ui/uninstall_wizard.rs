use std::cell::Cell;
use std::rc::Rc;

use windui::prelude::*;

use crate::uninstaller::cleanup::CleanupOptions;
use crate::uninstaller::perform_uninstall;
use super::theme;

/// 卸载向导页面
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UninstallPage {
    /// 确认 + 选项
    Confirm,
    /// 卸载完成
    Finish,
}

/// 卸载向导状态
struct UninstallWizardState {
    /// 当前页面
    page: Rc<Cell<UninstallPage>>,
    /// 清除用户配置
    clean_roaming: Rc<Cell<bool>>,
    /// 清除本地缓存
    clean_local_cache: Rc<Cell<bool>>,
    /// 备份到桌面
    backup_to_desktop: Rc<Cell<bool>>,
    /// 确认卸载
    confirmed: Rc<Cell<bool>>,
    /// 卸载完成
    finished: Rc<Cell<bool>>,
    /// 卸载成功
    success: Rc<Cell<bool>>,
    /// 需要重启
    need_reboot: Rc<Cell<bool>>,
}

impl UninstallWizardState {
    fn new() -> Self {
        Self {
            page: Rc::new(Cell::new(UninstallPage::Confirm)),
            clean_roaming: Rc::new(Cell::new(false)),
            clean_local_cache: Rc::new(Cell::new(true)),
            backup_to_desktop: Rc::new(Cell::new(true)),
            confirmed: Rc::new(Cell::new(false)),
            finished: Rc::new(Cell::new(false)),
            success: Rc::new(Cell::new(false)),
            need_reboot: Rc::new(Cell::new(false)),
        }
    }
}

/// 运行卸载向导
pub fn run_uninstall_wizard() {
    let state = UninstallWizardState::new();

    let ui = build_ui(&state);

    App::new("清风输入法 卸载程序", 480, 400)
        .bg(Color::hex(theme::BG_COLOR))
        .content(ui)
        .run();
}

/// 构建 UI
fn build_ui(state: &UninstallWizardState) -> Element {
    let page = state.page.clone();
    let clean_roaming = state.clean_roaming.clone();
    let clean_local_cache = state.clean_local_cache.clone();
    let backup_to_desktop = state.backup_to_desktop.clone();
    let confirmed = state.confirmed.clone();
    let finished = state.finished.clone();
    let success = state.success.clone();
    let need_reboot = state.need_reboot.clone();

    let content = Element::col()
        .fill()
        .padding(24)
        .spacing(16);

    match page.get() {
        UninstallPage::Confirm => {
            build_confirm_page(
                content,
                clean_roaming.clone(),
                clean_local_cache.clone(),
                backup_to_desktop.clone(),
                confirmed.clone(),
                finished.clone(),
                success.clone(),
                need_reboot.clone(),
                page.clone(),
            )
        }
        UninstallPage::Finish => {
            build_finish_page(
                content,
                success.clone(),
                need_reboot.clone(),
            )
        }
    }
}

/// 构建确认页面
fn build_confirm_page(
    content: Element,
    clean_roaming: Rc<Cell<bool>>,
    clean_local_cache: Rc<Cell<bool>>,
    backup_to_desktop: Rc<Cell<bool>>,
    confirmed: Rc<Cell<bool>>,
    finished: Rc<Cell<bool>>,
    success: Rc<Cell<bool>>,
    need_reboot: Rc<Cell<bool>>,
    page: Rc<Cell<UninstallPage>>,
) -> Element {
    content
        .child(theme::title_style("清风输入法 卸载程序"))
        .child(theme::subtitle_style("确定要卸载 清风输入法 吗？"))
        .child(
            Element::col()
                .width_match()
                .bg(Color::hex(theme::CARD_BG))
                .corner(8.0)
                .padding(16)
                .spacing(12)
                .child(theme::body_style("用户数据处理："))
                .child(theme::checkbox(
                    "清除用户配置数据（输入状态、自定义短语）",
                    clean_roaming.clone(),
                ))
                .child(
                    Element::label("  %APPDATA%\\WindInput")
                        .font_size(12.0)
                        .fg(Color::hex(theme::TEXT_SECONDARY))
                        .padding_xy(24, 0),
                )
                .child(theme::checkbox(
                    "备份配置数据到桌面（推荐）",
                    backup_to_desktop.clone(),
                ))
                .child(theme::checkbox(
                    "清除本地缓存数据（词库缓存）",
                    clean_local_cache.clone(),
                ))
                .child(
                    Element::label("  %LOCALAPPDATA%\\WindInput\\cache")
                        .font_size(12.0)
                        .fg(Color::hex(theme::TEXT_SECONDARY))
                        .padding_xy(24, 0),
                )
                .child(theme::divider())
                .child(theme::checkbox(
                    "我已阅读并确认卸载",
                    confirmed.clone(),
                )),
        )
        .child(
            Element::row()
                .width_match()
                .height(50)
                .child(Element::label("").weight(1.0))
                .child(theme::primary_button("卸载").on_click(move |ctx| {
                    if !confirmed.get() {
                        // 显示提示
                        return;
                    }

                    // 执行卸载
                    let options = CleanupOptions {
                        clean_roaming: clean_roaming.get(),
                        clean_local_cache: clean_local_cache.get(),
                        backup_to_desktop: backup_to_desktop.get(),
                        ..Default::default()
                    };

                    let result = perform_uninstall(&options);
                    success.set(result.success);
                    need_reboot.set(result.need_reboot);
                    finished.set(true);
                    page.set(UninstallPage::Finish);
                    ctx.mark_dirty();
                }))
                .child(theme::secondary_button("取消").on_click(|ctx| {
                    ctx.request_close();
                })),
        )
}

/// 构建完成页面
fn build_finish_page(
    content: Element,
    success: Rc<Cell<bool>>,
    need_reboot: Rc<Cell<bool>>,
) -> Element {
    content
        .child(theme::title_style("清风输入法 卸载完成"))
        .child(
            Element::col()
                .width_match()
                .bg(Color::hex(theme::CARD_BG))
                .corner(8.0)
                .padding(16)
                .spacing(12)
                .child(if success.get() {
                    theme::success_text("✓ 清风输入法 已从您的电脑中移除")
                } else {
                    theme::error_text("✗ 卸载过程中出现错误")
                })
                .child(if need_reboot.get() {
                    theme::body_style("部分文件需要重启后才能删除")
                } else {
                    Element::label("")
                }),
        )
        .child(
            Element::row()
                .width_match()
                .height(50)
                .child(Element::label("").weight(1.0))
                .child(theme::primary_button("完成").on_click(|ctx| {
                    ctx.request_close();
                })),
        )
}
