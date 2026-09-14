use std::path::Path;

use winreg::enums::*;
use winreg::RegKey;

use super::config::InstallConfig;
use crate::manifest::AutoStartInfo;
use crate::meta;

/// 自启动所在的 Run 键（HKCU）
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// 卸载信息注册表路径（运行时由清单 display_name 构造）
fn uninst_key() -> String {
    format!(
        r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{}",
        meta::app_display_name()
    )
}

/// 卸载信息键路径，供安装步骤写入回执。
pub fn uninstall_key_path() -> String {
    uninst_key()
}

/// 卸载信息键是否已存在。
///
/// 用于「记录实际存在的产物」：`write_uninstall_info` 先建键、再逐个写值，
/// 中途失败会留下一个孤儿键。事后按存在性记回执，才能保证它进得了撤销清单。
pub fn uninstall_key_exists() -> bool {
    RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(uninst_key(), KEY_READ)
        .is_ok()
}

/// URL 协议键是否已存在。理由同 [`uninstall_key_exists`]。
pub fn url_protocol_exists(protocol: &str) -> bool {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(format!(r"Software\Classes\{}", protocol), KEY_READ)
        .is_ok()
}

/// 设置开机自启动，目标与参数由清单 [autostart] 段声明
pub fn set_auto_start(install_dir: &Path, info: &AutoStartInfo) -> Result<(), String> {
    let exe_path = install_dir.join(info.exe_or(meta::main_exe()));
    let exe_path_str = exe_path.to_string_lossy().to_string();

    let command = if info.args.trim().is_empty() {
        format!("\"{}\"", exe_path_str)
    } else {
        format!("\"{}\" {}", exe_path_str, info.args.trim())
    };

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let run_key = hkcu
        .open_subkey_with_flags(r"Software\Microsoft\Windows\CurrentVersion\Run", KEY_WRITE)
        .map_err(|e| format!("Failed to open Run key: {}", e))?;

    run_key
        .set_value(meta::app_id(), &command)
        .map_err(|e| format!("Failed to set auto-start: {}", e))?;

    Ok(())
}

/// 移除开机自启动（回执驱动：按记录的值名删，不依赖当前清单的 app_id）
pub fn remove_auto_start_value(value_name: &str) -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let run_key = hkcu
        .open_subkey_with_flags(RUN_KEY, KEY_WRITE)
        .map_err(|e| format!("Failed to open Run key: {}", e))?;

    run_key
        .delete_value(value_name)
        .map_err(|e| format!("Failed to remove auto-start: {}", e))?;

    Ok(())
}

/// 注册清单 app.url_protocol 声明的 URL 协议
pub fn register_url_protocol(install_dir: &Path) -> Result<(), String> {
    let setting_exe = install_dir.join(meta::setting_exe());
    let setting_exe_str = setting_exe.to_string_lossy().to_string();

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let classes_key = hkcu
        .open_subkey_with_flags(r"Software\Classes", KEY_WRITE)
        .map_err(|e| format!("Failed to open Classes key: {}", e))?;

    let protocol_key = classes_key
        .create_subkey(meta::url_protocol())
        .map_err(|e| format!("Failed to create protocol key: {}", e))?
        .0;

    protocol_key
        .set_value("", &format!("URL:{} 协议", meta::app_display_name()))
        .map_err(|e| format!("Failed to set protocol description: {}", e))?;
    protocol_key
        .set_value("URL Protocol", &"")
        .map_err(|e| format!("Failed to set URL Protocol flag: {}", e))?;

    // 设置处理命令
    let command_key = protocol_key
        .create_subkey(r"shell\open\command")
        .map_err(|e| format!("Failed to create command key: {}", e))?
        .0;

    command_key
        .set_value("", &format!("\"{}\" \"%1\"", setting_exe_str))
        .map_err(|e| format!("Failed to set command: {}", e))?;

    Ok(())
}

/// 移除 URL 协议注册（回执驱动：按记录的协议名删）
pub fn unregister_url_protocol_named(protocol: &str) -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let classes_key = hkcu
        .open_subkey_with_flags(r"Software\Classes", KEY_WRITE)
        .map_err(|e| format!("Failed to open Classes key: {}", e))?;

    classes_key
        .delete_subkey_all(protocol)
        .map_err(|e| format!("Failed to remove protocol key: {}", e))?;

    Ok(())
}

/// 写入卸载信息到注册表
pub fn write_uninstall_info(config: &InstallConfig) -> Result<(), String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let (uninst_key, _) = hklm
        .create_subkey(uninst_key())
        .map_err(|e| format!("Failed to create uninstall key: {}", e))?;

    let install_dir_str = config.install_dir.to_string_lossy().to_string();
    let uninstall_exe = config.install_dir.join("uninstall.exe");
    let uninstall_exe_str = uninstall_exe.to_string_lossy().to_string();

    uninst_key
        .set_value("DisplayName", &config.app_name)
        .map_err(|e| format!("Failed to set DisplayName: {}", e))?;
    uninst_key
        .set_value("DisplayVersion", &config.app_version)
        .map_err(|e| format!("Failed to set DisplayVersion: {}", e))?;
    uninst_key
        .set_value("Publisher", &config.publisher)
        .map_err(|e| format!("Failed to set Publisher: {}", e))?;
    uninst_key
        .set_value("InstallLocation", &install_dir_str)
        .map_err(|e| format!("Failed to set InstallLocation: {}", e))?;
    uninst_key
        .set_value(
            "UninstallString",
            &format!("\"{}\" --uninstall", uninstall_exe_str),
        )
        .map_err(|e| format!("Failed to set UninstallString: {}", e))?;
    uninst_key
        .set_value(
            "QuietUninstallString",
            &format!("\"{}\" --uninstall --silent", uninstall_exe_str),
        )
        .map_err(|e| format!("Failed to set QuietUninstallString: {}", e))?;

    // 图标（使用安装器主程序图标，第一个图标资源）
    let icon_str = format!("\"{}\",0", uninstall_exe_str);
    uninst_key
        .set_value("DisplayIcon", &icon_str)
        .map_err(|e| format!("Failed to set DisplayIcon: {}", e))?;

    // 系统设置"应用"列表所需标记
    let one: u32 = 1;
    uninst_key
        .set_value("NoModify", &one)
        .map_err(|e| format!("Failed to set NoModify: {}", e))?;
    uninst_key
        .set_value("NoRepair", &one)
        .map_err(|e| format!("Failed to set NoRepair: {}", e))?;

    // 计算安装大小
    if let Ok(size) = get_dir_size(&config.install_dir) {
        let size_kb = (size / 1024) as u32;
        uninst_key
            .set_value("EstimatedSize", &size_kb)
            .map_err(|e| format!("Failed to set EstimatedSize: {}", e))?;
    }

    Ok(())
}

/// 移除卸载信息（回执驱动：按记录的键路径删，故 display_name 改过也能清掉旧键）
pub fn remove_uninstall_key(key: &str) -> Result<(), String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    hklm.delete_subkey_all(key)
        .map_err(|e| format!("Failed to remove uninstall key: {}", e))?;

    Ok(())
}

/// 删除应用自身的注册表键（含回执与字体跟踪值）。卸载收尾调用。
pub fn remove_app_key() -> Result<(), String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    hklm.delete_subkey_all(format!("Software\\{}", meta::app_id()))
        .map_err(|e| format!("Failed to remove app key: {}", e))?;

    Ok(())
}

/// 获取目录大小（字节）
fn get_dir_size(path: &Path) -> Result<u64, String> {
    let mut total = 0u64;

    for entry in std::fs::read_dir(path).map_err(|e| format!("Failed to read directory: {}", e))? {
        let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        let metadata = entry
            .metadata()
            .map_err(|e| format!("Failed to get metadata: {}", e))?;

        if metadata.is_dir() {
            total += get_dir_size(&entry.path())?;
        } else {
            total += metadata.len();
        }
    }

    Ok(total)
}

/// 安装器运行标记的值名。
///
/// ⚠️ 这个值的内容**必须**保持 `"1"`，一个字符都不能多：读端旧版本用的是
/// `WCHAR value[8]` 的定长缓冲，值一旦超过 7 个字符，`RegQueryValueExW` 返回
/// `ERROR_MORE_DATA` 而非 `ERROR_SUCCESS`，旧读端会把「标记存在」读成「不存在」，
/// 于是安装期间照样去拉起主程序——正是这个标记要防的事。身份信息因此另起一个值名。
const INSTALLER_RUNNING: &str = "InstallerRunning";

/// 标记主人的身份：`"<pid>|<进程创建时间 FILETIME>"`。读端在应用侧
/// （`wind_tsf/include/InstallerGuard.h`，那里有解析器与完整来龙去脉）。
const INSTALLER_RUNNING_OWNER: &str = "InstallerRunningOwner";

/// 设置安装器运行标记：供应用自身的常驻组件（如被系统重新加载的 TSF DLL）识别
/// 「正在安装」，避免它在安装期间把主程序重新拉起来。读端在应用侧，不在本仓。
///
/// 连同标记一起写下**立标记的进程是谁**，这样本进程若异常死亡（panic / 被杀 /
/// 装到一半失败退出），读端能看出标记已是遗物而不再听它的。没有这条兜底时，
/// 一次中途失败的安装会让输入法**永久不工作且毫无提示**：服务再也起不来，
/// 设置页读不到配置，看起来就是「配置和自造词全没了」，连重启服务都没用，
/// 反倒是重装一次旧版本会好（那次装完整了，把标记清掉了）——issue #120 的原始描述。
pub fn set_installer_running() -> Result<(), String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let (key, _) = hklm
        .create_subkey(format!("Software\\{}", meta::app_id()))
        .map_err(|e| format!("Failed to create app key: {}", e))?;

    // ⚠️ 顺序不能反：owner 必须先落地，最后才立 InstallerRunning。
    // 「上次安装崩了 → 留下 flag=1 + 已死的旧 owner → 用户重装」是最常见的路径，
    // 若先写 flag（值没变，等于没动）再写 owner，这两条写之间读端看到的是
    // 「有标记 + 旧 owner 已死」→ 判为遗物 → 在真安装期间放行启动服务，正是闸门要防的事。
    // 反过来先写 owner，中间态是「无标记」或「有标记 + 新 owner」，两者都安全。
    //
    // 身份取不到就宁可不写，而不是写个残缺的：读端见不到 owner 会回落到「键写入时间
    // 超过 10 分钟才算陈旧」，安装期内仍受保护；而一个创建时间为 0 的身份会让读端
    // 拿它与真实进程比对、当场判定主人已死，等于安装刚开始闸门就失效了。
    match current_process_owner() {
        Some(owner) => key
            .set_value(INSTALLER_RUNNING_OWNER, &owner)
            .map_err(|e| format!("Failed to set InstallerRunningOwner: {}", e))?,
        // 上一次安装可能留下过 owner，这次取不到就得删掉，否则读端会拿旧身份去比对。
        None => {
            let _ = key.delete_value(INSTALLER_RUNNING_OWNER);
        }
    }

    key.set_value(INSTALLER_RUNNING, &"1")
        .map_err(|e| format!("Failed to set InstallerRunning: {}", e))?;

    Ok(())
}

/// 拼身份串。与读端 `InstallerGuard::ParseOwner` 严格对应：十进制、单个 `|`、无空格。
fn format_owner(pid: u32, create_ft: u64) -> String {
    format!("{}|{}", pid, create_ft)
}

/// 本进程的 `(pid, 创建时间)`。
///
/// 创建时间不可省：Windows 会复用 PID，只凭 PID 的话，一个早已死去的安装器的 PID
/// 可能正被别的进程占着，读端就会把遗物误判成「主人还活着」，闸门照样永久卡死。
///
/// 不加 `#[cfg(windows)]`：整个 `installer` 模块在 lib.rs 里已是 Windows 专属，
/// 再套一层 cfg 只会让人误以为这里有跨平台回退路径。
fn current_process_owner() -> Option<String> {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    // SAFETY: 四个出参都是本栈上的有效对象；GetCurrentProcess 返回的是伪句柄，无需关闭。
    unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        )
        .ok()?;
    }
    let ft = ((creation.dwHighDateTime as u64) << 32) | creation.dwLowDateTime as u64;
    Some(format_owner(std::process::id(), ft))
}

/// 写入安装目录，供产品自身回指。
///
/// 仅在清单配了 `[ime] system_subdir`（TSF DLL 部署到系统目录）时需要：DLL 搬离安装
/// 目录后，`GetModuleFileName` 只能取到系统副本路径，产品进程再也推不出安装目录在哪。
/// 读端在应用侧，不在本仓。
///
/// 不单独记回执——`Software\{app_id}` 整个键在卸载时由 [`remove_app_key`] 删除。
pub fn set_install_dir(install_dir: &Path) -> Result<(), String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let (key, _) = hklm
        .create_subkey(format!("Software\\{}", meta::app_id()))
        .map_err(|e| format!("Failed to create app key: {}", e))?;

    key.set_value("InstallDir", &install_dir.to_string_lossy().to_string())
        .map_err(|e| format!("Failed to set InstallDir: {}", e))?;

    Ok(())
}

/// 清除安装器运行标记（连同身份一并清掉——留着身份会让读端对着一个不存在的标记做无谓解析）
pub fn clear_installer_running() -> Result<(), String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    if let Ok(key) = hklm.open_subkey_with_flags(format!("Software\\{}", meta::app_id()), KEY_WRITE)
    {
        let _ = key.delete_value(INSTALLER_RUNNING);
        let _ = key.delete_value(INSTALLER_RUNNING_OWNER);
    }
    Ok(())
}

/// 检测已安装版本
#[allow(dead_code)]
pub fn detect_installed_version() -> Option<String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    if let Ok(key) = hklm.open_subkey_with_flags(uninst_key(), KEY_READ) {
        if let Ok(version) = key.get_value::<String, _>("DisplayVersion") {
            return Some(version);
        }
    }
    None
}

/// 获取已安装版本的卸载命令
#[allow(dead_code)]
pub fn get_uninstall_string() -> Option<String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    if let Ok(key) = hklm.open_subkey_with_flags(uninst_key(), KEY_READ) {
        if let Ok(cmd) = key.get_value::<String, _>("UninstallString") {
            return Some(cmd);
        }
    }
    None
}

#[cfg(test)]
mod owner_format_tests {
    use super::format_owner;

    /// 读端 `InstallerGuard::ParseOwner`（wind_tsf/include/InstallerGuard.h）判据的**冻结快照**。
    ///
    /// 跨语言、跨仓的约定没法靠编译器守住：读端在 C++、在另一个仓，格式改一侧不会
    /// 编译失败也不会测试失败，只会让闸门的兜底静默失灵。这里抄一份对端的判据，
    /// 把写端钉在它上面。
    ///
    /// ⚠️ 它只单向生效：**写端**若改了格式，这里会红；**读端**若改了
    /// `ParseOwner`，这个副本不会跟着变，照样全绿。要做到真双向，得把样例挪进一份
    /// 两仓共读的 golden 文本，两边各读一遍——跨仓成本不低，暂未做。
    /// 所以改 `ParseOwner` 时请人工同步本函数。
    fn parse_owner_like_reader(s: &str) -> Option<(u32, u64)> {
        let (pid_s, ft_s) = s.split_once('|')?;
        // 读端不吞前导空白、不接受正负号、不接受空段与尾部垃圾，故这里同样严格。
        if pid_s.is_empty() || ft_s.is_empty() {
            return None;
        }
        if !pid_s.bytes().all(|b| b.is_ascii_digit()) || !ft_s.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let pid: u32 = pid_s.parse().ok()?;
        let ft: u64 = ft_s.parse().ok()?;
        // 读端把 pid == 0 判为非法（PID 0 是 System Idle Process，永远「活着」，
        // 认了它闸门就永久卡死）。
        if pid == 0 {
            return None;
        }
        Some((pid, ft))
    }

    #[test]
    fn format_is_readable_by_the_reader() {
        let s = format_owner(2628, 133_712_345_678_901_234);
        assert_eq!(s, "2628|133712345678901234");
        assert_eq!(
            parse_owner_like_reader(&s),
            Some((2628, 133_712_345_678_901_234))
        );
    }

    #[test]
    fn boundary_values_survive_the_round_trip() {
        for (pid, ft) in [(1u32, 0u64), (u32::MAX, u64::MAX), (4321, 1)] {
            let s = format_owner(pid, ft);
            assert_eq!(
                parse_owner_like_reader(&s),
                Some((pid, ft)),
                "读端解析不回来: {s}"
            );
        }
    }

    /// 真调 Win32 拿本进程身份——上面几条只验字符串拼接，这条验「取到的东西是真的」。
    ///
    /// `GetProcessTimes` 的出参顺序（creation/exit/kernel/user）写错不会编译失败，
    /// 只会让创建时间变成一个不像时间的数；而读端拿它与真实进程比对必然对不上，
    /// 表现就是闸门在安装刚开始就失效——静默、且只在真安装时才发作。
    #[test]
    #[cfg(windows)]
    fn current_process_owner_is_plausible() {
        let s = super::current_process_owner().expect("本进程必然取得到自己的创建时间");
        let (pid, ft) = parse_owner_like_reader(&s).expect("写端产物必须被读端规则接受");
        assert_eq!(pid, std::process::id(), "pid 应当是本进程");
        // 2020-01-01 UTC 的 FILETIME。取到的若是 0、或 kernel/user 时间（那是个小得多的
        // 时长而非时间点），都过不了这一关。
        const FT_2020: u64 = 132_223_104_000_000_000;
        assert!(ft > FT_2020, "创建时间不像时间点: {ft}");
    }

    #[test]
    fn zero_pid_is_rejected_by_the_reader() {
        // 真实进程不会是 0，这条是为了钉住「两端对 pid==0 的处置一致」：
        // 写端若哪天真写出 0，读端会判非法并回落年龄兜底，而不是永久挡住。
        assert_eq!(parse_owner_like_reader(&format_owner(0, 123)), None);
    }
}
