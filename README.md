# ljnljn-android-plugins

Android 取证插件，全部使用 LovelyCarve **LPP v2**（protocol 2）与类型化记录。
统一 git tag 与 GitHub topic：`lovelycarve-plugins`。

| 插件 | LPP | 能力 |
| --- | --- | --- |
| `ljnljn-android-packages` | v2 | packages.xml 系统版本与应用清单 |
| `ljnljn-android-contacts2` | v2 | contacts2.db 通话记录去重统计 |
| `ljnljn-android-native-artifacts` | v2 | 原生读取短信/MMS、通话、联系人、Chrome、WhatsApp、媒体库、账号同步与 WiFi 配置 |

原生插件直接打开提取物里的数据库/XML，不运行 ALEAPP；共享解析层在
仓库根目录的 `mobile-core/`。尚未原生覆盖的 ALEAPP 模块仍可用
`ljnljn-ileapp-aleapp-report` 导入 TSV 报告。

每个插件是独立 Cargo 工程，仓库根目录提供共享 SDK v2：

```bash
cd plugins/<插件目录>
cargo test --locked
cargo build --release --locked
```

Release 提供各插件 `.lcpkg`、完整平台 zip、`INDEX.json` 和
`SHA256SUMS.txt`。插件只读检材，不执行检材内容，不访问网络；结果需要
取证员复核。源码采用 AGPL-3.0-only。
