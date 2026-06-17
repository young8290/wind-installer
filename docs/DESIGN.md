# Wind Installer 设计文档

## 1. 项目概述

基于 `wind-ui-rust` GUI 框架，为 WindInput（清风输入法）打造的轻量级 Windows 安装管理器。

**设计理念**：类微信输入法风格，1-2 步完成安装，同时支持通用打包能力。

### 对标 NSIS 的优势

| 维度 | NSIS | Wind Installer |
|------|------|----------------|
| 二进制体积 | ~100KB stub | ~0.5MB（含 GUI） |
| 运行时内存 | ~5MB | ~3.6MB |
| UI 定制性 | 脚本驱动，有限 | Rust 原生，完全可控 |
| 压缩算法 | Zlib/LZMA | Zstd + LZMA 双支持 |
| 安装步骤 | 3-5 步向导 | 1-2 步极简流程 |

## 2. 物理架构

安装包由两部分拼接：

```
┌─────────────────────────┐  ← 文件起始
│   Stub (installer.exe)  │  ← 自解压程序（GUI + 解压逻辑）
│   ~0.5 MB               │
├─────────────────────────┤  ← Stub 结束位置
│   Payload Header        │  ← 文件索引表
│   - Magic: "WINDPKG\0"  │
│   - 压缩类型、条目数量    │
│   - 文件路径/偏移/大小    │
├─────────────────────────┤
│   Compressed Blocks     │  ← 压缩的文件数据
├─────────────────────────┤
│   Footer (16 bytes)     │
│   - Header offset (u64) │
│   - Magic: "WINDEND\0"  │
└─────────────────────────┘  ← 文件结束
```

## 3. 归档格式详细规范

### 3.1 Footer（文件末尾 16 字节）

```
Offset  Size  Field
0       8     header_offset: u64 (LE)  — Payload Header 起始偏移
8       8     magic: "WINDEND\0"
```

### 3.2 Header

```
Offset  Size  Field
0       8     magic: "WINDPKG\0"
8       4     version: u32 (LE) — 当前版本 = 1
12      1     compression: u8  — 0=Zstd, 1=LZMA
13      4     entry_count: u32 (LE)
17      4     payload_size: u32 (LE) — 压缩数据总大小（不含 header/footer）
21      ...   entries: [Entry; entry_count]
```

### 3.3 Entry

```
Offset  Size  Field
0       2     path_len: u16 (LE)
2       path_len  path: UTF-8 bytes（相对路径，如 "wind_tsf.dll"）
+0      8     offset: u64 (LE) — 压缩数据在文件中的绝对偏移
+8      8     compressed_size: u64 (LE)
+16     8     original_size: u64 (LE)
+24     4     crc32: u32 (LE) — 原始数据的 CRC32
```

## 4. 压缩算法

### Zstd（默认）
- 解压速度极快（~500 MB/s）
- 内存可控
- 压缩级别：19（高压缩率）

### LZMA（可选）
- 压缩率最高
- 解压较慢但压缩包更小
- 适合对体积极度敏感的场景

## 5. WindInput 安装流程

### 5.1 标准安装

1. **检测环境**：64 位系统检查
2. **检测已安装版本**：读取注册表 `UninstallString`
3. **停止进程**：wind_input, wind_setting, wind_portable
4. **反注册旧 COM**：regsvr32 /u
5. **释放文件**：流式解压到安装目录
6. **设置权限**：ALL APPLICATION PACKAGES 读取执行
7. **安装字体**：HeiTiZiGen.ttf → %WINDIR%\Fonts\
8. **注册 COM**：regsvr32
9. **注册输入法**：InstallLayoutOrTip
10. **配置自启动**：HKCU Run 键
11. **注册 URL 协议**：windinput://
12. **创建快捷方式**：开始菜单
13. **写入卸载信息**：注册表 + 生成 uninstall.exe
14. **预启动服务**：wind_input.exe

### 5.2 便携安装

1. 解压文件到指定目录
2. 创建 wind_portable_mode 标记文件
3. 不修改系统注册表
4. 不注册 COM/TSF

## 6. 卸载流程

### 6.1 自删除策略

```
uninstall.exe (在安装目录)
    ↓ 复制自身到 %TEMP%\wind_uninstall_<random>.exe
    ↓ 启动新进程，退出当前进程
%TEMP%\wind_uninstall_<random>.exe
    ↓ 执行清理
    ↓ 删除原安装目录的 uninstall.exe
    ↓ 删除自身
```

### 6.2 清理步骤

1. 停止进程（3 阶段：taskkill → PowerShell → REBOOTOK）
2. 移除系统输入法（InstallLayoutOrTip + ILOT_UNINSTALL）
3. 注销 COM（regsvr32 /u）
4. 卸载系统字体
5. 删除安装文件（BackupIfLocked 模式）
6. 清理快捷方式
7. 清理注册表
8. 处理用户数据（可选）

### 6.3 用户数据清理选项

| 数据类型 | 路径 | 默认行为 |
|---------|------|---------|
| 用户配置 | %APPDATA%\WindInput | 可选删除，默认保留 |
| 本地缓存 | %LOCALAPPDATA%\WindInput\cache | 默认删除 |
| WebView2 缓存 | %TEMP%\wind_setting | 始终删除 |
| 数据目录配置 | %LOCALAPPDATA%\WindInput\datadir.conf | 跟随用户配置 |

## 7. UI 设计

### 7.1 安装界面（3 页）

**Page 1 - 安装类型选择**
```
┌─────────────────────────────────────────────┐
│  [图标]  清风输入法 安装程序                  │
│                                             │
│  请选择安装方式：                             │
│                                             │
│  ○ 标准安装（推荐）                           │
│    注册输入法到系统，开机自动启动              │
│                                             │
│  ○ 便携模式                                  │
│    仅解压文件，不修改系统                      │
│                                             │
│  安装路径: [C:\Program Files\WindInput]      │
│                                             │
│           [ 下一步 ]  [ 取消 ]               │
└─────────────────────────────────────────────┘
```

**Page 2 - 数据目录 + 安装进度**
```
┌─────────────────────────────────────────────┐
│  [图标]  清风输入法 安装程序                  │
│                                             │
│  数据存储位置：                               │
│  ● 默认位置 (%APPDATA%\WindInput)           │
│  ○ 自定义位置: [________________] [浏览]     │
│                                             │
│  正在安装...                                 │
│  ████████████████░░░░░░░░  68%              │
│                                             │
│  正在释放文件: wind_input.exe                │
│                                             │
│           [ 取消 ]                           │
└─────────────────────────────────────────────┘
```

**Page 3 - 完成**
```
┌─────────────────────────────────────────────┐
│  [图标]  清风输入法 安装完成                  │
│                                             │
│  ✓ 清风输入法 已成功安装到您的电脑           │
│                                             │
│  [x] 立即启动 清风输入法 设置                │
│                                             │
│           [ 完成 ]                           │
└─────────────────────────────────────────────┘
```

### 7.2 卸载界面（2 页）

**Page 1 - 确认 + 用户数据选项**
```
┌─────────────────────────────────────────────┐
│  [图标]  清风输入法 卸载程序                  │
│                                             │
│  确定要卸载 清风输入法 吗？                   │
│                                             │
│  用户数据处理：                               │
│  [ ] 清除用户配置数据（输入状态、自定义短语）  │
│      %APPDATA%\WindInput                    │
│  [x] 备份配置数据到桌面（推荐）               │
│  [x] 清除本地缓存数据（词库缓存）             │
│      %LOCALAPPDATA%\WindInput\cache         │
│                                             │
│  [x] 我已阅读并确认卸载                      │
│                                             │
│           [ 卸载 ]  [ 取消 ]                 │
└─────────────────────────────────────────────┘
```

**Page 2 - 完成**
```
┌─────────────────────────────────────────────┐
│  [图标]  清风输入法 卸载完成                  │
│                                             │
│  ✓ 清风输入法 已从您的电脑中移除              │
│                                             │
│  [ ] 立即重启电脑（如有文件被锁定）           │
│                                             │
│           [ 完成 ]                           │
└─────────────────────────────────────────────┘
```

## 8. 项目结构

```
wind-installer/
├── Cargo.toml
├── build.rs                   # 嵌入 UAC Manifest + 图标
├── assets/
│   ├── app.manifest           # requireAdministrator
│   └── installer.ico
├── src/
│   ├── main.rs                # 入口：检测模式（安装/卸载/打包）
│   ├── lib.rs
│   ├── archive/               # 归档格式与压缩
│   │   ├── mod.rs
│   │   ├── format.rs          # 结构定义
│   │   ├── reader.rs          # 流式读取解压
│   │   └── writer.rs          # 打包写入
│   ├── installer/             # 安装逻辑
│   │   ├── mod.rs
│   │   ├── config.rs          # 安装配置
│   │   ├── extract.rs         # 文件释放
│   │   ├── registry.rs        # 注册表操作
│   │   ├── shortcut.rs        # 快捷方式
│   │   ├── font.rs            # 字体安装
│   │   ├── acl.rs             # 权限设置
│   │   ├── process.rs         # 进程管理
│   │   └── ime.rs             # TSF/COM 注册
│   ├── uninstaller/           # 卸载逻辑
│   │   ├── mod.rs
│   │   ├── cleanup.rs         # 清理逻辑
│   │   └── selfdelete.rs      # 自删除
│   ├── ui/                    # 安装界面
│   │   ├── mod.rs
│   │   ├── theme.rs           # 主题
│   │   ├── install_wizard.rs  # 安装向导
│   │   └── uninstall_wizard.rs# 卸载向导
│   └── util/                  # 工具函数
│       ├── mod.rs
│       ├── admin.rs           # UAC 权限
│       ├── single.rs          # 单实例
│       └── path.rs            # 路径工具
├── tools/
│   └── packer/                # 打包工具
│       ├── Cargo.toml
│       └── main.rs
└── docs/
    └── DESIGN.md
```

## 9. 依赖选型

```toml
[dependencies]
windui = { path = "../wind-ui-rust" }
zstd = "0.13"                  # Zstd 解压
xz2 = "0.1"                    # LZMA 解压
windows = { version = "0.58", features = [...] }
winreg = "0.52"                # 注册表封装
mslnk = "0.1"                  # 快捷方式
crc32fast = "1.4"              # CRC32 校验
serde = { version = "1", features = ["derive"] }
toml = "0.8"                   # 配置文件
clap = { version = "4", features = ["derive"] }

[build-dependencies]
embed-resource = "2.4"         # 嵌入 Manifest
```

## 10. Anti-Virus 对策

| 策略 | 实现 |
|------|------|
| 代码签名 | signtool Authenticode 签名 |
| Payload 混淆 | 简单 XOR 混淆避免特征码 |
| 行为透明 | UI 反馈所有操作 |
| 标准 API | 使用官方 windows crate |

## 11. 开发路线

| 阶段 | 内容 | 工作量 |
|------|------|--------|
| Phase 1 | 归档格式 + 流式解压 + 打包工具 | 2-3 天 |
| Phase 2 | 安装逻辑（文件 + 注册表 + TSF） | 3-4 天 |
| Phase 3 | GUI 界面（基于 windui） | 2-3 天 |
| Phase 4 | 卸载器 + 自删除 | 2 天 |
| Phase 5 | 测试 + 签名集成 | 2-3 天 |
