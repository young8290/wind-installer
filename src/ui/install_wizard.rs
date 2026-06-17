use std::cell::{Cell, RefCell};
use std::rc::Rc;

use windui::prelude::*;

use crate::installer::config::InstallConfig;
use super::theme;

/// 安装向导页面
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum InstallPage {
    /// 安装类型选择
    TypeSelect,
    /// 数据目录设置 + 进度
    DataDirAndProgress,
    /// 安装完成
    Finish,
}

/// 安装向导状态
struct InstallWizardState {
    /// 当前页面
    page: Rc<Cell<InstallPage>>,
    /// 安装模式 (0=Standard, 1=Portable)
    install_mode: Rc<Cell<usize>>,
    /// 安装目录
    install_dir: Rc<RefCell<String>>,
    /// 使用自定义数据目录 (0=默认, 1=自定义)
    use_custom_data_dir: Rc<Cell<usize>>,
    /// 自定义数据目录
    custom_data_dir: Rc<RefCell<String>>,
    /// 自动启动
    auto_start: Rc<Cell<bool>>,
    /// 安装进度
    progress: Rc<Cell<f32>>,
    /// 当前状态文本
    status_text: Rc<RefCell<String>>,
    /// 安装完成
    finished: Rc<Cell<bool>>,
    /// 安装成功
    success: Rc<Cell<bool>>,
    /// 需要重启
    need_reboot: Rc<Cell<bool>>,
    /// 启动设置程序
    launch_settings: Rc<Cell<bool>>,
}

impl InstallWizardState {
    fn new() -> Self {
        let config = InstallConfig::default();

        Self {
            page: Rc::new(Cell::new(InstallPage::TypeSelect)),
            install_mode: Rc::new(Cell::new(0)),
            install_dir: Rc::new(RefCell::new(config.install_dir.to_string_lossy().to_string())),
            use_custom_data_dir: Rc::new(Cell::new(0)),
            custom_data_dir: Rc::new(RefCell::new(String::new())),
            auto_start: Rc::new(Cell::new(true)),
            progress: Rc::new(Cell::new(0.0)),
            status_text: Rc::new(RefCell::new(String::new())),
            finished: Rc::new(Cell::new(false)),
            success: Rc::new(Cell::new(false)),
            need_reboot: Rc::new(Cell::new(false)),
            launch_settings: Rc::new(Cell::new(true)),
        }
    }
}

/// 运行安装向导
pub fn run_install_wizard() {
    let state = InstallWizardState::new();

    let ui = build_ui(&state);

    App::new("清风输入法 安装程序", 480, 400)
        .bg(Color::hex(theme::BG_COLOR))
        .content(ui)
        .run();
}

/// 构建 UI
fn build_ui(state: &InstallWizardState) -> Element {
    let page = state.page.clone();
    let install_mode = state.install_mode.clone();
    let install_dir = state.install_dir.clone();
    let use_custom_data_dir = state.use_custom_data_dir.clone();
    let custom_data_dir = state.custom_data_dir.clone();
    let auto_start = state.auto_start.clone();
    let progress = state.progress.clone();
    let status_text = state.status_text.clone();
    let finished = state.finished.clone();
    let success = state.success.clone();
    let need_reboot = state.need_reboot.clone();
    let launch_settings = state.launch_settings.clone();

    // 页面内容
    let content = Element::col()
        .fill()
        .padding(24)
        .spacing(16);

    // 根据当前页面构建内容
    match page.get() {
        InstallPage::TypeSelect => {
            build_type_select_page(
                content,
                install_mode.clone(),
                install_dir.clone(),
                auto_start.clone(),
                page.clone(),
            )
        }
        InstallPage::DataDirAndProgress => {
            build_data_dir_progress_page(
                content,
                use_custom_data_dir.clone(),
                custom_data_dir.clone(),
                progress.clone(),
                status_text.clone(),
                finished.clone(),
                page.clone(),
            )
        }
        InstallPage::Finish => {
            build_finish_page(
                content,
                success.clone(),
                need_reboot.clone(),
                launch_settings.clone(),
            )
        }
    }
}

/// 构建安装类型选择页面
fn build_type_select_page(
    content: Element,
    install_mode: Rc<Cell<usize>>,
    install_dir: Rc<RefCell<String>>,
    auto_start: Rc<Cell<bool>>,
    page: Rc<Cell<InstallPage>>,
) -> Element {
    content
        .child(theme::title_style("清风输入法 安装程序"))
        .child(theme::subtitle_style("请选择安装方式："))
        .child(
            Element::col()
                .width_match()
                .bg(Color::hex(theme::CARD_BG))
                .corner(8.0)
                .padding(16)
                .spacing(12)
                .child(
                    Element::row()
                        .width_match()
                        .height(40)
                        .cross(Align::Center)
                        .spacing(12)
                        .child(Element::radio("标准安装（推荐）", install_mode.clone(), 0))
                        .child(Element::label("注册输入法到系统，开机自动启动").font_size(13.0).fg(Color::hex(theme::TEXT_SECONDARY))),
                )
                .child(
                    Element::row()
                        .width_match()
                        .height(40)
                        .cross(Align::Center)
                        .spacing(12)
                        .child(Element::radio("便携模式", install_mode.clone(), 1))
                        .child(Element::label("仅解压文件，不修改系统").font_size(13.0).fg(Color::hex(theme::TEXT_SECONDARY))),
                )
                .child(theme::divider())
                .child(theme::form_row(
                    "安装路径：",
                    theme::text_input(install_dir.clone(), "安装目录"),
                ))
                .child(theme::checkbox("开机自启动", auto_start.clone())),
        )
        .child(
            Element::row()
                .width_match()
                .height(50)
                .child(Element::label("").weight(1.0))
                .child(theme::primary_button("下一步").on_click(move |ctx| {
                    page.set(InstallPage::DataDirAndProgress);
                    ctx.mark_dirty();
                }))
                .child(theme::secondary_button("取消").on_click(|ctx| {
                    ctx.request_close();
                })),
        )
}

/// 构建数据目录 + 进度页面
#[allow(unused_variables)]
fn build_data_dir_progress_page(
    content: Element,
    use_custom_data_dir: Rc<Cell<usize>>,
    custom_data_dir: Rc<RefCell<String>>,
    progress: Rc<Cell<f32>>,
    status_text: Rc<RefCell<String>>,
    finished: Rc<Cell<bool>>,
    page: Rc<Cell<InstallPage>>,
) -> Element {
    content
        .child(theme::title_style("清风输入法 安装程序"))
        .child(
            Element::col()
                .width_match()
                .bg(Color::hex(theme::CARD_BG))
                .corner(8.0)
                .padding(16)
                .spacing(12)
                .child(theme::subtitle_style("数据存储位置："))
                .child(
                    Element::col()
                        .width_match()
                        .spacing(8)
                        .child(Element::radio("默认位置（推荐）", use_custom_data_dir.clone(), 0))
                        .child(Element::radio("自定义位置", use_custom_data_dir.clone(), 1))
                        .child(theme::text_input(custom_data_dir.clone(), "自定义数据目录路径")),
                )
                .child(theme::divider())
                .child(theme::subtitle_style("安装进度："))
                .child(theme::progress_bar(progress.clone()))
                .child(Element::label("准备安装...").font_size(13.0).fg(Color::hex(theme::TEXT_SECONDARY))),
        )
        .child(
            Element::row()
                .width_match()
                .height(50)
                .child(Element::label("").weight(1.0))
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
    launch_settings: Rc<Cell<bool>>,
) -> Element {
    content
        .child(theme::title_style("清风输入法 安装完成"))
        .child(
            Element::col()
                .width_match()
                .bg(Color::hex(theme::CARD_BG))
                .corner(8.0)
                .padding(16)
                .spacing(12)
                .child(if success.get() {
                    theme::success_text("✓ 清风输入法 已成功安装到您的电脑")
                } else {
                    theme::error_text("✗ 安装失败，请查看日志")
                })
                .child(if need_reboot.get() {
                    theme::body_style("部分文件需要重启后才能更新")
                } else {
                    Element::label("")
                })
                .child(theme::checkbox("立即启动 清风输入法 设置", launch_settings.clone())),
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
