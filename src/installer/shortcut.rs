use std::path::{Path, PathBuf};

use crate::manifest::{expand_placeholders, ShortcutInfo, ShortcutLocation};
use crate::meta;

/// `create_shortcuts` 实际创建了什么 + 失败信息。
///
/// 不用 `Result<_, String>` 是因为回执必须记下「已经做成的部分」：一个条目失败时若
/// 整体返回 Err 而丢掉已创建的那些，卸载后它们会永久留在桌面/开始菜单，
/// 且指向已被删除的 exe。
#[derive(Debug, Default)]
pub struct CreatedShortcuts {
    /// 创建成功的 .lnk 绝对路径。
    pub links: Vec<PathBuf>,
    /// 若创建过开始菜单快捷方式，则为那个文件夹（卸载时整个删除）。
    pub start_menu_dir: Option<PathBuf>,
    pub errors: Vec<String>,
}

/// 创建清单 [[shortcut]] 段声明的快捷方式。目标不存在的条目跳过（不报错）。
/// 逐条尝试，返回实际创建成功的那些。
pub fn create_shortcuts(install_dir: &Path, items: &[ShortcutInfo]) -> CreatedShortcuts {
    let mut created = CreatedShortcuts::default();

    // 占位符替换值（使能力段配置对 dev/release 等变体通用）
    let (main_exe, setting_exe, display, app_id) = (
        meta::main_exe(),
        meta::setting_exe(),
        meta::app_display_name(),
        meta::app_id(),
    );
    let expand = |s: &str| expand_placeholders(s, main_exe, setting_exe, display, app_id);

    for item in items {
        // {setting_exe} 在未配置设置程序时展开为空 → 跳过该快捷方式
        let target_rel = expand(&item.target);
        if target_rel.trim().is_empty() {
            continue;
        }
        let target = install_dir.join(&target_rel);
        if !target.exists() {
            continue;
        }

        let dir = location_dir(item.location);
        if let Err(e) = std::fs::create_dir_all(&dir) {
            created
                .errors
                .push(format!("无法创建快捷方式目录 {:?}: {}", dir, e));
            continue;
        }

        let link_path = dir.join(format!("{}.lnk", expand(item.effective_name())));
        match create_shortcut(
            &target,
            &link_path,
            &install_dir.to_string_lossy(),
            &expand(item.effective_description()),
        ) {
            Ok(()) => {
                // 标识写失败不撤掉快捷方式：它照样能打开程序，只是通知不归它管。
                // 记成错误让日志看得见，链接本身照常进回执。
                let aumid = expand(&item.app_user_model_id);
                if !aumid.trim().is_empty() {
                    if let Err(e) = set_app_user_model_id(&link_path, aumid.trim()) {
                        created.errors.push(e);
                    }
                }
                if item.location == ShortcutLocation::StartMenu {
                    created.start_menu_dir = Some(dir);
                }
                created.links.push(link_path);
            }
            Err(e) => created.errors.push(e),
        }
    }

    created
}

// 删除快捷方式由 `uninstaller::steps::UndoReceipt` 按回执记录的绝对路径逐个撤销，
// 无需在此按清单反推——清单改过名字也不会留下孤儿 .lnk。

fn create_shortcut(
    target: &Path,
    link_path: &Path,
    working_dir: &str,
    description: &str,
) -> Result<(), String> {
    let mut link =
        mslnk::ShellLink::new(target).map_err(|e| format!("无法创建快捷方式对象: {}", e))?;

    link.set_working_dir(Some(working_dir.to_string()));
    link.set_name(Some(description.to_string()));

    link.create_lnk(link_path)
        .map_err(|e| format!("无法保存快捷方式 {:?}: {}", link_path, e))?;

    Ok(())
}

/// `System.AppUserModel.ID`（propkey.h 的 PKEY_AppUserModel_ID）。就地定义而不开
/// `Win32_Storage_EnhancedStorage` feature：为一个常量拉进一整个模块的绑定不划算。
const PKEY_APP_USER_MODEL_ID: windows::Win32::Foundation::PROPERTYKEY =
    windows::Win32::Foundation::PROPERTYKEY {
        fmtid: windows::core::GUID::from_u128(0x9f4c2855_9f79_4b39_a8d0_e1d42de1d5f3),
        pid: 5,
    };

/// 给已存在的 .lnk 写入 AppUserModelID。
///
/// mslnk 只会写 Shell Link 的基本字段，不支持属性存储，故这里用系统的 ShellLink COM
/// 对象重新打开刚生成的文件、写属性、存回。值必须是 `VT_LPWSTR`：windows 库自带的
/// `PROPVARIANT::from(&str)` 产出的是 `VT_BSTR`，官方示例与 `InitPropVariantFromString`
/// 都用前者，不赌 shell 对后者的兼容。
fn set_app_user_model_id(link: &Path, id: &str) -> Result<(), String> {
    use std::mem::ManuallyDrop;

    use windows::core::{Interface, HSTRING};
    use windows::Win32::System::Com::StructuredStorage::{
        PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0, PROPVARIANT_0_0_0,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, IPersistFile, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED, STGM_READWRITE,
    };
    use windows::Win32::System::Variant::VT_LPWSTR;
    use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
    use windows::Win32::UI::Shell::{IShellLinkW, SHStrDupW, ShellLink};

    unsafe {
        // 安装步骤跑在后台线程上，没人替它初始化 COM。已被别处以 MTA 初始化时这里
        // 返回 RPC_E_CHANGED_MODE：照用即可（ShellLink 两种套间都支持），但不能配对
        // CoUninitialize。
        let inited = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();

        // 闭包收住全部 COM 对象：它们必须在 CoUninitialize 之前析构
        let result = (|| -> windows::core::Result<()> {
            let shell_link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
            let file: IPersistFile = shell_link.cast()?;
            let path = HSTRING::from(link.as_os_str());
            file.Load(&path, STGM_READWRITE)?;

            // 字符串由 CoTaskMem 分配，PROPVARIANT 析构时 PropVariantClear 负责释放
            let text = SHStrDupW(&HSTRING::from(id))?;
            let value = PROPVARIANT {
                Anonymous: PROPVARIANT_0 {
                    Anonymous: ManuallyDrop::new(PROPVARIANT_0_0 {
                        vt: VT_LPWSTR,
                        wReserved1: 0,
                        wReserved2: 0,
                        wReserved3: 0,
                        Anonymous: PROPVARIANT_0_0_0 { pwszVal: text },
                    }),
                },
            };

            let store: IPropertyStore = shell_link.cast()?;
            store.SetValue(&PKEY_APP_USER_MODEL_ID, &value)?;
            store.Commit()?;
            file.Save(&path, true)
        })();

        if inited {
            CoUninitialize();
        }
        result.map_err(|e| format!("无法给快捷方式 {:?} 写入应用标识: {}", link, e))
    }
}

fn location_dir(loc: ShortcutLocation) -> PathBuf {
    match loc {
        ShortcutLocation::StartMenu => start_menu_dir(),
        ShortcutLocation::Desktop => desktop_dir(),
    }
}

/// 全局开始菜单下的应用文件夹。
fn start_menu_dir() -> PathBuf {
    let program_data =
        std::env::var("ProgramData").unwrap_or_else(|_| r"C:\ProgramData".to_string());
    PathBuf::from(program_data)
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs")
        .join(meta::start_menu_folder())
}

/// 公共桌面（对所有用户可见）。
fn desktop_dir() -> PathBuf {
    let public = std::env::var("PUBLIC").unwrap_or_else(|_| r"C:\Users\Public".to_string());
    PathBuf::from(public).join("Desktop")
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use windows::core::{Interface, BSTR, HSTRING};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, IPersistFile, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED, STGM_READ,
    };
    use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
    use windows::Win32::System::Variant::{VARENUM, VT_LPWSTR};
    use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};

    use super::*;

    /// wine 的 ShellLink 接受属性写入但不落盘，回读必然为空——那是 wine 的缺口，
    /// 不是这里的缺陷。真 Windows（CI）上照常断言。
    fn running_under_wine() -> bool {
        unsafe {
            GetModuleHandleW(windows::core::w!("ntdll.dll"))
                .ok()
                .and_then(|h| GetProcAddress(h, windows::core::s!("wine_get_version")))
                .is_some()
        }
    }

    /// 读回 (属性类型, 标识, 目标路径)。
    fn read_back(link: &Path) -> (VARENUM, String, String) {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let shell_link: IShellLinkW =
                CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).unwrap();
            let file: IPersistFile = shell_link.cast().unwrap();
            file.Load(&HSTRING::from(link.as_os_str()), STGM_READ)
                .unwrap();
            let store: IPropertyStore = shell_link.cast().unwrap();
            let value = store.GetValue(&PKEY_APP_USER_MODEL_ID).unwrap();
            let vt = value.Anonymous.Anonymous.vt;
            let text = BSTR::try_from(&value)
                .map(|b| b.to_string())
                .unwrap_or_default();
            let mut buf = [0u16; 260];
            shell_link
                .GetPath(&mut buf, std::ptr::null_mut(), 0)
                .unwrap();
            let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            (vt, text, String::from_utf16_lossy(&buf[..len]))
        }
    }

    /// 写进去的必须是 shell 能读回来的 `VT_LPWSTR`，且原有的目标路径不能被这次
    /// 「打开—写属性—存回」弄丢。
    #[test]
    fn app_user_model_id_round_trips_through_the_shell() {
        if running_under_wine() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("wind_aumid_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = std::env::current_exe().unwrap();
        let link = dir.join("demo.lnk");

        create_shortcut(&target, &link, &dir.to_string_lossy(), "demo").unwrap();
        set_app_user_model_id(&link, "com.demo.app").unwrap();

        let (vt, text, path) = read_back(&link);
        assert_eq!(vt, VT_LPWSTR);
        assert_eq!(text, "com.demo.app");
        assert!(
            path.eq_ignore_ascii_case(&target.to_string_lossy()),
            "目标路径被改写: {path}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
