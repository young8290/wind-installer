# Wind Installer

清单驱动的 Windows 安装器生成器：**一个 stub 二进制 + 一份 `app.toml`，不重新编译即可为任意应用打出安装包。**

```
wind-packer build --config app.toml --stub wind-installer.exe
```

清单被嵌入归档头部，安装器与卸载器在**运行期**读取它。应用身份、界面文案、主题色、
要注册什么、要装哪些字体，全部来自这份配置——代码里不写任何具体产品的信息。

## 能力段：缺省即不执行

清单的每个能力段都遵循同一约定——**不声明就不做**。一个只有 `[app]` 身份段的普通应用，
安装计划里只剩「解压 + 可卸载」，不碰系统任何位置。

| 段 | 缺省行为 | 声明后 |
|---|---|---|
| `[ime]` | 跳过 | 注册 TSF 输入法（COM + InstallLayoutOrTip） |
| `[[font]]` | 跳过 | 装字体到 `%WINDIR%\Fonts` 并注册 |
| `[autostart]` | 不注册 | 写 `HKCU\...\Run` |
| `[[shortcut]]` | 不创建 | 开始菜单 / 桌面快捷方式 |
| `[startup]` | 装完不启动 | 装完以 `DETACHED_PROCESS` 拉起主程序 |
| `[datadir]` | 不写 | 把向导中选定的数据目录落盘供主程序读取 |
| `[strings]` | 中性文案 | 覆盖为应用自己的领域措辞 |

这是刻意的保守默认：通用安装器不擅自改用户的系统。会动注册表的行为一律由清单显式 opt-in。

## 卸载靠回执，不靠重新推断

安装过程中每个有系统副作用的步骤都写一条**回执**（registry key、字体文件、快捷方式路径……），
卸载时读回执逐条撤销。卸载器**不重新读清单**——这样即便用户升级过若干版本、清单早已变化，
卸载撤销的仍然精确是当初装下去的那些东西。

## 锁定文件与「需要重启」

升级时旧文件常被占用。安装器不会因此失败：改名让路 + `MoveFileEx(DELAY_UNTIL_REBOOT)` 排队删除，
新版照常就位；但「还需重启才能清理干净」这个事实会一路传到用户面前——交互式向导显示警示提示，
`--silent` 以 **3010**（`ERROR_SUCCESS_REBOOT_REQUIRED`）退出。

> 调用方注意：`3010` 是**成功**，不是失败。只判 `exit == 0` 会把「装好了但请重启」误报成安装失败。

安装器只提示、不代劳重启——它无从判断用户手头有没有没保存的工作。

## 快速开始

```powershell
# 1. 改 app.toml 描述你的应用（仓库自带一份演示用的虚构应用清单）
# 2. 编译 stub 并打包
.\scripts\pack.ps1
```

产物落在 `[package] output_dir`。图标与 logo 也由清单指定，打包时写入 PE 资源。

## 构建 / 测试

```
cargo build
cargo test --tests      # 集成测试是纯函数断言，无需管理员权限
cargo clippy --lib
```

`cargo test` 含 doctest 时会因 `src/archive/format.rs` 文档注释里的 ASCII 结构图报一个
既有失败，与业务逻辑无关，用 `--tests` 规避。

## 依赖与许可

本项目以 MIT 许可发布，见 [LICENSE](LICENSE)。

- GUI 层用 [windui](https://github.com/huanfeng/wind-ui-rust)。
- `vendor/editpe` 是 [editpe](https://crates.io/crates/editpe)（BSD-2-Clause）的本地修补副本：
  上游 0.2.3 的 `VersionInfo::build()` 用 UTF-8 字节数计算版本资源头部长度，含中文时结构错乱、
  Windows 读不到版本信息；副本已修为 UTF-16 码元数。许可证随副本保留在 `vendor/editpe/LICENSE`。
