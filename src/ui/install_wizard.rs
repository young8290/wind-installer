use std::cell::{Cell, RefCell};
use std::rc::Rc;

use windui::prelude::*;

/// 安装向导页面
const PAGE_CONFIG: usize = 0;
const PAGE_PROGRESS: usize = 1;
const PAGE_FINISH: usize = 2;

/// 运行安装向导
pub fn run_install_wizard() {
    let current_page = Rc::new(Cell::new(PAGE_CONFIG));
    let current_page2 = current_page.clone();
    let current_page3 = current_page.clone();
    let install_dir = Rc::new(RefCell::new(r"C:\Program Files\WindInput".to_string()));
    let data_mode = Rc::new(Cell::new(0usize)); // 0=默认, 1=自定义
    let custom_data_dir = Rc::new(RefCell::new(String::new()));
    let progress = Rc::new(Cell::new(0.0f32));
    let success = Rc::new(Cell::new(false));
    let launch_settings = Rc::new(Cell::new(true));

    // ---- Page 0: 配置页 ----
    let page_config = {
        let cp = current_page.clone();
        let dir = install_dir.clone();
        let dm = data_mode.clone();
        let cdd = custom_data_dir.clone();
        let prog = progress.clone();
        let suc = success.clone();

        Element::col()
            .fill()
            .padding(32)
            .spacing(20)
            .child(
                Element::label("欢迎安装 清风输入法")
                    .font_size(22.0)
                    .fg(Color::hex(0x1A1A2E))
                    .height(32)
                    .width_match(),
            )
            .child(
                Element::label("清风输入法是一款轻量开源输入法，支持五笔、拼音等多种输入方案。")
                    .font_size(13.0)
                    .fg(Color::hex(0x636E72))
                    .height(40)
                    .width_match(),
            )
            .child(Element::divider())
            .child(
                Element::col()
                    .width_match()
                    .spacing(8)
                    .child(
                        Element::label("安装目录")
                            .font_size(14.0)
                            .fg(Color::hex(0x2D3436))
                            .height(22)
                            .width_match(),
                    )
                    .child(Element::text_input(dir.clone(), "").width_match()),
            )
            .child(
                Element::col()
                    .width_match()
                    .spacing(8)
                    .child(
                        Element::label("数据存储位置")
                            .font_size(14.0)
                            .fg(Color::hex(0x2D3436))
                            .height(22)
                            .width_match(),
                    )
                    .child(
                        Element::radio("默认位置（%APPDATA%\\WindInput，推荐）", dm.clone(), 0),
                    )
                    .child(Element::radio("自定义位置", dm.clone(), 1))
                    .child(
                        Element::text_input(cdd.clone(), "自定义数据目录路径")
                            .width_match(),
                    ),
            )
            .child(Element::label("").weight(1.0))
            .child(
                Element::row()
                    .width_match()
                    .height(40)
                    .child(Element::label("").weight(1.0))
                    .child(
                        Element::button("立即安装")
                            .width(120)
                            .height(36)
                            .on_click(move |ctx| {
                                cp.set(PAGE_PROGRESS);
                                prog.set(0.05);
                                suc.set(true);
                                ctx.mark_dirty();
                                // TODO: 后台执行实际安装，通过定时器或回调更新进度
                            }),
                    ),
            )
            .visible_when(move || current_page.get() == PAGE_CONFIG)
    };

    // ---- Page 1: 进度页 ----
    let page_progress = {
        let prog = progress.clone();

        Element::col()
            .fill()
            .padding(32)
            .spacing(20)
            .child(
                Element::label("正在安装 清风输入法")
                    .font_size(22.0)
                    .fg(Color::hex(0x1A1A2E))
                    .height(32)
                    .width_match(),
            )
            .child(
                Element::label("请稍候，正在完成安装...")
                    .font_size(13.0)
                    .fg(Color::hex(0x636E72))
                    .height(20)
                    .width_match(),
            )
            .child(Element::divider())
            .child(Element::label("").weight(1.0))
            .child(Element::progress(prog).width_match())
            .child(
                Element::label("正在释放文件...")
                    .font_size(13.0)
                    .fg(Color::hex(0x636E72))
                    .height(20)
                    .width_match(),
            )
            .child(Element::label("").weight(1.0))
            .visible_when(move || current_page2.get() == PAGE_PROGRESS)
    };

    // ---- Page 2: 完成页 ----
    let page_finish = {
        let launch = launch_settings.clone();

        Element::col()
            .fill()
            .padding(32)
            .spacing(20)
            .child(
                Element::label("安装完成")
                    .font_size(22.0)
                    .fg(Color::hex(0x1A1A2E))
                    .height(32)
                    .width_match(),
            )
            .child(
                Element::label("清风输入法 已成功安装到您的电脑")
                    .font_size(14.0)
                    .fg(Color::hex(0x27AE60))
                    .height(22)
                    .width_match(),
            )
            .child(Element::divider())
            .child(
                Element::checkbox("立即启动 清风输入法 设置", launch.clone()),
            )
            .child(Element::label("").weight(1.0))
            .child(
                Element::row()
                    .width_match()
                    .height(40)
                    .child(Element::label("").weight(1.0))
                    .child(
                        Element::button("完成")
                            .width(120)
                            .height(36)
                            .on_click(|ctx| ctx.request_close()),
                    ),
            )
            .visible_when(move || current_page3.get() == PAGE_FINISH)
    };

    // ---- 组装 ----
    let ui = Element::col()
        .fill()
        .bg(Color::hex(0xF5F7FA))
        .child(page_config)
        .child(page_progress)
        .child(page_finish);

    App::new("清风输入法 安装程序", 500, 420)
        .bg(Color::hex(0xF5F7FA))
        .content(ui)
        .run();
}
