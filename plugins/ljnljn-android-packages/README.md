# ljnljn-android-packages

LovelyCarve 原生插件（LPP v1 / C ABI v1），插件 ID `org.ljnljn.android-packages`。

解析 packages.xml，提取系统与应用版本、安装器与包属性。

## 分析器

| 类型 | analyzer | 扩展名 |
| --- | --- | --- |
| 单文件 | `ljnljn.android-packages.file` | xml |

## 覆盖范围

- <packages> 根与 <last-platform-version internal/external/fingerprint>（属性名大小写不敏感）
- 枚举 <package> 与可选 <updated-package>，按包名去重并保留首见位置
- 输出包名、version、codePath、installer、首见字节偏移与行号
- XML 声明、命名空间前缀、注释、自闭合标签、属性顺序与转义字符
- 重复包名、属性顺序变化、空文件、坏 XML 的明确状态与限制说明

## 构建

仓库内置 LovelyCarve 插件 SDK（`vendor/lovelycarve-sdk`，AGPL-3.0），无需外部路径：

```bash
cargo test --locked
cargo build --release --locked
```

Windows 产物为 `target/release/ljnljn_kb_android_packages_xml_2d5e9b71.dll`。

## 安装

把 `plugin.toml` 与构建出的 `ljnljn_kb_android_packages_xml_2d5e9b71.dll` 放进 LovelyCarve 插件目录下的同名子目录，重启引擎后生效。插件使用 `isolation = "process"`。

## 取证边界

只解析 XML 清单，不解析 APK 内容。

插件只读检材，不执行检材内容，不访问网络。已通过仓库内单元测试与 release 构建；尚未在真实 LovelyCarve GUI 与真实案件检材上完成端到端验证。

## 许可证

AGPL-3.0-only，见 [LICENSE](LICENSE)。内置 SDK 保留其原始许可证与协议文档。
