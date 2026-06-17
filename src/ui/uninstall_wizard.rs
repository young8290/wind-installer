use std::cell::Cell;
use std::rc::Rc;

use windui::prelude::*;

/// 卸载向导页面
const PAGE_CONFIRM: usize = 0;
const PAGE_FINISH: usize = 1;

/// 运行卸载向导
pub fn run_uninstall_wizard() {
    let current_page = Rc::new(Cell::new(PAGE_CONFIRM));
    let current_page2 = current_page.clone();
    let clean_roaming = Rc::new(Cell::new(false));
    let clean_cache = Rc::new(Cell::new(true));
    let backup_desktop = Rc::new(Cell::new(true));
    let confirmed = Rc::new(Cell::new(false));
    let success = Rc::new(Cell::new(false));
    let need_reboot = Rc::new(Cell::new(false));

    // ---- Page 0: 确认页 ----
    let page_confirm = {
        let cp = current_page.clone();
        let cr = clean_roaming.clone();
        let cc = clean_cache.clone();
        let bd = backup_desktop.clone();
        let conf = confirmed.clone();
        let suc = success.clone();
        let _reboot = need_reboot.clone();

        Element::col()
            .fill()
            .padding(32)
            .spacing(16)
            .child(
                Element::label("卸载 清风输入法")
                    .font_size(22.0)
                    .fg(Color::hex(0x1A1A2E))
                    .height(32)
                    .width_match(),
            )
            .child(
                Element::label("此向导将帮助您卸载 清风输入法。")
                    .font_size(13.0)
                    .fg(Color::hex(0x636E72))
                    .height(20)
                    .width_match(),
            )
            .child(Element::divider())
            .child(
                Element::label("用户数据处理：")
                    .font_size(14.0)
                    .fg(Color::hex(0x2D3436))
                    .height(22)
                    .width_match(),
            )
            .child(
                Element::checkbox("清除用户配置数据（输入状态、自定义短语）", cr.clone()),
            )
            .child(
                Element::label("    %APPDATA%\\WindInput")
                    .font_size(12.0)
                    .fg(Color::hex(0x999999))
                    .height(18)
                    .width_match(),
            )
            .child(Element::checkbox("备份配置数据到桌面（推荐）", bd.clone()))
            .child(Element::checkbox("清除本地缓存数据（词库缓存）", cc.clone()))
            .child(
                Element::label("    %LOCALAPPDATA%\\WindInput\\cache")
                    .font_size(12.0)
                    .fg(Color::hex(0x999999))
                    .height(18)
                    .width_match(),
            )
            .child(Element::divider())
            .child(Element::checkbox("我已确认卸载", conf.clone()))
            .child(Element::label("").weight(1.0))
            .child(
                Element::row()
                    .width_match()
                    .height(40)
                    .child(Element::label("").weight(1.0))
                    .child(
                        Element::button("卸载")
                            .width(100)
                            .height(36)
                            .on_click(move |ctx| {
                                if !conf.get() {
                                    return;
                                }
                                // TODO: 执行实际卸载
                                suc.set(true);
                                cp.set(PAGE_FINISH);
                                ctx.mark_dirty();
                            }),
                    )
                    .child(
                        Element::button("取消")
                            .width(100)
                            .height(36)
                            .on_click(|ctx| ctx.request_close()),
                    ),
            )
            .visible_when(move || current_page.get() == PAGE_CONFIRM)
    };

    // ---- Page 1: 完成页 ----
    let page_finish = {
        Element::col()
            .fill()
            .padding(32)
            .spacing(20)
            .child(
                Element::label("卸载完成")
                    .font_size(22.0)
                    .fg(Color::hex(0x1A1A2E))
                    .height(32)
                    .width_match(),
            )
            .child(
                Element::label("清风输入法 已从您的电脑中移除")
                    .font_size(14.0)
                    .fg(Color::hex(0x27AE60))
                    .height(22)
                    .width_match(),
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
            .visible_when(move || current_page2.get() == PAGE_FINISH)
    };

    // ---- 组装 ----
    let ui = Element::col()
        .fill()
        .bg(Color::hex(0xF5F7FA))
        .child(page_confirm)
        .child(page_finish);

    App::new("清风输入法 卸载程序", 500, 460)
        .bg(Color::hex(0xF5F7FA))
        .content(ui)
        .run();
}
