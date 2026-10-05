//! 系统运行时依赖（`[[prerequisite]]`）：检测、缺失时运行随包的引导程序。
//!
//! 检测与「要不要提示」分开：安装步骤 [`super::steps::EnsurePrerequisites`] 负责代装，
//! 向导在计划跑完后**再检测一遍**决定完成页提示什么——代装可能失败、超时，也可能
//! 清单只声明了「只提示」，以装完那一刻的真实状态为准，而不是转述步骤的返回值。

use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use winreg::enums::*;
use winreg::RegKey;

use crate::manifest::{probe_value_present, PrerequisiteInfo, RegistryHive, RegistryProbe};

/// 引导程序最长等多久。WebView2 这类在线引导程序要现场下载上百 MB，慢网下几分钟
/// 很正常；但不设上限的话，一个卡死的引导程序会让安装向导永远停在进度页。
/// 超时后不杀它（它可能只是慢），安装照常收尾，完成页按真实检测结果提示。
const BOOTSTRAP_TIMEOUT: Duration = Duration::from_secs(20 * 60);

/// 该依赖当前是否已装。
pub fn is_satisfied(p: &PrerequisiteInfo) -> bool {
    p.detect.iter().any(probe)
}

/// 清单里当前仍缺失的依赖，按声明顺序给出下标。
pub fn missing(list: &[PrerequisiteInfo]) -> Vec<usize> {
    list.iter()
        .enumerate()
        .filter(|(_, p)| !is_satisfied(p))
        .map(|(i, _)| i)
        .collect()
}

fn probe(d: &RegistryProbe) -> bool {
    let Some((hive, sub)) = d.split_hive() else {
        return false;
    };
    let root = RegKey::predef(match hive {
        RegistryHive::LocalMachine => HKEY_LOCAL_MACHINE,
        RegistryHive::CurrentUser => HKEY_CURRENT_USER,
    });
    let value = root
        .open_subkey_with_flags(sub, KEY_READ)
        .and_then(|k| k.get_value::<String, _>(&d.value))
        .ok();
    probe_value_present(value.as_deref())
}

/// 缺失时运行引导程序，跑完再检测一遍。
///
/// `Ok` 表示跑完后已检测到；`Err` 带上给日志看的原因。未声明 `installer`、或随包的
/// 引导程序不在盘上时也返回 `Err`（不是静默成功）——那种包装出来依赖就是缺的。
pub fn ensure(install_dir: &Path, p: &PrerequisiteInfo) -> Result<(), String> {
    if is_satisfied(p) {
        return Ok(());
    }
    if p.installer.trim().is_empty() {
        return Err(format!("未检测到 {}（清单未配置引导程序，只提示）", p.name));
    }
    let exe = install_dir.join(&p.installer);
    if !exe.exists() {
        return Err(format!(
            "未检测到 {}，且引导程序 {:?} 不在包里",
            p.name, exe
        ));
    }

    let mut child = Command::new(&exe)
        .args(p.args.split_whitespace())
        .spawn()
        .map_err(|e| format!("无法启动 {} 的引导程序: {}", p.name, e))?;

    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() >= BOOTSTRAP_TIMEOUT => {
                return Err(format!(
                    "{} 的引导程序 {} 分钟内没有结束，不再等待（它仍在后台运行）",
                    p.name,
                    BOOTSTRAP_TIMEOUT.as_secs() / 60
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(500)),
            Err(e) => return Err(format!("等待 {} 的引导程序失败: {}", p.name, e)),
        }
    };

    if is_satisfied(p) {
        Ok(())
    } else {
        Err(format!(
            "{} 的引导程序已退出（{}），但仍未检测到它",
            p.name, status
        ))
    }
}
