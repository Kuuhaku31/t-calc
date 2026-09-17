
mod torrent_to_postgres;
pub(crate) use torrent_to_postgres::upsert_torrents;

use postgres::{Client, NoTls};

use crate::models::Table;

/// PostgreSQL 表字段信息
struct PgColumn {
    name: String,
    data_type: String,
}


/// 将 Table 中指定的字段插入或更新到 PostgreSQL 数据库中:
///
/// 规则:
/// 1. PostgreSQL 连接信息从 .env 读取
/// 2. Table 第一行为字段名
/// 3. PostgreSQL 主键自动获取
/// 4. 数据库的所有主键字段必须存在于 keys 中
/// 5. 所有数据库主键字段都必须存在于 Table 中
/// 6. Table 中只有出现在 keys 中的字段才会被考虑
/// 7. keys 中不存在于 PostgreSQL 的字段 -> 忽略
/// 8. Table 中不存在于 PostgreSQL 的字段 -> 忽略
/// 9. Table 和 PostgreSQL 字段顺序不要求一致
/// 10. PostgreSQL 中不在 keys 中的字段 -> 不处理
/// 11. 根据 PostgreSQL 实际主键执行 UPSERT
/// 12. 单行数据写入失败 -> 跳过该行, 不影响其他行
/// 13. 最后打印成功/失败数量
///
/// Table 中的数据全部为 Option<String>
/// PostgreSQL 类型转换在 SQL 中完成.
pub(crate) fn upsert_postgresql(
    table: Table,
    table_name: &str,
    keys: Vec<String>,
) -> Result<(), String> {

    // ------------------------------------------------------------
    // 检查 Table
    // ------------------------------------------------------------
    if table.get_row_count() == 0 { return Err("Table 为空, 至少需要一行表头".to_string()); }
    if table.get_column_count() == 0 { return Err("Table 没有列".to_string()); }
    if keys.is_empty() { return Err("至少需要一个处理列".to_string()); }

    // ------------------------------------------------------------
    // 检查 keys 是否重复
    // ------------------------------------------------------------
    for i in 0..keys.len() {
        for j in (i + 1)..keys.len() {
            if keys[i] == keys[j] {
                return Err(format!(
                    "keys 存在重复字段: {}",
                    keys[i]
                ));
            }
        }
    }

    // ------------------------------------------------------------
    // 读取 .env
    // ------------------------------------------------------------
    let host = std::env::var("PG_HOST").map_err(|e| format!("无法读取环境变量 PG_HOST: {e}"))?;
    let port = std::env::var("PG_PORT").map_err(|e| format!("无法读取环境变量 PG_PORT: {e}"))?;
    let user = std::env::var("PG_USER").map_err(|e| format!("无法读取环境变量 PG_USER: {e}"))?;
    let password = std::env::var("PG_PASSWORD").map_err(|e| format!("无法读取环境变量 PG_PASSWORD: {e}"))?;
    let database = std::env::var("PG_DATABASE").map_err(|e| format!("无法读取环境变量 PG_DATABASE: {e}"))?;

    // ------------------------------------------------------------
    // 连接 PostgreSQL
    // ------------------------------------------------------------
    let conn_str = format!(
        "host={} port={} user={} password={} dbname={}",
        host, port, user, password, database
    );
    let mut client = Client::connect(&conn_str, NoTls)
    .map_err(|e| format!("连接 PostgreSQL 失败: {e}"))?;

    // ------------------------------------------------------------
    // 检查 PostgreSQL 表是否存在
    // -----------------------------------------------------------
    let table_exists = client
        .query_one(
            "
            SELECT EXISTS (
                SELECT 1
                FROM pg_catalog.pg_class c
                JOIN pg_catalog.pg_namespace n
                  ON n.oid = c.relnamespace
                WHERE c.relkind IN ('r', 'p')
                  AND n.nspname = 'public'
                  AND c.relname = $1
            )
            ",
            &[&table_name],
        )
        .map_err(|e| format!("检查 PostgreSQL 表失败: {e}"))?
        .get::<_, bool>(0);

    if !table_exists {
        return Err(format!(
            "PostgreSQL 表不存在: public.{table_name}"
        ));
    }

    // ------------------------------------------------------------
    // 读取 PostgreSQL 字段和类型
    // ------------------------------------------------------------
    let pg_columns: Vec<PgColumn> = client
    .query(
        "
        SELECT
            a.attname,
            pg_catalog.format_type(a.atttypid, a.atttypmod)
        FROM pg_catalog.pg_attribute a
        JOIN pg_catalog.pg_class c
            ON c.oid = a.attrelid
        JOIN pg_catalog.pg_namespace n
            ON n.oid = c.relnamespace
        WHERE n.nspname = 'public'
            AND c.relname = $1
            AND a.attnum > 0
            AND NOT a.attisdropped
        ORDER BY a.attnum
        ",
        &[&table_name],
    )
    .map_err(|e| format!("读取 PostgreSQL 字段失败: {e}"))?
    .into_iter()
    .map(|row| PgColumn {
        name: row.get(0),
        data_type: row.get(1),
    })
    .collect();

    // ------------------------------------------------------------
    // 获取 PostgreSQL 主键
    // ------------------------------------------------------------

    let pg_primary_keys: Vec<String> = client
    .query(
        "
        SELECT a.attname
        FROM pg_catalog.pg_index i
        JOIN pg_catalog.pg_attribute a
            ON a.attrelid = i.indrelid
            AND a.attnum = ANY(i.indkey)
        JOIN pg_catalog.pg_class c
            ON c.oid = i.indrelid
        JOIN pg_catalog.pg_namespace n
            ON n.oid = c.relnamespace
        WHERE i.indisprimary
            AND n.nspname = 'public'
            AND c.relname = $1
        ORDER BY array_position(i.indkey, a.attnum)
        ",
        &[&table_name],
    )
    .map_err(|e| format!("读取 PostgreSQL 主键失败: {e}"))?
    .into_iter()
    .map(|row| row.get(0))
    .collect();
    if pg_primary_keys.is_empty() {
        return Err(format!(
            "PostgreSQL 表 {table_name} 没有主键, 无法执行 UPSERT"
        ));
    }

    // ------------------------------------------------------------
    // 获取 Table 表头
    // ------------------------------------------------------------
    let table_columns: Vec<String> = (0..table.get_column_count())
    .map(|col| {
        table
        .get_data()
        .get(col)
        .cloned()
        .flatten()
        .unwrap_or_default()
    })
    .collect();

    // ------------------------------------------------------------
    // 检查 Table 表头重复
    // ------------------------------------------------------------
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
    // 所有数据库主键必须同时存在于 Table 和 keys 中
    //
    // 否则无法保证 INSERT/UPDATE 使用完整主键
    // ------------------------------------------------------------
    for pk in &pg_primary_keys {
        if !keys.iter().any(|key| key == pk) {
            return Err(format!(
                "数据库主键字段 {pk} 不在 keys 中"
            ));
        } else if !table_columns.iter().any(|col| col == pk) {
            return Err(format!(
                "数据库主键字段 {pk} 不在 Table 表头中"
            ));
        }
    }

    // ------------------------------------------------------------
    // 确定实际需要处理的字段
    //
    // 条件:
    // 1. 出现在 keys
    // 2. 出现在 Table
    // 3. 出现在 PostgreSQL
    //
    // 顺序按照 Table 表头顺序
    // ------------------------------------------------------------
    let upsert_columns: Vec<&PgColumn> = table_columns
    .iter()
    .filter(|table_column| {
        keys.iter().any(|key| key == *table_column)
    })
    .filter_map(|table_column| {
        pg_columns
            .iter()
            .find(|pg_column| pg_column.name == *table_column)
    })
    .collect();
    if upsert_columns.is_empty() {
        return Err(
            "Table 中没有任何可以写入 PostgreSQL 的字段"
                .to_string()
        );
    }

    // ------------------------------------------------------------
    // Table 列索引
    // ------------------------------------------------------------
    let table_column_indices: Vec<usize> = upsert_columns
    .iter()
    .map(|pg_column| {
        table_columns
            .iter()
            .position(|column| column == &pg_column.name)
            .ok_or_else(|| {
                format!("找不到字段: {}", pg_column.name)
            })
    })
    .collect::<Result<Vec<_>, _>>()?;

    // ------------------------------------------------------------
    // 构造 INSERT 字段列表
    // ------------------------------------------------------------
    let quoted_table = quote_identifier(table_name);
    let quoted_columns = upsert_columns
        .iter()
        .map(|column| quote_identifier(&column.name))
        .collect::<Vec<_>>()
        .join(", ");

    // ------------------------------------------------------------
    // 构造参数
    // ------------------------------------------------------------
    let placeholders = upsert_columns
        .iter()
        .enumerate()
        .map(|(index, column)| {
            format!(
                "(${}::text)::{}",
                index + 1,
                column.data_type
            )
        })
        .collect::<Vec<_>>()
        .join(", ");

    // ------------------------------------------------------------
    // 主键
    // 自动使用 PostgreSQL 实际主键
    // ------------------------------------------------------------
    let quoted_primary_keys = pg_primary_keys
        .iter()
        .map(|key| quote_identifier(key))
        .collect::<Vec<_>>()
        .join(", ");

    // ------------------------------------------------------------
    // 非主键字段
    // ------------------------------------------------------------
    let update_columns: Vec<&PgColumn> = upsert_columns
        .iter()
        .copied()
        .filter(|column| {
            !pg_primary_keys
                .iter()
                .any(|pk| pk == &column.name)
        })
        .collect();

    // ------------------------------------------------------------
    // 构造 UPSERT SQL
    // ------------------------------------------------------------
    let sql = if update_columns.is_empty() {
        format!(
            "
            INSERT INTO {quoted_table}
                ({quoted_columns})
            VALUES
                ({placeholders})
            ON CONFLICT ({quoted_primary_keys})
            DO NOTHING
            "
        )
    } else {
        let assignments = update_columns
            .iter()
            .map(|column| {
                let quoted = quote_identifier(&column.name);

                format!(
                    "{quoted} = EXCLUDED.{quoted}"
                )
            })
            .collect::<Vec<_>>()
            .join(", ");

        format!(
            "
            INSERT INTO {quoted_table}
                ({quoted_columns})
            VALUES
                ({placeholders})
            ON CONFLICT ({quoted_primary_keys})
            DO UPDATE SET
                {assignments}
            "
        )
    };

    // ------------------------------------------------------------
    // 开始事务
    // ------------------------------------------------------------
    let mut transaction = client
        .transaction()
        .map_err(|e| {
            format!("开始 PostgreSQL 事务失败: {e}")
        })?;

    let statement = transaction
        .prepare(&sql)
        .map_err(|e| {
            format!(
                "准备 PostgreSQL SQL 失败: {e}\nSQL:\n{sql}"
            )
        })?;

    // ------------------------------------------------------------
    // 插入 / 更新
    //
    // 每一行使用独立 SAVEPOINT。
    // 某一行失败时:
    //
    // SAVEPOINT
    //    ↓
    // execute 失败
    //    ↓
    // ROLLBACK TO SAVEPOINT
    //    ↓
    // 继续下一行
    // ------------------------------------------------------------
    let mut success_count = 0usize;
    let mut failed_count = 0usize;
    for row in 1..table.get_row_count() {
        let values: Vec<Option<String>> = table_column_indices
            .iter()
            .map(|&col| {
                table
                    .get_data()
                    .get(
                        row * table.get_column_count() + col
                    )
                    .cloned()
                    .flatten()
            })
            .collect();

        let params: Vec<&(dyn postgres::types::ToSql + Sync)> =
            values
                .iter()
                .map(|value| {
                    value as &(dyn postgres::types::ToSql + Sync)
                })
                .collect();

        let savepoint = format!("row_{row}");

        // 创建 savepoint
        if let Err(e) = transaction.batch_execute(
            &format!("SAVEPOINT {savepoint}")
        ) {
            return Err(format!(
                "创建第 {} 行 SAVEPOINT 失败: {e}",
                row + 1
            ));
        }

        match transaction.execute(&statement, &params) {
            Ok(_) => {
                // 成功后释放 savepoint
                if let Err(e) = transaction.batch_execute(
                    &format!("RELEASE SAVEPOINT {savepoint}")
                ) {
                    return Err(format!(
                        "释放第 {} 行 SAVEPOINT 失败: {e}",
                        row + 1
                    ));
                }

                success_count += 1;
            }

            Err(e) => {
                eprintln!(
                    "跳过 Table 第 {} 行: {e}\n数据: {:?}",
                    row + 1,
                    values
                );

                // 回滚当前行
                if let Err(rollback_error) = transaction.batch_execute(
                    &format!("ROLLBACK TO SAVEPOINT {savepoint}")
                ) {
                    return Err(format!(
                        "回滚第 {} 行失败: {rollback_error}",
                        row + 1
                    ));
                }

                // 回滚后释放 savepoint
                if let Err(release_error) = transaction.batch_execute(
                    &format!("RELEASE SAVEPOINT {savepoint}")
                ) {
                    return Err(format!(
                        "释放第 {} 行 SAVEPOINT 失败: {release_error}",
                        row + 1
                    ));
                }

                failed_count += 1;
            }
        }
    }

    drop(statement);

    // ------------------------------------------------------------
    // 提交事务
    // ------------------------------------------------------------
    transaction
    .commit()
    .map_err(|e| {
        format!("提交 PostgreSQL 事务失败: {e}")
    })?;

println!(
    "PostgreSQL 导入完成: 共 {} 行, 成功 {} 行, 失败 {} 行",
    success_count + failed_count,
    success_count,
    failed_count
    );

    Ok(())
}

/// PostgreSQL 标识符转义
fn quote_identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}