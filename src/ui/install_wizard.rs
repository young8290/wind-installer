use std::path::PathBuf;

use windui::app::App;
use windui::core::EventCtx;
use windui::geometry::Color;
use windui::platform::PickDialog;
use windui::signal::signal;
use windui::spec::Align;
use windui::ui::{Element, WindowButtonKind};

use windui::prelude::Sender;

use crate::installer::step::Reporter;
use crate::installer::InstallMode;
use crate::meta;
use super::theme;

const PAGE_CONFIG: usize = 0;
const PAGE_PROGRESS: usize = 1;
const PAGE_FINISH: usize = 2;

enum ProgressMsg {
    Status(String),
    /// 总体进度 ∈ [0,1]，由 [`GuiReporter`] 按「已完成步骤 + 步内比例」折算。
    Progress(f32),
    Finished(bool, String),
}

/// 把 [`Reporter`] 事件转成 UI 通道消息，并同步写安装日志。
///
/// 进度按步骤折算而非按文件数——旧实现只统计解压文件数，导致注册类步骤全部
/// 挤在 99% 处不动。
struct GuiReporter {
    tx: Sender<ProgressMsg>,
    logger: crate::util::log::InstallLogger,
    index: usize,
    total: usize,
}

impl GuiReporter {
    fn emit(&self, fraction_within_step: f32) {
        let overall = (self.index as f32 + fraction_within_step) / self.total.max(1) as f32;
        let _ = self.tx.send(ProgressMsg::Progress(overall.clamp(0.0, 0.99)));
    }
}

impl Reporter for GuiReporter {
    fn step_begin(&mut self, index: usize, total: usize, name: &str) {
        self.index = index;
        self.total = total;
        self.logger.log(name);
        let _ = self.tx.send(ProgressMsg::Status(name.to_string()));
        self.emit(0.0);
    }

    fn step_progress(&mut self, detail: &str, fraction: f32) {
        self.logger.log(&format!("  {}", detail));
        self.emit(fraction);
    }

    fn log(&mut self, msg: &str) {
        self.logger.log(msg);
    }

    fn warn(&mut self, msg: &str) {
        self.logger.log(&format!("  警告: {}", msg));
    }
}

pub fn run_install_wizard() {
    // ---- 运行期窗口尺寸 / 标题（来自清单）----
    let (win_w, win_h) = meta::install_win();
    let title = meta::window_title();

    // ---- 状态（Signal<T> 是 Copy 句柄，move 闭包自动复制，无需 clone 样板）----
    let is_fresh_install = crate::installer::registry::detect_installed_version().is_none();
    let current_page      = signal(PAGE_CONFIG);
    let install_mode      = signal(0usize);
    let install_dir       = signal(default_install_dir());
    let install_dir_portable = signal(default_portable_dir());
    let data_dir          = signal(default_data_dir());
    let progress_text     = signal(String::from("正在准备安装..."));
    let progress_value    = signal(0.0f32);
    let finish_success    = signal(false);
    let finish_error      = signal(String::new());
    let config_error      = signal(String::new());
    let agreed            = signal(false);

    // ---- 跨线程进度通道（on_message 在 UI 线程调用，可直接写 Signal）----
    let mut app = App::new(title.as_str(), win_w, win_h);
    let tx = app.channel::<ProgressMsg>(move |msg| match msg {
        ProgressMsg::Status(s)   => progress_text.set(s),
        ProgressMsg::Progress(f) => progress_value.set(f),
        ProgressMsg::Finished(ok, detail) => {
            progress_value.set(1.0);
            finish_success.set(ok);
            if !ok {
                finish_error.set(detail);
            }
            current_page.set(PAGE_FINISH);
        }
    });

    // ============================================================
    //  PAGE 0：配置页
    // ============================================================
    let page_config = Element::col()
        .fill()
        .weight(1.0)
        .padding_xy(40, 0)
        .spacing(8)
        .visible_when(move || current_page.get() == PAGE_CONFIG)
        // ── 品牌区（Logo + 应用名 + 版本）─────────────────────────
        .child(Element::leaf().weight(1.0))
        .child(
            Element::col()
                .width_match()
                .spacing(6)
                .cross(Align::Center)
                .child(
                    Element::image_bytes(meta::logo())
                        .size(72, 72)
                        .corner(18.0),
                )
                .child(
                    Element::label(meta::app_display_name())
                        .width_match()
                        .font_size(22.0)
                        .fg(Color::hex(theme::text_primary()))
                        .text_align(Align::Center),
                )
                .child(
                    Element::label(meta::app_version())
                        .width_match()
                        .font_size(12.0)
                        .fg(Color::hex(theme::text_muted()))
                        .text_align(Align::Center),
                ),
        )
        .child(Element::leaf().height(10))
        // 安装目录（行高固定 34，内容随模式切换，避免布局抖动）
        .child(
            Element::row()
                .width_match()
                .height(34)
                .spacing(8)
                .cross(Align::Center)
                .child(
                    Element::label("安装目录")
                        .width(64)
                        .font_size(12.0)
                        .fg(Color::hex(theme::text_secondary()))
                )
                .child(
                    Element::text_input(install_dir, "安装路径")
                        .weight(1.0)
                        .height(34)
                        .visible_when(move || install_mode.get() == 0)
                )
                .child(
                    Element::button("更改")
                        .height(34)
                        .visible_when(move || install_mode.get() == 0)
                        .on_click(move |ctx: &mut EventCtx| {
                            ctx.request_pick_folder(
                                folder_pick_dialog("选择安装目录", &install_dir.get()),
                                move |path| {
                                    if let Some(path) = path {
                                        install_dir.set(
                                            finalize_picked_folder(path)
                                                .to_string_lossy()
                                                .to_string(),
                                        );
                                    }
                                },
                            );
                        })
                )
                .child(
                    Element::text_input(install_dir_portable, "安装路径")
                        .weight(1.0)
                        .height(34)
                        .visible_when(move || install_mode.get() == 1)
                )
                .child(
                    Element::button("更改")
                        .height(34)
                        .visible_when(move || install_mode.get() == 1)
                        .on_click(move |ctx: &mut EventCtx| {
                            ctx.request_pick_folder(
                                folder_pick_dialog("选择安装目录", &install_dir_portable.get()),
                                move |path| {
                                    if let Some(path) = path {
                                        install_dir_portable.set(
                                            finalize_picked_folder(path)
                                                .to_string_lossy()
                                                .to_string(),
                                        );
                                    }
                                },
                            );
                        })
                )
        )
        // 数据目录（行高固定 34；便捷模式显示说明文字）
        .child(
            Element::row()
                .width_match()
                .height(34)
                .spacing(8)
                .cross(Align::Center)
                .child(
                    Element::label("数据目录")
                        .width(64)
                        .font_size(12.0)
                        .fg(Color::hex(theme::text_secondary()))
                )
                .child(
                    Element::text_input(data_dir, meta::s_data_dir_hint())
                        .weight(1.0)
                        .height(34)
                        .enabled(signal(is_fresh_install))
                        .visible_when(move || install_mode.get() == 0)
                )
                .child(
                    Element::button("更改")
                        .height(34)
                        .enabled(signal(is_fresh_install))
                        .visible_when(move || install_mode.get() == 0)
                        .on_click(move |ctx: &mut EventCtx| {
                            ctx.request_pick_folder(
                                folder_pick_dialog("选择数据目录", &data_dir.get()),
                                move |path| {
                                    if let Some(path) = path {
                                        data_dir.set(
                                            finalize_picked_folder(path)
                                                .to_string_lossy()
                                                .to_string(),
                                        );
                                    }
                                },
                            );
                        })
                )
                .child(
                    Element::label("便捷模式不配置数据目录")
                        .weight(1.0)
                        .font_size(12.0)
                        .fg(Color::hex(theme::text_muted()))
                        .visible_when(move || install_mode.get() == 1)
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
                        .child(Element::radio("标准安装（推荐）", install_mode, 0))
                        .child(Element::radio("便捷模式", install_mode, 1))
                )
                .child(
                    Element::label(meta::s_mode_hint())
                        .font_size(11.0)
                        .fg(Color::hex(theme::text_muted()))
                        .width_match()
                )
        )
        // 用户协议
        .child(
            Element::row()
                .cross(Align::Center)
                .spacing(4)
                .child(Element::checkbox("我已阅读并同意", agreed))
                .child(if meta::agreement_url().is_empty() {
                    Element::label(meta::s_agreement_text())
                        .font_size(13.0)
                        .fg(Color::hex(theme::text_secondary()))
                } else {
                    Element::link(meta::s_agreement_text())
                        .url(meta::agreement_url())
                })
        )
        // 校验错误提示（仅未勾协议时可见）
        .child(
            Element::label_rc(config_error)
                .font_size(11.0)
                .fg(Color::hex(theme::error()))
                .align(Align::Center)
        )
        // 安装按钮（align=Center 使其在父 col 中水平居中）
        .child(
            Element::button("立即安装")
                .width(300)
                .height(48)
                .corner(8.0)
                .bg(Color::hex(theme::accent()))
                .fg(Color::hex(0xFFFFFF))
                .align(Align::Center)
                .enabled(agreed)
                .on_click(move |_ctx: &mut EventCtx| {
                    config_error.set(String::new());
                    progress_value.set(0.0);
                    current_page.set(PAGE_PROGRESS);

                    let install_dir_val = if install_mode.get() == 0 {
                        expand_env_path(&install_dir.get())
                    } else {
                        expand_env_path(&install_dir_portable.get())
                    };
                    let data_dir_val   = expand_env_path(&data_dir.get());
                    let mode           = if install_mode.get() == 0 {
                        InstallMode::Standard
                    } else {
                        InstallMode::Portable
                    };

                    let tx = tx.clone();
                    std::thread::spawn(move || {
                        let mut logger = crate::util::log::InstallLogger::new();
                        logger.log(&format!("安装目录: {:?}", install_dir_val));
                        logger.log(&format!("安装模式: {:?}", mode));
                        let log_path = logger.path.to_string_lossy().to_string();

                        let mut config = crate::installer::config::InstallConfig::default();
                        config.install_dir = install_dir_val;
                        // 数据目录始终取向导里的值（其默认值即 default_data_dir()），
                        // config 是步骤读取数据目录的唯一入口
                        config.custom_data_dir = Some(data_dir_val);
                        config.use_custom_data_dir = true;

                        let mut archive = match crate::archive::ArchiveReader::open_current_exe() {
                            Ok(a) => a,
                            Err(e) => {
                                let msg = format!("无法打开安装数据: {}", e);
                                logger.log_error(&msg);
                                tx.send(ProgressMsg::Finished(false, msg)).ok();
                                return;
                            }
                        };

                        // 与静默路径共用同一份计划，仅 Reporter 不同
                        let plan = crate::installer::plan::plan_install(crate::meta::manifest(), mode);
                        let mut reporter = GuiReporter {
                            tx: tx.clone(),
                            logger,
                            index: 0,
                            total: plan.len(),
                        };

                        let result = crate::installer::step::run_plan(
                            &plan,
                            &config,
                            mode,
                            is_fresh_install,
                            &mut archive,
                            &mut reporter,
                        );

                        match result {
                            Ok(_) => {
                                reporter.log("=== 安装完成 ===");
                                tx.send(ProgressMsg::Finished(true, log_path)).ok();
                            }
                            Err(e) => {
                                // 致命失败：清除标志，否则宿主进程将永久停摆
                                let _ = crate::installer::registry::clear_installer_running();
                                reporter.log(&format!("=== 安装失败: {} ===", e));
                                tx.send(ProgressMsg::Finished(false, e)).ok();
                            }
                        }
                    });
                })
        )
        .child(Element::leaf().weight(1.0));

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
                    Element::label("正在安装中，请稍候...")
                        .font_size(14.0)
                        .fg(Color::hex(theme::text_primary()))
                )
                .child(
                    Element::progress(progress_value)
                        .width_match()
                        .height(6)
                        .corner(3.0)
                )
                .child(
                    Element::label_rc(progress_text)
                        .font_size(12.0)
                        .fg(Color::hex(theme::text_secondary()))
                        .width_match()
                        .text_align(Align::Center)
                )
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
        // 安装成功
        .child(
            Element::col()
                .width_match()
                .spacing(8)
                .cross(Align::Center)
                .visible_when(move || finish_success.get())
                .child(
                    Element::label("✓")
                        .font_size(44.0)
                        .fg(Color::hex(theme::success()))
                )
                .child(
                    Element::label("安装完成")
                        .font_size(18.0)
                        .fg(Color::hex(theme::text_primary()))
                )
                .child(
                    Element::label(format!("{} 已准备就绪，可以开始使用", meta::app_display_name()))
                        .font_size(13.0)
                        .fg(Color::hex(theme::text_secondary()))
                        .width_match()
                        .text_align(Align::Center)
                )
        )
        // 安装失败
        .child(
            Element::col()
                .width_match()
                .spacing(8)
                .cross(Align::Center)
                .visible_when(move || !finish_success.get())
                .child(
                    Element::label("✗")
                        .font_size(44.0)
                        .fg(Color::hex(theme::error()))
                )
                .child(
                    Element::label("安装失败")
                        .font_size(18.0)
                        .fg(Color::hex(theme::text_primary()))
                )
                .child(
                    Element::label_rc(finish_error)
                        .font_size(12.0)
                        .fg(Color::hex(theme::error()))
                        .width_match()
                        .text_align(Align::Center)
                )
        )
        .child(Element::leaf().weight(1.0))
        // 完成按钮
        .child(
            Element::button("完 成")
                .width(200)
                .height(48)
                .corner(8.0)
                .bg(Color::hex(theme::accent()))
                .fg(Color::hex(0xFFFFFF))
                .align(Align::Center)
                .on_click(move |_ctx: &mut EventCtx| {
                    std::process::exit(0);
                })
        )
        .child(Element::leaf().height(20));

    // ============================================================
    //  无边框自定义标题栏：浅色，与窗口背景同色，左侧标题右侧按钮
    // ============================================================
    #[cfg(feature = "frameless")]
    let title_bar = Element::row()
        .width_match()
        .height(36)
        .cross(Align::Center)
        .bg(Color::hex(theme::bg_primary()))
        .window_drag()
        .child(Element::leaf().width(14))
        .child(
            Element::label(title.clone())
                .font_size(12.0)
                .fg(Color::hex(theme::text_secondary())),
        )
        .child(Element::leaf().weight(1.0))
        .child(Element::window_button(WindowButtonKind::Minimize).fg(Color::hex(theme::text_secondary())))
        .child(Element::window_button(WindowButtonKind::Close).fg(Color::hex(theme::text_secondary())));

    // ============================================================
    //  组装根节点
    // ============================================================
    let root = Element::col()
        .size(win_w, win_h)
        .bg(Color::hex(theme::bg_primary()));

    #[cfg(feature = "frameless")]
    let root = root.child(title_bar);

    let root = root
        .child(page_config)
        .child(page_progress)
        .child(page_finish);

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

fn default_install_dir() -> String {
    use winreg::enums::*;
    use winreg::RegKey;
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let key_path = format!(
        r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{}",
        crate::meta::app_display_name()
    );
    if let Ok(key) = hklm.open_subkey_with_flags(&key_path, KEY_READ) {
        if let Ok(dir) = key.get_value::<String, _>("InstallLocation") {
            if !dir.is_empty() {
                return dir;
            }
        }
    }
    crate::meta::default_install_path()
}

fn default_portable_dir() -> String {
    crate::meta::default_portable_path()
}

fn default_data_dir() -> String {
    crate::meta::default_data_path()
}

/// 展开路径中的 %VAR% 环境变量占位符。
fn expand_env_path(s: &str) -> std::path::PathBuf {
    let mut result = s.to_string();
    for (var, fallback) in &[
        ("ProgramFiles", r"C:\Program Files"),
        ("APPDATA",      r"C:\Users\Default\AppData\Roaming"),
        ("LOCALAPPDATA", r"C:\Users\Default\AppData\Local"),
        ("USERPROFILE",  r"C:\Users\Default"),
    ] {
        let token = format!("%{}%", var);
        if result.contains(&token) {
            let val = std::env::var(var).unwrap_or_else(|_| fallback.to_string());
            result = result.replace(&token, &val);
        }
    }
    std::path::PathBuf::from(result)
}

/// 构建选目录用的 `PickDialog`：起始目录取当前配置路径（`%VAR%` 展开后）最近的
/// 已存在祖先目录，而不是让 Shell 用它自己记住的上次访问位置——那个位置完全不
/// 可控（可能是网络共享/已拔出的移动盘/云盘同步目录），既是"偶发打开卡顿"的
/// 诱因之一，也会让"更改"按钮的默认落点跟用户已经填好的路径对不上。
fn folder_pick_dialog(title: &str, current_path: &str) -> PickDialog {
    let mut dialog = PickDialog::new().title(title);
    let expanded = expand_env_path(current_path);
    let mut probe = Some(expanded.as_path());
    while let Some(p) = probe {
        if p.exists() {
            dialog = dialog.directory(p);
            break;
        }
        probe = p.parent();
    }
    dialog
}

/// 用户选完目录后的收尾：若选的目录名不是 APP_ID，自动追加子目录。
fn finalize_picked_folder(path: PathBuf) -> PathBuf {
    if path.file_name().map(|n| n != crate::meta::app_id()).unwrap_or(true) {
        path.join(crate::meta::app_id())
    } else {
        path
    }
}
