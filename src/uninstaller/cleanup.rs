use std::path::{Path, PathBuf};

use crate::meta;
use crate::util::reboot;

/// 清理选项
#[derive(Debug, Clone)]
pub struct CleanupOptions {
    /// 安装目录
    pub install_dir: PathBuf,
    /// 是否清除用户配置数据（%APPDATA%\AppID）
    pub clean_roaming: bool,
    /// 是否清除本地缓存（%LOCALAPPDATA%\AppID\cache）
    pub clean_local_cache: bool,
    /// 清除用户配置数据前是否先备份到桌面（仅在 clean_roaming 时生效）
    pub backup_to_desktop: bool,
    /// 静默模式下的保留用户数据标志
    pub keep_user_data: bool,
}

impl Default for CleanupOptions {
    fn default() -> Self {
        let program_files =
            std::env::var("ProgramFiles").unwrap_or_else(|_| r"C:\Program Files".to_string());

        Self {
            install_dir: PathBuf::from(program_files).join(meta::app_id()),
            clean_roaming: false,
            clean_local_cache: true,
            backup_to_desktop: true,
            keep_user_data: false,
        }
    }
}

/// 解析本机实际生效的用户数据目录：优先读清单 `[datadir]` 声明的配置文件，其次默认位置。
///
/// 卸载确认对话框显示的路径与卸载真正删除的路径必须出自这里同一次解析——用户看到
/// 「将永久删除 X」时勾的是同意删 X，下游动作却是 `remove_dir_all`，两者一旦不同就是
/// 在骗用户按下不可逆的按钮。
///
/// **必须在 `UndoReceipt` 之前调用**——撤销会把这个配置文件删掉。卸载流程应在开始时
/// 解析一次并带着结果走（见 `steps::UninstallCtx::new`）。
pub fn resolve_user_data_dir() -> PathBuf {
    // 读 conf 的实现只此一份（安装向导显示的也是它），避免「界面显示 A、实际动 B」
    if let Some(dir) = crate::installer::userdata::read_datadir_conf() {
        return dir;
    }

    // 清单未声明 [datadir] 段、或配置文件缺失/为空 —— 用默认位置
    let app_data = std::env::var("APPDATA").unwrap_or_else(|_| {
        let mut p = PathBuf::from(std::env::var("USERPROFILE").unwrap_or_default());
        p.push("AppData\\Roaming");
        p.to_string_lossy().to_string()
    });
    PathBuf::from(app_data).join(meta::app_id())
}

impl CleanupOptions {
    /// 解析用户数据目录：见 [`resolve_user_data_dir`]。
    ///
    /// **必须在 `UndoReceipt` 之前调用**——撤销会把这个配置文件删掉。调用方应在
    /// 卸载开始时解析一次并带着结果走（见 `steps::UninstallCtx::new`）。
    pub fn user_data_dir(&self) -> PathBuf {
        resolve_user_data_dir()
    }

    /// 获取本地缓存目录
    pub fn local_cache_dir(&self) -> PathBuf {
        let local_app_data = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| {
            let mut p = PathBuf::from(std::env::var("USERPROFILE").unwrap_or_default());
            p.push("AppData\\Local");
            p.to_string_lossy().to_string()
        });
        PathBuf::from(local_app_data)
            .join(meta::app_id())
            .join("cache")
    }
}

// ── 用户数据目录删除守卫 ─────────────────────────────────────────────────────
//
// 数据目录路径来自 `datadir.conf`——一个**用户可编辑的明文文件**，而下游动作是
// `remove_dir_all` + 拷贝到桌面。判错一次就是不可逆的数据损失，故删除前两层校验：
// 路径形状（通用）与内容标志（清单声明 `markers` 才启用）。

/// 不得整体删除的著名目录。取当前用户/系统环境实际值，空值自动跳过。
///
/// 只禁止**等于**，不禁止其子目录——默认数据目录 `%APPDATA%\{id}` 正是 `%APPDATA%`
/// 的子目录，一并禁掉会把正常卸载也拦下。
fn forbidden_roots() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = [
        "SystemRoot",
        "ProgramFiles",
        "ProgramFiles(x86)",
        "ProgramW6432",
        "USERPROFILE",
        "APPDATA",
        "LOCALAPPDATA",
        "PUBLIC",
        "TEMP",
    ]
    .iter()
    .filter_map(|k| std::env::var_os(k).map(PathBuf::from))
    .collect();
    if let Some(profile) = std::env::var_os("USERPROFILE").map(PathBuf::from) {
        for sub in [
            "Desktop",
            "Documents",
            "Downloads",
            "Pictures",
            "Music",
            "Videos",
        ] {
            v.push(profile.join(sub));
        }
    }
    v
}

/// 路径大小写不敏感相等（去尾分隔符）。
fn path_eq_ignore_case(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| {
        p.to_string_lossy()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_lowercase()
    };
    norm(a) == norm(b)
}

/// 第一层：路径形状。纯函数（`forbidden` 由调用方注入），便于单测。
fn guard_shape(dir: &Path, forbidden: &[PathBuf]) -> Result<(), String> {
    // 绝对路径。这一条同时挡住 Windows 驱动器相对路径 `X:name`——它看着像绝对路径，
    // `is_absolute()` 却为 false，会解析到该盘当前目录上。
    if !dir.is_absolute() {
        return Err(format!("不是绝对路径: {}", dir.display()));
    }
    if dir
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(format!("路径含 `..`: {}", dir.display()));
    }
    // `D:\` 的组件是 [Prefix, RootDir] = 2；`D:\Foo` 才是 3。据此排除驱动器/共享根。
    if dir.components().count() < 3 {
        return Err(format!("过于靠近驱动器根: {}", dir.display()));
    }
    if let Some(hit) = forbidden.iter().find(|f| path_eq_ignore_case(dir, f)) {
        return Err(format!("命中受保护目录: {}", hit.display()));
    }
    Ok(())
}

/// 第二层：内容标志。`markers` 为空即不检查；空目录一律放行（没东西可丢）。
fn guard_markers(dir: &Path, markers: &[String]) -> Result<(), String> {
    if markers.is_empty() {
        return Ok(());
    }
    let mut entries = match std::fs::read_dir(dir) {
        Ok(it) => it,
        // 读不出来就别删——宁可留下也不盲删。
        Err(e) => return Err(format!("无法读取目录（{}）: {}", e, dir.display())),
    };
    if entries.next().is_none() {
        return Ok(());
    }
    if markers.iter().any(|m| dir.join(m).exists()) {
        return Ok(());
    }
    Err(format!(
        "目录非空且不含本产品标志物（{}）: {}",
        markers.join(" / "),
        dir.display()
    ))
}

/// 用户数据目录是否可安全删除。两层守卫都通过才放行。
pub fn guard_user_data_dir(dir: &Path, markers: &[String]) -> Result<(), String> {
    guard_shape(dir, &forbidden_roots())?;
    guard_markers(dir, markers)
}

/// 删除安装文件
pub fn delete_install_files(install_dir: &PathBuf) -> Result<(), String> {
    // 从 meta 动态构建二进制文件列表：进程名→.exe + ACL DLL 列表
    let mut binaries: Vec<String> = meta::process_names()
        .iter()
        .map(|n| format!("{}.exe", n))
        .collect();
    for dll in meta::acl_dlls() {
        if !binaries.iter().any(|b| b == dll) {
            binaries.push(dll.to_string());
        }
    }

    for binary in &binaries {
        let binary = binary.as_str();
        let path = install_dir.join(binary);
        if path.exists() && std::fs::remove_file(&path).is_err() {
            // 被锁定删不掉：改名让路 + 排重启删。改名成功就删改名后的，失败就直接排原路径。
            let random_suffix = rand::random::<u32>();
            let old_name = format!("{}.old_{:08x}", binary, random_suffix);
            let old_path = install_dir.join(&old_name);
            let target = if std::fs::rename(&path, &old_path).is_ok() {
                old_path
            } else {
                path
            };
            let _ = reboot::schedule_delete_on_reboot(&target);
        }
    }

    // 删除数据目录
    let data_dir = install_dir.join("data");
    if data_dir.exists() && std::fs::remove_dir_all(&data_dir).is_err() {
        eprintln!("Warning: Could not delete data directory");
    }

    // 删除卸载程序
    let uninstall_exe = install_dir.join("uninstall.exe");
    if uninstall_exe.exists() {
        let _ = std::fs::remove_file(&uninstall_exe);
    }

    // 清理备份文件（含本次改名让路产生的 .old_）：仍锁定删不掉的排重启删。
    if let Ok(entries) = std::fs::read_dir(install_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !(name.contains(".old_") || name.ends_with(".bak")) {
                continue;
            }
            let p = entry.path();
            if std::fs::remove_file(&p).is_err() {
                let _ = reboot::schedule_delete_on_reboot(&p);
            }
        }
    }

    // 尝试删除安装目录：删不掉（尚有锁定文件）就**递归**排重启删。
    //
    // 这里此前只排目录自身一条，而 `MoveFileExW` 对非空目录无效——上面单独排过队的
    // 只有 binaries 与 `.old_`/`.bak` 残留，`data/` 等数据目录下的文件一个都没排，
    // 于是重启时这个目录仍非空、删除静默失败，整棵树的重启兜底等于不存在。
    // `schedule_dir_on_reboot` 自底向上逐项处理，且能当场删掉的一律当场删、不进队列，
    // 故正常路径（`remove_dir_all` 一把成功）根本走不到它，也不会多排任何重启任务。
    if std::fs::remove_dir_all(install_dir).is_err() {
        reboot::schedule_dir_on_reboot(install_dir);
    }

    Ok(())
}

// 注册表清理已由 `steps::UndoReceipt` 按安装回执反向回放接管——它撤销的是安装时
// **实际写成**的键，而非按当前清单猜测的键，故升级后仍能清掉上一版留下的东西。

/// 清理用户数据。
///
/// `user_data_dir` 由调用方在卸载开始时解析并传入，而非在此现算——`UndoReceipt`
/// 会删掉数据目录配置文件，现算就只能拿到默认位置，用户自定义的数据目录会被漏掉。
pub fn cleanup_user_data(options: &CleanupOptions, user_data_dir: &Path) -> Result<(), String> {
    if options.keep_user_data {
        return Ok(());
    }

    let local_cache_dir = options.local_cache_dir();

    // 清除用户配置（删除前可选备份到桌面）
    if options.clean_roaming && user_data_dir.exists() {
        // 守卫在**备份之前**：路径可疑时连拷贝都不做——把一个非本产品的目录整份复制到
        // 桌面同样是伤害（体积、隐私），而且随后就要 remove_dir_all 它。
        let markers = meta::manifest()
            .datadir
            .as_ref()
            .map(|d| d.markers.as_slice())
            .unwrap_or(&[]);
        match guard_user_data_dir(user_data_dir, markers) {
            // 守卫失败只跳过这一块，**不中断整个清理**——本地缓存与 WebView2 缓存
            // 位置由我们自己算出、与这个可疑路径无关，照常清理。
            Err(reason) => eprintln!(
                "Refusing to delete user data directory: {} — 已跳过，请手动确认后自行删除",
                reason
            ),
            Ok(()) => {
                if options.backup_to_desktop {
                    // 目录名带本地时间戳，每次卸载生成唯一目录，避免覆盖历史备份
                    let backup_dir = desktop_dir().join(format!(
                        "{}_Backup_{}",
                        meta::app_id(),
                        local_timestamp()
                    ));
                    if let Err(e) = copy_dir_all(user_data_dir, &backup_dir) {
                        eprintln!("Warning: Failed to backup user data to desktop: {}", e);
                    }
                }
                if let Err(e) = std::fs::remove_dir_all(user_data_dir) {
                    eprintln!("Warning: Failed to remove user data: {}", e);
                }
            }
        }
    }

    // 清除本地缓存
    if options.clean_local_cache && local_cache_dir.exists() {
        if let Err(e) = std::fs::remove_dir_all(&local_cache_dir) {
            eprintln!("Warning: Failed to remove local cache: {}", e);
        }
    }

    // 始终清理 WebView2 缓存
    let temp_dir = std::env::temp_dir();
    let setting_cache = temp_dir.join(meta::setting_exe_stem());
    if setting_cache.exists() {
        let _ = std::fs::remove_dir_all(&setting_cache);
    }

    Ok(())
}

/// 当前用户桌面目录（%USERPROFILE%\Desktop）
fn desktop_dir() -> PathBuf {
    let user_profile = std::env::var("USERPROFILE").unwrap_or_default();
    PathBuf::from(user_profile).join("Desktop")
}

/// 本地时间戳 YYYYMMDD_HHMMSS（用于备份目录名，使每次备份唯一、不覆盖历史）
fn local_timestamp() -> String {
    use windows::Win32::System::SystemInformation::GetLocalTime;
    let st = unsafe { GetLocalTime() };
    format!(
        "{:04}{:02}{:02}_{:02}{:02}{:02}",
        st.wYear, st.wMonth, st.wDay, st.wHour, st.wMinute, st.wSecond
    )
}

/// 递归复制目录（用于卸载前备份用户数据到桌面）
fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let dest = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_all(&entry.path(), &dest)?;
        } else {
            std::fs::copy(entry.path(), &dest)?;
        }
    }
    Ok(())
}

// 以下为测试，须置于文件末尾：`#[cfg(test)] mod` 在非测试编译下整块消失，
// 把真实代码排在它后面会让人误以为文件到此为止。
#[cfg(test)]
mod guard_tests {
    use super::*;

    fn forbidden() -> Vec<PathBuf> {
        vec![
            PathBuf::from(r"C:\Windows"),
            PathBuf::from(r"C:\Users\Someone"),
            PathBuf::from(r"C:\Users\Someone\Desktop"),
        ]
    }

    /// 正常数据目录必须放行——守卫拦错了等于卸载删不干净。
    #[test]
    fn normal_data_dirs_pass() {
        for ok in [r"C:\Users\Someone\AppData\Roaming\Demo", r"D:\MyData\Demo"] {
            assert!(
                guard_shape(Path::new(ok), &forbidden()).is_ok(),
                "应放行: {ok}"
            );
        }
    }

    /// 驱动器根、UNC 共享根：一旦删下去就是整盘。
    #[test]
    fn drive_and_share_roots_rejected() {
        for bad in [r"D:\", r"C:\", r"\\server\share"] {
            assert!(
                guard_shape(Path::new(bad), &forbidden()).is_err(),
                "应拒绝: {bad}"
            );
        }
    }

    /// 驱动器相对路径 `X:name` 看着像绝对路径，实则落到该盘当前目录。
    #[test]
    fn drive_relative_and_traversal_rejected() {
        assert!(guard_shape(Path::new("C:data"), &forbidden()).is_err());
        assert!(guard_shape(Path::new(r"data\Demo"), &forbidden()).is_err());
        assert!(guard_shape(Path::new(r"D:\a\..\..\Windows"), &forbidden()).is_err());
    }

    /// 著名目录本身不可整体删；大小写与尾分隔符不应绕过。
    #[test]
    fn forbidden_roots_rejected_case_insensitively() {
        for bad in [r"C:\Windows", r"c:\windows\", r"C:\Users\Someone\Desktop"] {
            assert!(
                guard_shape(Path::new(bad), &forbidden()).is_err(),
                "应拒绝: {bad}"
            );
        }
    }

    /// 但受保护目录的**子目录**必须放行——默认数据目录正是 `%APPDATA%` 的子目录。
    #[test]
    fn children_of_forbidden_roots_pass() {
        assert!(guard_shape(Path::new(r"C:\Users\Someone\AppData"), &forbidden()).is_ok());
    }

    #[test]
    fn markers_empty_means_no_content_check() {
        let dir = tmpdir("markers_off");
        std::fs::write(dir.join("random.txt"), b"x").unwrap();
        assert!(guard_markers(&dir, &[]).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 非空但不含标志物 = 大概率不是我们的目录（用户把 conf 改成了 D:\Documents 之类）。
    #[test]
    fn non_empty_without_markers_rejected() {
        let dir = tmpdir("markers_miss");
        std::fs::write(dir.join("holiday.jpg"), b"x").unwrap();
        let markers = vec!["config.toml".to_string(), "schemas".to_string()];
        assert!(guard_markers(&dir, &markers).is_err());

        // 命中任一标志物即放行（子目录形式也算）。
        std::fs::create_dir_all(dir.join("schemas")).unwrap();
        assert!(guard_markers(&dir, &markers).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 空目录放行：没东西可丢，拦下来只会留垃圾。全新装未启动过就是这种状态。
    #[test]
    fn empty_dir_passes_marker_check() {
        let dir = tmpdir("markers_empty");
        assert!(guard_markers(&dir, &["config.toml".to_string()]).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("wind_guard_{}_{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }
}
