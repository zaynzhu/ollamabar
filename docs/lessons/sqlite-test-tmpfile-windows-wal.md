# SQLite 测试临时文件在 Windows 上的 WAL 句柄坑

**结论速览**：临时库测试收尾先 `drop(store)` 再 `remove_file`，并一并清 `-wal`/`-shm` 附属文件。
适用条件：Windows + rusqlite（WAL 模式）+ 测试里建临时 .db 文件。

## ✅ Windows 下删除 SQLite 临时库报"文件被占用"（2026-10-08）

- **为何值得记**：Windows 独有的测试坑，报错信息不指向根因（SQLite 句柄），Linux/macOS 上测试不会暴露，跨平台项目易漏。
- **最终方案**：
```rust
drop(store); // Windows 下 WAL 连接持有句柄，先 drop 才能删文件
let _ = std::fs::remove_file(&path);
let _ = std::fs::remove_file(path.with_extension("db-wal"));
let _ = std::fs::remove_file(path.with_extension("db-shm"));
```
- **为什么这样做**：rusqlite 的 `Connection` 活着时操作系统文件句柄未释放，WAL 模式还会额外产生 `xxx.db-wal` / `xxx.db-shm` 两个附属文件；测试里 `store` 变量存活到作用域末尾，`remove_file` 抢在 drop 之前执行必失败。
- **适用条件**：Windows 任意版本 + rusqlite 0.32.1（其他版本同机制）；`path` 形如 `xxx.db` 时 `with_extension("db-wal")` 恰好生成 WAL 附属文件名 `xxx.db-wal`。
- **验证证据**：`store.rs` 迁移测试首次运行报 `Os { code: 32, kind: Uncategorized, message: "另一个程序正在使用此文件，进程无法访问。" }`；加 `drop` 并改 `unwrap()` 为忽略错误后，`cargo test` 全绿（42 项通过）。
- **交叉验证**：单 agent 单次跑通；修复后纳入常驻测试套件（`src-tauri/src/store.rs` `旧库迁移自动补requests_24h列`），后续每次 `cargo test` 隐式复验。
- **关键步骤**：drop 连接 → 删主库 → 删 -wal → 删 -shm，顺序无硬性要求但 drop 必须最先。
- **易错点**：最容易再错的是只删主库忘删附属文件（WAL 残留文件在下次同名建库时可能复活旧状态）；以及把 `remove_file(...).unwrap()` 留着——在 CI 的 Windows runner 上同样会炸。