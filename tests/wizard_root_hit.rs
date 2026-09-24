//! 卸载向导根节点：窗口客户区比清单尺寸大时，底部按钮仍须点得到。
//!
//! 缺陷现场（GH#145，v121/v122，Win10）：卸载页「开始卸载」「取消」看得见、点不动。
//! 用户的数据目录是 `C:\Users\Administrator.DESKTOP-XXXX\AppData\Roaming\WindInput`，
//! 附注折成两行，确认页比清单高度 420 高出一截，按钮被挤到 y>420。
//!
//! 按钮之所以**还画得出来**：windui 无边框窗口按「清单尺寸 + 系统标题栏/边框」建外框，
//! 再用 WM_NCCALCSIZE 把整个外框划给客户区，于是客户区比 420 高出约 40dp；绘制不按父
//! 节点裁剪，溢出的按钮落在这段额外客户区里照样可见。而命中测试**要求点落在每一层父
//! 节点的矩形内**——内容列被写死成 `.size(w, 420)`，420 以下的点在这一层就被拒，
//! 永远到不了按钮。画在上面、点不到。
//!
//! 所以根节点必须铺满实际客户区，而不是写死清单尺寸。本测试按实际情形构造：
//! 布局尺寸比清单高、按钮压在清单高度之下。
//!
//! 文件名不含 "install"/"setup"：命中 UAC 安装器检测启发式的测试二进制普通权限起不来。

#![cfg(windows)]

use windui::core::Tree;
use windui::geometry::{Point, Size};
use windui::signal::signal;
use windui::ui::Element;

use wind_installer::manifest::AppManifest;
use wind_installer::{meta, ui::uninstall_wizard};

const MANIFEST: &str = r#"
[app]
id           = "Demo"
display_name = "Demo App"
version      = "1.0.0"
publisher    = "Demo Inc"
main_exe     = "demo.exe"

[ui]
uninstall_win = { w = 480, h = 420 }
"#;

const BUTTON_W: i32 = 123;
const BUTTON_H: i32 = 42;

#[test]
fn bottom_button_below_manifest_height_is_clickable() {
    meta::init(AppManifest::from_toml_bytes(MANIFEST.as_bytes()).expect("解析清单失败"));
    let (win_w, win_h) = meta::uninstall_win();
    // 无边框窗口的实际客户区：清单尺寸 + 系统标题栏与边框（96dpi 下约 39dp）。
    let client = Size::new(win_w, win_h + 40);

    // 内容比清单高度高：一段 440 高的占位把按钮挤到 [440, 482) —— 越过 420，
    // 但仍在客户区（460）之内看得见的那一段 [440, 460)。
    let content = Element::col().child(Element::leaf().height(440)).child(
        Element::button("开始卸载")
            .width(BUTTON_W)
            .height(BUTTON_H)
            .on_click(|_| {}),
    );
    let dialog = Element::dialog(signal(false), Element::col());
    let root = uninstall_wizard::window_root(content, dialog);

    let mut tree = Tree::new();
    let id = root.build(&mut tree);
    tree.root = Some(id);
    tree.layout_root(client, &mut windui::text::LineAwareTextEngine);

    let p = Point::new(BUTTON_W / 2, 450);
    let hit = tree.hit_test(p).map(|n| tree.abs_bounds(n));
    assert!(
        hit.is_some_and(|b| b.w == BUTTON_W && b.h == BUTTON_H),
        "按钮画在 y=440..482、客户区高 {}，点 {:?} 却命中 {:?}",
        client.h,
        p,
        hit
    );
}
