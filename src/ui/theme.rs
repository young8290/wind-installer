#![allow(dead_code)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use windui::prelude::*;

/// 安装器主题颜色
pub const BG_COLOR: u32 = 0xF5F7FA;
pub const CARD_BG: u32 = 0xFFFFFF;
pub const PRIMARY: u32 = 0x4A90D9;
pub const PRIMARY_HOVER: u32 = 0x3A7BC8;
pub const TEXT_PRIMARY: u32 = 0x1A1A2E;
pub const TEXT_SECONDARY: u32 = 0x636E72;
pub const SUCCESS: u32 = 0x27AE60;
pub const WARNING: u32 = 0xF39C12;
pub const ERROR: u32 = 0xE74C3C;
pub const BORDER: u32 = 0xE0E0E0;

/// 标题样式
pub fn title_style(text: &str) -> Element {
    Element::label(text)
        .font_size(24.0)
        .fg(Color::hex(TEXT_PRIMARY))
        .height(34)
        .width_match()
}

/// 副标题样式
pub fn subtitle_style(text: &str) -> Element {
    Element::label(text)
        .font_size(14.0)
        .fg(Color::hex(TEXT_SECONDARY))
        .height(20)
        .width_match()
}

/// 正文样式
pub fn body_style(text: &str) -> Element {
    Element::label(text)
        .font_size(13.0)
        .fg(Color::hex(TEXT_PRIMARY))
        .height(20)
        .width_match()
}

/// 主要按钮样式
pub fn primary_button(text: &str) -> Element {
    Element::button(text)
        .width(120)
        .height(36)
}

/// 次要按钮样式
pub fn secondary_button(text: &str) -> Element {
    Element::button(text)
        .width(100)
        .height(36)
}

/// 卡片容器
pub fn card(title: &str, body: Element) -> Element {
    Element::col()
        .width_match()
        .bg(Color::hex(CARD_BG))
        .corner(8.0)
        .padding(16)
        .spacing(8)
        .child(
            Element::label(title)
                .font_size(16.0)
                .fg(Color::hex(TEXT_PRIMARY))
                .height(24)
                .width_match(),
        )
        .child(Element::divider())
        .child(body)
}

/// 进度条样式
pub fn progress_bar(value: Rc<Cell<f32>>) -> Element {
    Element::progress(value).width_match()
}

/// 不确定进度条
pub fn progress_indeterminate() -> Element {
    Element::progress_indeterminate().width_match()
}

/// 复选框样式
pub fn checkbox(text: &str, state: Rc<Cell<bool>>) -> Element {
    Element::checkbox(text, state)
}

/// 单选按钮组
pub fn radio_group(options: &[&str], selected: Rc<Cell<usize>>) -> Element {
    let mut row = Element::row().spacing(16);
    for (i, option) in options.iter().enumerate() {
        row = row.child(Element::radio(*option, selected.clone(), i));
    }
    row
}

/// 文本输入框样式
pub fn text_input(text: Rc<RefCell<String>>, placeholder: &str) -> Element {
    Element::text_input(text, placeholder).width_match()
}

/// 表单行：标签 + 控件
pub fn form_row(label: &str, control: Element) -> Element {
    Element::row()
        .width_match()
        .height(40)
        .cross(Align::Center)
        .spacing(12)
        .child(
            Element::label(label)
                .font_size(14.0)
                .fg(Color::hex(TEXT_PRIMARY))
                .width(120),
        )
        .child(control)
}

/// 分隔线
pub fn divider() -> Element {
    Element::divider()
}

/// 错误提示样式
pub fn error_text(text: &str) -> Element {
    Element::label(text)
        .font_size(13.0)
        .fg(Color::hex(ERROR))
        .height(20)
        .width_match()
}

/// 成功提示样式
pub fn success_text(text: &str) -> Element {
    Element::label(text)
        .font_size(13.0)
        .fg(Color::hex(SUCCESS))
        .height(20)
        .width_match()
}
