# mobile-core

iOS / Android 原生工件插件共用的只读解析层。

- `sqlite.rs`：把数据库和 `-wal`/`-shm`/`-journal` 复制到私有临时目录后
  只读打开；限制单库、行数和结果字节数。
- `time.rs`：Cocoa/Core Data、Unix 秒/毫秒/微秒/纳秒、Chromium/WebKit
  1601 纪元时间的归一化。
- `artifact.rs`：`mobile.artifact` / `mobile.artifact.file` 记录，以及按
  表存在性选择固定 SQL 变体的执行框架。

本 crate 不单独发布为插件，只被 `ljnljn-ios-native-artifacts` 和
`ljnljn-android-native-artifacts` 编译进去。

许可证：AGPL-3.0-only。
