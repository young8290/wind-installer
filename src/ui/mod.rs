pub mod theme;
pub mod install_wizard;
pub mod uninstall_wizard;

/// 是否使用 D2D 硬件加速渲染。
/// 默认开启；以下任一条件触发软渲染回退：
///   - 环境变量 `WIND_SOFT_RENDER` 非空且不为 `"0"`
///   - 命令行包含 `--soft-render`
pub fn is_accelerated() -> bool {
    let env_soft = std::env::var("WIND_SOFT_RENDER")
        .map(|v| !v.is_empty() && v != "0")
        .unwrap_or(false);
    let arg_soft = std::env::args().any(|a| a == "--soft-render");
    !env_soft && !arg_soft
}
