# AGENTS.md — wind-installer

## 本仓库的定位（不可动摇的目标）

**wind-installer 是一个「通用 Windows 安装器生成器」，不是清风输入法的专用安装器。**

同一个 stub 二进制（`wind-installer.exe`）配上不同的清单（`app.toml`）即可为**任意应用**打出安装包，无需重新编译：

```
wind-packer build --config <app.toml> --stub wind-installer.exe
```

清单被嵌入归档头部，由安装/卸载器**运行期读取**。因此：

> **一切与具体应用相关的东西都必须写在清单里，绝不能硬编码进 Rust 代码。**

清风输入法只是**第一个使用者**，不是特权使用者。任何"因为我们是输入法所以直接写死"的改动，都是在把通用安装器退化成定制安装器——这是明确要避免的方向。

## 硬性规则

1. **禁止在 Rust 代码里出现**：产品名（"清风输入法"）、具体 GUID/CLSID、DLL 名、"输入法/词库"等领域词、固定安装路径。这些一律从 `meta::manifest()` 取。
   - 领域中性由测试守护：`tests/ui_defaults.rs::defaults_are_domain_neutral`。
2. **能力段缺省 = 该能力不执行**。清单不声明某段（`[ime]`/`[[font]]`/`[autostart]`/…）→ 对应 `Step` 不入计划。一个只有 `[app]` 身份的普通应用，计划里只剩"解压 + 可卸载"。由 `tests/step_plan.rs::minimal_manifest_plans_no_capability_steps` 守护。
3. **有系统副作用的步骤必须写回执**（`ctx.receipt`），否则卸载撤销不掉。卸载是回执驱动的，不重新读清单。
4. **保守默认**：新增的、会改系统（尤其注册表）的行为，默认关闭，由打包器在清单里显式 opt-in。通用安装器不擅自动系统。

## 新增一种能力的标准套路

1. `src/manifest.rs`：给对应段/结构加字段（`#[serde(default)]` 保证旧清单兼容）。
2. `src/installer/<capability>.rs`：实现具体逻辑，只从 `meta`/传入的清单结构取参数。
3. `src/installer/steps.rs`：加一个 `impl Step<InstallCtx>`，有副作用就 push 回执。
4. `src/installer/plan.rs`：加一行门控，按清单字段决定是否入计划。
5. `tests/step_plan.rs`：加断言——「缺省不入计划、声明才入计划」（参考 `datadir_conf_is_opt_in`、`ime_residue_sweep_is_opt_in`）。

`plan::plan_install` 是"装什么、按什么顺序装"的唯一真相；GUI 与静默路径共用它，仅 Reporter 不同。不要在步骤内部再按模式/产品分支。

## 锁定文件与「需要重启」

升级时旧文件常被占用（典型是仍被 `ctfmon` 加载的 TSF DLL）。安装器不会因此失败：解压走「改名 `.old_xxxxxxxx` 让路 + `MoveFileEx(DELAY_UNTIL_REBOOT)` 排队」，新版照常就位。但**这个事实必须一路传到用户面前**。

- **记账入口**：`util::reboot::schedule_delete_on_reboot` / `record_pending`。任何「当前删不掉」的路径都要经它们过一遍——它维护一本进程级账本。底层 IO 处没有 `Step` 上下文也没有 `Reporter`，逐层改签名会污染 `extract_entry` 等无关 API，故用账本承接。
- **汇总点**：`step::run_plan` 收尾时读账本，与「步骤失败 + `needs_reboot_on_failure()`」合并成 `RunOutcome::need_reboot`。步骤自身不要去读账本。
- **新增删除逻辑的义务**：凡是可能删不掉文件的新代码，失败分支必须记账，否则「需要重启」的结论会漏判。**不要**只 `eprintln!`——安装器是 `windows_subsystem = "windows"` 的无控制台 GUI 进程，那等于丢弃。

`need_reboot` 的三种去向，一处都不能少：

| 路径 | 行为 |
|---|---|
| 交互式向导 / `--quiet` | 完成页显示 warning 色提示；**quiet 模式放弃自动退出**，窗口留给用户亲手关闭 |
| `--silent`（无界面） | 以 **3010**（`ERROR_SUCCESS_REBOOT_REQUIRED`）退出，并写 `%TEMP%\wind_installer_args.log` |
| 卸载（两条路径同上） | 同上 |

> **调用方契约**：`--silent` 的 `3010` 是**成功**，不是失败。宿主（wind-setting）若只判 `exit == 0` 会把「装好了但请重启」误报成安装失败。

安装器**只提示、不代劳重启**：它无从判断用户手头有没有没保存的工作。提示用 `theme::warning()` 而非 `error()`——红色会让用户以为装失败而去重装，而重装解决不了任何问题。

## 变体隔离（dev / release）

输入法有 dev（CLSID `{99C2DEB0-…}`）与 release（CLSID `{99C2EE30-…}`）两套 GUID，设计上要能共存。凡是按 CLSID/profile 操作系统的逻辑（注册、反注册、残留清扫），**只用清单里的 GUID 推导目标键**——这样正式版包只碰 EE 系列、dev 包只碰 DEB 系列，隔离性来自"配置即身份"，代码里**不写任何变体判断分支**。

## 配置文件的位置（重要）

- **生产清单**：`../WindInput/config/app.toml`（归属 **WindInput 仓库**）。这才是清风输入法真正打包用的配置。
- **本仓库的 `app.toml`**：只是**示例/参考**，演示清单写法，不是产品配置。改产品行为要改 WindInput 那份。

## 构建 / 测试

```
cargo build
cargo test --tests      # 跳过 doctest；集成测试是纯函数断言，无需管理员权限
cargo clippy --lib
```

注：`cargo test`（含 doctest）目前会因 `src/archive/format.rs` 文档注释里的 ASCII 图而报一个既有 doctest 失败，与业务逻辑无关，用 `--tests` 规避。
