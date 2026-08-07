## 变更说明

<!-- 简要描述这个 PR 做了什么 -->

## 变更类型

- [ ] Bug 修复
- [ ] 新功能
- [ ] 重构 / 代码优化
- [ ] 文档更新
- [ ] 构建 / CI 相关
- [ ] 其他（请说明）

## 相关 Issue

<!-- 如有关联的 Issue，请填写，例如：Fixes #123 -->

## 测试情况

<!-- 描述你做了哪些测试来验证这个变更 -->

- [ ] `cargo fmt --all -- --check` 通过（逻辑修改与格式化修改分开提交）
- [ ] `cargo build --locked --bin wind-installer --bin wind-uninstaller` 通过
- [ ] `cargo build --locked --bin wind-packer --features packer` 通过
- [ ] `cargo test --locked --tests` 通过（注意 `--tests`，见 CONTRIBUTING）
- [ ] `cargo clippy --locked --lib -- -D warnings` 通过
- [ ] 已在 Windows 10/11 实测（涉及注册表、字体、快捷方式、TSF、文件占用等系统副作用时**必须**勾选）

<!-- 涉及系统副作用时，请说明测了哪些场景：全新安装 / 升级覆盖 / 卸载 / 卸载后重装 -->

## 通用性检查

<!-- 本仓库是通用安装器生成器，以下几条是硬性约束 -->

- [ ] 未在 Rust 代码中引入任何具体应用的信息（产品名、GUID/CLSID、DLL 名、领域词、固定安装路径），此类信息一律走 `meta::manifest()`
- [ ] 新增的清单字段带 `#[serde(default)]`，旧清单仍可解析
- [ ] 新增的能力段**缺省即不执行**，并在 `tests/step_plan.rs` 补了"缺省不入计划、声明才入计划"的断言
- [ ] 新增的、会改动系统的行为默认关闭，由清单显式 opt-in

## 副作用与卸载

- [ ] 新增的有系统副作用的步骤已 push 回执（`ctx.receipt`），卸载可逐条撤销
- [ ] 新增的可能删不掉文件的路径，失败分支已经过 `util::reboot` 记账（不是只 `eprintln!`）
- [ ] 未修改静态链接 MSVC CRT 的相关配置（`scripts/verify-static-crt.ps1` 是发布门禁）

<!-- 以上各项如不适用，请直接写「不适用」并说明原因 -->

## 检查清单

- [ ] 提交信息遵循 [Conventional Commits](https://www.conventionalcommits.org/) 规范
- [ ] 复杂或反直觉的决策已在代码注释中写明**为什么**
- [ ] 已阅读 [贡献指南](../CONTRIBUTING.md) 与 [AGENTS.md](../AGENTS.md)
- [ ] 首次贡献已签署 [CLA](../CLA.md)
