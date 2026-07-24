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
