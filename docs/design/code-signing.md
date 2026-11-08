# Moodflow Windows 代码签名

2026-11-08：用户尚未购买签名证书。本仓已接入签名及验签门禁，当前本机产物仍是未签名开发构建；不能将上游签名记录视为 Moodflow 的签名验收。

## 接入

购买代码签名证书后，按签发机构说明配置硬件令牌或云签名客户端，使证书及私钥操作可通过 Windows `CurrentUser/My` 或 `LocalMachine/My` 证书存储访问。脚本通过指纹精确选择证书，不使用名称模糊匹配。

复制 `scripts/sign.local.ps1.example` 为已被 Git 忽略的 `scripts/sign.local.ps1`，填写公开的证书指纹和存储位置。也可设置 `MOODFLOW_SIGN_THUMBPRINT`、`MOODFLOW_SIGN_STORE` 环境变量。私钥、令牌 PIN、服务凭据不写入命令行或仓库。

```powershell
.\scripts\sign.ps1 -Status
.\scripts\dev.ps1 sign 8
```

支持通过 Windows 加密提供程序访问的证书。只有专用 API、无法映射证书存储的云服务需要另加适配器；未预设服务商、会话时长或 CI 限制。将来自动签名应在受控的发布环境中配置凭据，不从不可信 PR 代码访问签名服务。

## 签名顺序

1. 编译核心、Hub、TSF 和其他随包程序；先签名并验证这些 PE 文件。
2. 安装器执行 `pack.ps1 -PrepOnly`，给卸载器写入 Moodflow 图标、版本资源和清单。
3. 签名并验证加工后的 `uninstall.exe`。
4. `pack.ps1 -SkipBuild -SkipPrep` 封装归档，保证已经签名的卸载器字节不再变化。
5. 签名并验证最终 Setup。
6. 最后计算 Setup 的 SHA-256 和长度，生成升级元数据。

便携包在压缩前签名其内部的程序。对已压缩安装包只补签外壳，不能证明内部程序已签名。

每次签名使用 SHA-256 和 RFC3161 时间戳。成功返回前必须验证签名状态、时间戳、配置的证书指纹，以及 SignTool 的 Authenticode 策略。已经签过的文件也只有这些条件全部满足才允许跳过；相同指纹但内容已改变的文件不能跳过。[Microsoft SignTool 文档](https://learn.microsoft.com/en-us/windows/win32/seccrypto/signtool)

## 开发与发布

未提供 `sign` 的普通构建不要求证书，会明确提示未签名。显式请求 `sign` 时，证书不存在、过期、尚未生效、缺少私钥或代码签名用途，以及任一文件验签失败，均中止；不得继续生成“已签名”的升级元数据。空文件集合不能通过验签。

```powershell
# 只验签；配置指纹时同时校验发布者身份
.\scripts\sign.ps1 -Verify -Path .\dist\Moodflow-Setup-*.exe
# 策略回归测试，不调用真实签名服务、不安装证书
.\scripts\moodflow\test-signing.ps1
```

发布前还需在真正的证书环境中验收：签名整个包、解包验证内部 PE、验证卸载器 overlay 仍可读、验证更新清单哈希对应签名后的 Setup。尚未购买证书时，这些验收不能标记完成。PE 的产品名称来自 Moodflow 清单；Windows 显示的已验证发布者身份来自证书，不能靠改字符串或自签证书代替。
