
// src/sqlite.rs
// SQLite 数据库操作

use crate::models::Table;
use rusqlite;


/// 将表数据插入或更新到 SQLite 数据库中:
///
/// 规则:
/// 1. 数据库不存在 -> 报错
/// 2. SQLite 表不存在 -> 报错
/// 3. Table 第一行为字段名
/// 4. primary_keys 中的字段必须同时存在于 Table 和 SQLite
/// 5. Table 中不存在于 SQLite 的普通字段 -> 忽略
/// 6. Table 和 SQLite 字段顺序不要求一致
/// 7. 只 upsert Table 中存在且 SQLite 中也存在的字段
/// 8. Table 没有提供的 SQLite 字段 -> 不处理
pub(crate) fn upsert_sqlite(
    db_path: &str,
    table_name: &str,
    table: Table,
    primary_keys: Vec<String>,
) -> Result<(), String> {

    if table.get_row_count() == 0 {
        return Err("Table 为空, 至少需要一行表头".to_string());
    }
    if table.get_column_count() == 0 {
        return Err("Table 没有列".to_string());
    }
    if primary_keys.is_empty() {
        return Err("至少需要一个主键".to_string());
    }

    // 打开数据库
    // 不允许自动创建数据库
    let conn = rusqlite::Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .map_err(|e| format!("打开数据库失败: {e}"))?;

    // 检查 SQLite 表是否存在
    let table_exists: bool = conn
        .query_row(
            "SELECT EXISTS(
                SELECT 1
                FROM sqlite_master
                WHERE type = 'table'
                  AND name = ?1
            )",
            [table_name],
            |row| row.get(0),
        )
        .map_err(|e| format!("检查 SQLite 表失败: {e}"))?;

    if !table_exists {
        return Err(format!("SQLite 表不存在: {table_name}"));
    }

    // 读取 SQLite 表结构
    let pragma_sql = format!("PRAGMA table_info({})", quote_identifier(table_name));
    let mut stmt = conn
    .prepare(&pragma_sql)
    .map_err(|e| format!("读取表结构失败: {e}"))?;

    struct ColumnInfo {
        name: String,
        pk_order: i32,
    }

    let sqlite_columns = stmt
    .query_map([], |row| {
        Ok(ColumnInfo {
            name: row.get(1)?,
            pk_order: row.get(5)?,
        })
    })
    .map_err(|e| format!("读取表结构失败: {e}"))?
    .collect::<Result<Vec<_>, _>>()
    .map_err(|e| format!("读取表结构失败: {e}"))?;

    // SQLite 字段
    let sqlite_column_names: Vec<String> = sqlite_columns
        .iter()
        .map(|c| c.name.clone())
        .collect();
    // SQLite 主键
    let mut sqlite_primary_keys: Vec<(i32, String)> = sqlite_columns
        .iter()
        .filter(|c| c.pk_order > 0)
        .map(|c| (c.pk_order, c.name.clone()))
        .collect();
    sqlite_primary_keys.sort_by_key(|(order, _)| *order);


    // 取得 Table 表头
    let table_columns: Vec<String> = (0..table.get_column_count())
    .map(|col| {
        table
            .get_data()
            .get(col)
            .cloned()
            .unwrap_or_default()
    }).flatten()
    .collect();
    // 检查 Table 表头是否重复
    for i in 0..table_columns.len() {
        for j in (i + 1)..table_columns.len() {
            if table_columns[i] == table_columns[j] {
                return Err(format!(
                    "Table 表头存在重复字段: {}",
                    table_columns[i]
                ));
            }
        }
    }

    // ------------------------------------------------------------
    // 检查 primary_keys
    //
    // 这里只要求:
    // primary_keys 中的字段存在于 SQLite
    // primary_keys 中的字段存在于 Table
    //
    // 不要求 Table 包含 SQLite 所有字段
    // 不要求 Table 顺序和 SQLite 一致
    // ------------------------------------------------------------
    for key in &primary_keys {
        if !table_columns.iter().any(|c| c == key) {
            return Err(format!(
                "主键字段 {} 不存在于 Table 表头",
                key
            ));
        }

        if !sqlite_column_names.iter().any(|c| c == key) {
            return Err(format!(
                "主键字段 {} 不存在于 SQLite 表 {}",
                key,
                table_name
            ));
        }
    }

    // ------------------------------------------------------------
    // 检查 primary_keys 内部是否重复
    // ------------------------------------------------------------
    for i in 0..primary_keys.len() {
        for j in (i + 1)..primary_keys.len() {
            if primary_keys[i] == primary_keys[j] {
                return Err(format!(
                    "primary_keys 存在重复字段: {}",
                    primary_keys[i]
                ));
            }
        }
    }

    // ------------------------------------------------------------
    // 确定真正参与 upsert 的字段
    //
    // 只保留:
    // Table 有
    // +
    // SQLite 也有
    //
    // 顺序按照 Table 表头顺序
    // ------------------------------------------------------------
    let mut upsert_columns: Vec<String> = Vec::new();
    for column in &table_columns {
        if sqlite_column_names.iter().any(|c| c == column) {
            upsert_columns.push(column.clone());
        }
    }

    // ------------------------------------------------------------
    // 确定 upsert_columns 对应的 Table 列索引
    // ------------------------------------------------------------
    let upsert_table_indices: Vec<usize> = upsert_columns
        .iter()
        .map(|column| {
            table_columns
                .iter()
                .position(|c| c == column)
                .ok_or_else(|| {
                    format!("找不到字段: {column}")
                })
        })
        .collect::<Result<Vec<_>, _>>()?;

    // ------------------------------------------------------------
    // 构造 SQL
    // ------------------------------------------------------------
    let quoted_table = quote_identifier(table_name);

    let quoted_columns = upsert_columns
        .iter()
        .map(|c| quote_identifier(c))
        .collect::<Vec<_>>()
        .join(", ");

    let placeholders = (1..=upsert_columns.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");

    let quoted_primary_keys = primary_keys
        .iter()
        .map(|c| quote_identifier(c))
        .collect::<Vec<_>>()
        .join(", ");

    // ------------------------------------------------------------
    // 非主键字段
    // ------------------------------------------------------------
    let update_columns: Vec<&String> = upsert_columns
        .iter()
        .filter(|column| !primary_keys.iter().any(|pk| pk == *column))
        .collect();

    let sql = if update_columns.is_empty() {
        // Table 中只有主键
        format!(
            "INSERT INTO {quoted_table} ({quoted_columns})
             VALUES ({placeholders})
             ON CONFLICT ({quoted_primary_keys})
             DO NOTHING"
        )
    } else {
        let assignments = update_columns
            .iter()
            .map(|column| {
                let quoted = quote_identifier(column);
                format!("{quoted} = excluded.{quoted}")
            })
            .collect::<Vec<_>>()
            .join(", ");

        format!(
            "INSERT INTO {quoted_table} ({quoted_columns})
             VALUES ({placeholders})
             ON CONFLICT ({quoted_primary_keys})
             DO UPDATE SET {assignments}"
        )
    };

    // ------------------------------------------------------------
    // 开始事务
    // ------------------------------------------------------------
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("开始事务失败: {e}"))?;

    let mut stmt = tx
        .prepare(&sql)
        .map_err(|e| format!("准备 SQL 失败: {e}\nSQL: {sql}"))?;

    // ------------------------------------------------------------
    // 插入 / 更新
    //
    // Table 第 0 行是表头, 所以从第 1 行开始
    // ------------------------------------------------------------
    for row in 1..table.get_row_count() {
        let values: Vec<String> = upsert_table_indices
            .iter()
            .map(|&col| {
                table
                    .get_data()
                    .get(row * table.get_column_count() + col)
                    .cloned()
                    .unwrap_or_default()
            }).flatten()
            .collect();

        stmt.execute(rusqlite::params_from_iter(values.iter()))
            .map_err(|e| {
                format!(
                    "写入 Table 第 {} 行失败: {e}",
                    row + 1
                )
            })?;
    }

    drop(stmt);

    tx.commit().map_err(|e| format!("提交事务失败: {e}"))?;

    Ok(())
}

/// SQLite 标识符转义
fn quote_identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}