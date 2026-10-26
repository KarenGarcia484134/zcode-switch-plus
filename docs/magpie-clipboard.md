# Magpie 账号剪贴板格式 v1

账号卡片的 **复制到 Magpie** 使用现有 `account_secret` IPC 命令，`kind` 为 `magpie`。后端读取所选存档账号，返回一行紧凑 JSON；前端写入剪贴板。复制不发起网络请求，不切换账号，不写入 ZCode 配置，不创建导出文件。

在配套 `magpie-zcode-switch-auth` 0.3.0 或更高版本中选择 **Switch Plus: Import account**，粘贴该内容即可添加；不再单独询问站点、名称或凭据种类。

```json
{
  "format": "zcode-switch-plus/magpie-account",
  "version": 1,
  "site": "zai",
  "account": { "id": "存档账号的稳定 ID", "name": "账号名称" },
  "credentials": { "apiKey": "id.secret", "jwt": "完整的 ZCode JWT" },
  "deviceId": "账号已保存的设备标识"
}
```

- `site` 仅为 `zai` 或 `bigmodel`，首先取解密后的 `oauth:active_provider`。缺少该字段时，仅在 config／credentials 明确属于单一站点时推断；有歧义则报错。
- `account.id` 来自所选存档账号，`account.name` 为其备注。插件使用站点与来源 ID 生成稳定身份；同一名称的不同来源账号不会覆盖。相同名称和来源账号更新 JWT 时会更新原条目。若同时改了账号名称，Magpie 当前宿主可能保留旧名称条目，需在 Magpie 中管理旧条目。
- `credentials.apiKey` 与 `credentials.jwt` 均可省略，但至少有一个可用。API Key 只取本站点的非 Start Coding Plan key，优先启用的 config entry，再取新版 credentials 的本站点 key；支持本地解密。
- `jwt` 来自 `zcodejwttoken`。已过期或在 30 秒内过期的 JWT 不复制；若仍有有效 API Key，则继续复制该 key。两者都没有可用值时，提示重新登录保存。
- `deviceId` 仅在账号已有合法设备标识时包含；复制不会为此修改存档。缺少时由插件创建并保存在其登录状态中。
- 不复制业务 OAuth access token、用户密码、邮箱、完整 credentials/config、API host、代理设置或本地路径。插件始终自行选择固定的官方端点。

此数据属于**含明文凭据的剪贴板内容**，与密码加密的 `.zsb` 导出不同。用户点击按钮才复制，界面不显示完整内容，错误消息不包含凭据。

契约样本为 `tests/fixtures/magpie-account.json`，其中全部凭据均为测试数据；Rust 导出测试与配套插件导入测试使用同一份样本。后端实现位于 `src-tauri/src/magpie.rs`。
