# SQLite 旧库迁移测试必须手建真实旧 schema

**结论速览**：迁移测试用旧版 SCHEMA 原文手建临时库（含 NULL 列数据行）再触发 open 迁移；只测新 schema 的内存库测不出旧行容错问题。
适用条件：任何"给已发布 SQLite 库加列"的迁移。

## ✅ 迁移测试抓到 log_rows 读 NULL 列整行崩溃（2026-10-08）

- **为何值得记**：这是本次 Ollama Cloud 接口升级终审前的真实 bug——若迁移测试只走 `open_in_memory`（新 SCHEMA 建库），该崩溃会带上线；旧库 NULL 容错是生产升级路径上唯一的未测面。
- **最终方案**：
```rust
// 手工建旧 schema 库（无 requests_24h 列 + 插入只填部分列的行，制造 NULL）
let conn = Connection::open(&path).unwrap();
conn.execute_batch("CREATE TABLE samples (...旧 schema 原文...);
    INSERT INTO samples (alias, fetched_at, session_pct) VALUES ('k1', '...', 5.0)").unwrap();
// 再 Store::open(path) 触发迁移，断言新列存在、旧行读出 NULL 不报错
```
读取侧对可空列一律走 `Option` 容错：
```rust
let models_json: Option<String> = r.get(3)?; // 旧行可能为 NULL
serde_json::from_str(models_json.as_deref().unwrap_or("[]")).unwrap_or_default()
```
- **为什么这样做**：生产代码的新 insert 永远写全量值，所以新 schema 测试全绿；但旧库里的行（尤其用户手工编辑、旧版本缺陷、或本次这种"手写迁移 SQL 漏列"）可以是任意 NULL 组合——`r.get::<String>(n)` 遇 NULL 直接返回 `InvalidColumnType` 错误，一行脏数据炸掉整个查询。
- **适用条件**：rusqlite 0.32.1 / SQLite 任意；`ALTER TABLE ADD COLUMN` 型迁移（新列旧行为 NULL）必中。
- **验证证据**：首次运行报 `InvalidColumnType(3, "session_models_json", Null)`；修复（`Option<String>` + `unwrap_or`）+ 补 `open` 时幂等迁移（PRAGMA table_info 查列再 ALTER）后，`旧库迁移自动补requests_24h列` 测试通过，全套 `cargo test` 42 项绿。
- **交叉验证**：单 agent 单次跑通，测试常驻套件；终审 agent（独立上下文）复核迁移路径"PRAGMA 查列 + 条件 ALTER 幂等；NULL 容错双处"无问题。
- **关键步骤**：旧 SCHEMA 原文建库 → 插入含 NULL 的旧行 → open 触发迁移 → 断言新列存在 + 旧行可读且新值为 NULL。
- **易错点**：最容易再错的是拿新 SCHEMA 建库再"假装"它是旧库（CREATE TABLE IF NOT EXISTS 不会改已建表，测不出 ALTER 路径）；以及给迁移测试插"全列有值"的完美行——脏数据才是要测的东西。

## ⛔ 只用 open_in_memory 测迁移相关读写（2026-10-08）

- **报错原文**：`InvalidColumnType(3, "session_models_json", Null)`
- **报错稳定片段**：`InvalidColumnType`（rusqlite 错误枚举名，grep 可命中）
- **错误原因**：内存库用新 SCHEMA 直接建全量列，所有行由新代码写入、永远不 NULL；旧行的 NULL 组合在测试里根本不存在。
- **为何不可再采用**：迁移的本质风险就在旧行的旧形态，绕开旧行等于没测迁移。
- **替代方案**：本文件上方 ✅ 条目（手建旧 schema 库）。
- **判定时效**：基于 rusqlite 0.32.1 / 2026-10；`r.get::<T>` 对 NULL 的报错行为属库契约，短期不会变。