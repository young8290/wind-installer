use std::path::PathBuf;

use windui::app::App;
use windui::core::EventCtx;
use windui::geometry::Color;
use windui::signal::signal;
use windui::spec::Align;
use windui::ui::Element;
// 仅无边框标题栏用得到；非 frameless 构建下整段标题栏不存在，导入也不该存在。
#[cfg(feature = "frameless")]
use windui::ui::WindowButtonKind;

use windui::prelude::Sender;

use super::theme;
use crate::installer::step::Reporter;
use crate::meta;

const PAGE_CONFIRM: usize = 0;
const PAGE_PROGRESS: usize = 1;
const PAGE_FINISH: usize = 2;

enum UninstallMsg {
    Status(String),
    Finished {
        ok: bool,
        detail: String,
        /// 有文件被占用删不掉，已排进重启删除队列。
        need_reboot: bool,
    },
}

/// 把 [`Reporter`] 事件转成 UI 通道消息。卸载页只有状态文字、无进度条，
/// 故步内细粒度进度也走 Status（如「反注册 COM ...」逐条显示）。
struct GuiReporter {
    tx: Sender<UninstallMsg>,
}

impl Reporter for GuiReporter {
    fn step_begin(&mut self, _index: usize, _total: usize, name: &str) {
        let _ = self.tx.send(UninstallMsg::Status(name.to_string()));
    }
    fn step_progress(&mut self, detail: &str, _fraction: f32) {
        let _ = self.tx.send(UninstallMsg::Status(detail.to_string()));
    }
    fn log(&mut self, _msg: &str) {}
    fn warn(&mut self, msg: &str) {
        eprintln!("Warning: {}", msg);
    }
}

/// `%LOCALAPPDATA%\{app.id}` 下跟随「删除用户数据」勾选的那一组，渲染成一句可读文本。
/// 清单未声明 `[localdata]`（或 `state_files` 为空）即返回空串。
///
/// 勾选行的附注与确认弹窗**都**从这里取。弹窗才是「同意」的闸口——用户按下
/// 「确定删除」前读的最后一句话——两处各写一遍迟早漂移，而漂移掉的那一半恰好是
/// 不可逆动作的说明（AGENTS.md 规则 5：提示文案与不可逆动作之间不能有第二份推导）。
///
/// 公开是为了让 `tests/wizard_data_dir.rs` 能直接对账「界面说的 = 清单声明的」，
/// 那个测试的职责本就是向导初值、卸载提示、实际删除三方一致。
pub fn state_files_clause() -> String {
    let files: &[String] = meta::localdata().map_or(&[], |l| l.state_files.as_slice());
    if files.is_empty() {
        return String::new();
    }
    format!(
        "%LOCALAPPDATA%\\{} 下的 {}",
        meta::app_id(),
        files.join("、")
    )
}

pub fn run_uninstall_wizard() {
    // ---- 运行期窗口尺寸（来自清单；非无边框模式高度 -40 补偿系统标题栏）----
    let (win_w, base_h) = meta::uninstall_win();
    let win_h = if cfg!(feature = "frameless") {
        base_h
    } else {
        base_h - 40
    };
    let title = format!("{} 卸载程序", meta::app_display_name());

    // ---- 状态（Signal<T> 是 Copy 句柄，move 闭包自动复制，无需 clone 样板）----
    let current_page = signal(PAGE_CONFIRM);
    let clean_roaming = signal(false);
    let clean_cache = signal(true);
    let backup_to_desktop = signal(true);
    let confirmed = signal(false);
    let finish_success = signal(false);
    let finish_error = signal(String::new());
    // 卸载完了但有文件删不掉（已排重启删除队列）——完成页据此提示重启。
    let finish_reboot = signal(false);
    let status_text = signal(String::from("正在准备卸载..."));
    let install_dir = signal(detect_install_dir());
    let show_delete_confirm = signal(false);

    // ---- 跨线程进度通道（on_message 在 UI 线程调用，可直接写 Signal）----
    let mut app = App::new(title.clone(), win_w, win_h);
    // windui 0.12 起 on_message 收 `&mut EventCtx`（宿主能力通道：toast / 对话框）。
    // 0.14 起通道回调里的 close / close_forced / window_op 被**丢弃**：通道挂在 App 级、
    // 借哪棵树排空是实现细节，「关掉哪个窗口」本就不确定。要关窗请走窗口自身的交互。
    // 这里只写 Signal 切页，用不上 ctx。
    let tx = app.channel::<UninstallMsg>(move |_ctx, msg| match msg {
        UninstallMsg::Status(s) => status_text.set(s),
        UninstallMsg::Finished {
            ok,
            detail,
            need_reboot,
        } => {
            finish_success.set(ok);
            finish_reboot.set(need_reboot);
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
        .child(Element::image_bytes(meta::logo()).size(52, 52).corner(13.0))
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

    // 两个勾选各自会动 %LOCALAPPDATA%\{app.id} 下的一组条目（清单 [localdata] 声明）。
    // 这两串说明**由同一份清单生成**，不是另写一遍：界面上说会删什么、卸载就删什么，
    // 是 AGENTS.md 规则 5 的要求——用户是照着这行字按下不可逆按钮的。清单没声明
    // [localdata] 时两串都是空，缓存勾选整行也不显示（勾了什么都不做比没有更糟）。
    let local_root = format!("%LOCALAPPDATA%\\{}", meta::app_id());
    let cache_dirs: &[String] = meta::localdata().map_or(&[], |l| l.cache_dirs.as_slice());
    let has_cache_entries = !cache_dirs.is_empty();
    // 勾选行附注与确认弹窗都从 state_files_clause() 取，不各写一遍——见该函数文档。
    let state_clause = state_files_clause();
    let confirm_clause = state_clause.clone();
    // 措辞必须是**条件式**的。写成「另将删除 …」是在陈述事实，而这一组只有在上面那个
    // 危险勾选被勾上时才删——没勾就不删，界面却说了要删，同样是规则 5 的违反，只是
    // 方向相反（说了不做，而不是做了不说）。指名是哪个勾选、而不是靠「上一项」这种
    // 位置指代：文案与那个勾选取自同一个 s_user_data_label()，中间再插几行也不会错位。
    let state_note = if state_clause.is_empty() {
        String::new()
    } else {
        format!(
            "勾选「{}」时，还将一并删除 {}",
            meta::s_user_data_label(),
            state_clause
        )
    };

    // 删除用户数据：危险勾选行（受控 on_toggle：未勾时弹应用内确认对话框，已勾时直接取消）
    // 括号内是本机实际生效的数据目录（自定义过就显示自定义路径），与确认对话框、
    // 与真正删除的目录同出一次解析。
    let delete_data_row = Element::checkbox(
        format!(
            "{}（{}）",
            meta::s_user_data_label(),
            crate::uninstaller::cleanup::resolve_user_data_dir().display()
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
            Element::label(format!(
                "即将从您的电脑中卸载 {}，请确认：",
                meta::app_display_name()
            ))
            .font_size(13.0)
            .fg(Color::hex(theme::text_secondary()))
            .width_match(),
        )
        .child(delete_data_row)
        .child(
            Element::label(state_note.clone())
                .width_match()
                .font_size(11.0)
                .fg(Color::hex(theme::text_muted()))
                .visible_when(move || !state_note.is_empty()),
        )
        .child(
            // 0.12 的启用轴分三形态，绑 Signal 走 `_signal` 后缀那一支
            // （`enabled(bool)` 现在是静态形态，传 Signal 会 E0308）。
            Element::checkbox("删除前备份配置数据到桌面（推荐）", backup_to_desktop)
                .enabled_signal(clean_roaming),
        )
        .child(
            Element::checkbox(
                format!(
                    "{}（{} 下的 {}）",
                    meta::s_cache_label(),
                    local_root,
                    cache_dirs.join("、")
                ),
                clean_cache,
            )
            .visible_when(move || has_cache_entries),
        )
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
                        .enabled_signal(confirmed)
                        .on_click(move |_ctx: &mut EventCtx| {
                            current_page.set(PAGE_PROGRESS);

                            let options = crate::uninstaller::cleanup::CleanupOptions {
                                install_dir: install_dir.get(),
                                clean_roaming: clean_roaming.get(),
                                clean_local_cache: clean_cache.get(),
                                backup_to_desktop: backup_to_desktop.get(),
                                keep_user_data: false,
                            };

                            let tx = tx.clone();
                            std::thread::spawn(move || {
                                // 与静默路径共用同一份计划，仅 Reporter 不同
                                let plan = crate::uninstaller::plan::plan_uninstall();
                                let mut reporter = GuiReporter { tx: tx.clone() };
                                let mut ctx =
                                    crate::uninstaller::steps::UninstallCtx::new(&options);

                                let outcome = crate::installer::step::run_plan(
                                    &plan,
                                    &mut ctx,
                                    &mut reporter,
                                );

                                // 卸载计划无致命步骤，一路尽力而为；删不掉的东西
                                // 转化为「需要重启」（与 perform_uninstall 同一口径）。
                                let need_reboot = ctx.need_reboot
                                    || outcome.map(|o| o.need_reboot).unwrap_or(false);

                                let _ = crate::installer::registry::clear_installer_running();

                                tx.send(UninstallMsg::Finished {
                                    ok: true,
                                    detail: String::new(),
                                    need_reboot,
                                })
                                .ok();
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
                    Element::label_signal(status_text)
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
        // 卸载完成，但有文件正被占用删不掉（已排入系统的重启删除队列）。
        // 卸载场景下这条比安装场景更要紧：残留会让用户以为"没卸干净"而反复手动删。
        .child(
            Element::col()
                .width_match()
                .spacing(4)
                .cross(Align::Center)
                .visible_when(move || finish_success.get() && finish_reboot.get())
                .child(Element::leaf().height(10))
                .child(
                    Element::label("部分文件正被占用，需重启电脑才能彻底清除")
                        .font_size(12.0)
                        .fg(Color::hex(theme::warning()))
                        .width_match()
                        .text_align(Align::Center),
                )
                .child(
                    Element::label("已排入系统清理队列，重启后将自动删除")
                        .font_size(11.0)
                        .fg(Color::hex(theme::text_muted()))
                        .width_match()
                        .text_align(Align::Center),
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
                    Element::label_signal(finish_error)
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
            .child(
                Element::window_button(WindowButtonKind::Minimize)
                    .fg(Color::hex(theme::text_secondary())),
            )
            .child(
                Element::window_button(WindowButtonKind::Close)
                    .fg(Color::hex(theme::text_secondary())),
            )
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
                    // 路径必须取本机实际生效的数据目录，而非默认位置模板：自定义过
                    // 数据目录的用户看到 `%APPDATA%\{id}` 会以为删的是别处，而下游
                    // 动作是 remove_dir_all —— 提示与实际删除必须是同一个路径。
                    Element::label(meta::s_delete_data_confirm(
                        &crate::uninstaller::cleanup::resolve_user_data_dir().to_string_lossy(),
                    ))
                    .font_size(13.0)
                    .fg(Color::hex(theme::text_secondary()))
                    .width_match(),
                )
                .child(
                    // 弹窗才是「同意」的闸口——用户按下「确定删除」前读的最后一句话。
                    // 清单给的那段正文只说了数据目录，而勾上这个框现在还会删
                    // %LOCALAPPDATA% 那一组；少说这一句，提示与不可逆动作之间就又有了
                    // 第二份推导（AGENTS.md 规则 5）。文案与勾选行附注同源，不另取。
                    Element::label(if confirm_clause.is_empty() {
                        String::new()
                    } else {
                        format!("另含 {}。", confirm_clause)
                    })
                    .font_size(12.0)
                    .fg(Color::hex(theme::text_muted()))
                    .width_match()
                    .visible_when(move || !state_files_clause().is_empty()),
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
        .renderer(super::renderer())
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
