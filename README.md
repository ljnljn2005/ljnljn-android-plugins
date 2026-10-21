# ljnljn-android-plugins

Android 取证插件。本仓库按**平台**组织：一个平台一个仓库，统一使用 git tag 与 GitHub
topic `lovelycarve-plugins`。

当前包含 packages.xml 与 contacts2.db 两个分析器。

| 插件 | LPP | 能力 |
| --- | --- | --- |
| `ljnljn-android-packages` | v1 | packages.xml 系统版本与应用清单 |
| `ljnljn-android-contacts2` | v1 | contacts2.db 通话记录去重统计 |

每个插件是独立 Cargo 工程，共享仓库根目录的 SDK；进入插件目录即可单独测试与构建：

```bash
cd plugins/<插件目录>
cargo test --locked
cargo build --release --locked
```


Release 提供各插件的 `.lcpkg`、完整平台包与 `SHA256SUMS.txt`。也可以手动把
`plugin.toml`、DLL（v2 还包括 `schema.json` 与 `locales/`）放进插件包目录后重启
LovelyCarve。

插件只读检材，不执行检材内容，不访问网络。源码采用 AGPL-3.0-only。
