
use postgres::{Client, NoTls};

use crate::models::Table;

/// PostgreSQL 表字段信息
struct PgColumn {
    name: String,
    data_type: String,
}

/// 将表数据插入或更新到 PostgreSQL 数据库中:
///
/// 规则:
/// 1. PostgreSQL 连接信息从 .env 读取
/// 2. PG_TABLE 从 .env 读取目标表名
/// 3. Table 第一行为字段名
/// 4. primary_keys 中的字段必须同时存在于 Table 和 PostgreSQL
/// 5. primary_keys 必须与 PostgreSQL 表的主键完全一致
/// 6. Table 中不存在于 PostgreSQL 的普通字段 -> 忽略
/// 7. Table 和 PostgreSQL 字段顺序不要求一致
/// 8. 只 upsert Table 中存在且 PostgreSQL 中也存在的字段
/// 9. Table 没有提供的 PostgreSQL 字段 -> 不处理
///
/// Table 中的数据全部为 String
/// PostgreSQL 类型转换在 SQL 中完成, 例如:
///
///     ($1::text)::integer
///     ($2::text)::date
///     ($3::text)::text
pub(crate) fn upsert_postgresql(
    table: Table,
    table_name: &str,
    primary_keys: Vec<String>,
) -> Result<(), String> {
    // ------------------------------------------------------------
    // 检查 Table
    // ------------------------------------------------------------

    if table.get_row_count() == 0 {
        return Err("Table 为空，至少需要一行表头".to_string());
    }

    if table.get_column_count() == 0 {
        return Err("Table 没有列".to_string());
    }

    if primary_keys.is_empty() {
        return Err("至少需要一个主键".to_string());
    }

    // ------------------------------------------------------------
    // 读取 .env
    // ------------------------------------------------------------
    let host = std::env::var("PG_HOST")
        .map_err(|_| "缺少环境变量 PG_HOST".to_string())?;

    let port = std::env::var("PG_PORT")
        .map_err(|_| "缺少环境变量 PG_PORT".to_string())?;

    let user = std::env::var("PG_USER")
        .map_err(|_| "缺少环境变量 PG_USER".to_string())?;

    let password = std::env::var("PG_PASSWORD")
        .map_err(|_| "缺少环境变量 PG_PASSWORD".to_string())?;

    let database = std::env::var("PG_DATABASE")
        .map_err(|_| "缺少环境变量 PG_DATABASE".to_string())?;

    // let table_name = std::env::var("PG_TABLE")
        // .map_err(|_| "缺少环境变量 PG_TABLE".to_string())?;

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
    // ------------------------------------------------------------

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
    //
    // format_type() 可以得到真正用于 SQL CAST 的类型，例如:
    //
    // integer
    // bigint
    // numeric(10,2)
    // date
    // timestamp without time zone
    // character varying
    // text
    // uuid
    // integer[]
    // ...
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

    let pg_column_names: Vec<String> = pg_columns
        .iter()
        .map(|column| column.name.clone())
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

    // ------------------------------------------------------------
    // 获取 Table 表头
    // ------------------------------------------------------------

    let table_columns: Vec<String> = (0..table.get_column_count())
        .map(|col| {
            table
                .get_data()
                .get(col)
                .cloned()
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
    // 检查 primary_keys 在 Table 中存在
    // ------------------------------------------------------------

    for key in &primary_keys {
        if !table_columns.iter().any(|column| column == key) {
            return Err(format!(
                "主键字段 {key} 不存在于 Table 表头"
            ));
        }
    }

    // ------------------------------------------------------------
    // 检查 primary_keys 在 PostgreSQL 中存在
    // ------------------------------------------------------------

    for key in &primary_keys {
        if !pg_column_names.iter().any(|column| column == key) {
            return Err(format!(
                "主键字段 {key} 不存在于 PostgreSQL 表 {table_name}"
            ));
        }
    }

    // ------------------------------------------------------------
    // 检查 primary_keys 是否真的等于 PostgreSQL 主键
    //
    // 不要求顺序一致
    // ------------------------------------------------------------

    let mut expected_keys = primary_keys.clone();
    let mut actual_keys = pg_primary_keys.clone();

    expected_keys.sort();
    actual_keys.sort();

    if expected_keys != actual_keys {
        return Err(format!(
            "主键不匹配\n\
             指定主键: {:?}\n\
             PostgreSQL 主键: {:?}",
            primary_keys,
            pg_primary_keys
        ));
    }

    // ------------------------------------------------------------
    // 确定真正参与 upsert 的字段
    //
    // Table 中有
    // +
    // PostgreSQL 中也有
    //
    // 顺序保持 Table 的顺序
    // ------------------------------------------------------------

    let upsert_columns: Vec<&PgColumn> = table_columns
        .iter()
        .filter_map(|table_column| {
            pg_columns
                .iter()
                .find(|pg_column| pg_column.name == *table_column)
        })
        .collect();

    if upsert_columns.is_empty() {
        return Err(
            "Table 没有任何可以写入 PostgreSQL 的字段".to_string()
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

    let quoted_table = quote_identifier(&table_name);

    let quoted_columns = upsert_columns
        .iter()
        .map(|column| quote_identifier(&column.name))
        .collect::<Vec<_>>()
        .join(", ");

    // ------------------------------------------------------------
    // 构造参数
    //
    // 注意：
    //
    //     $1
    //
    // 如果 PostgreSQL 推断它是 integer，
    // Rust String 就可能出现:
    //
    //     error serializing parameter 0
    //
    // 因此明确让参数类型为 TEXT:
    //
    //     $1::text
    //
    // 然后再转成目标类型:
    //
    //     ($1::text)::integer
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
    // ------------------------------------------------------------

    let quoted_primary_keys = primary_keys
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
            !primary_keys
                .iter()
                .any(|key| key == &column.name)
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
        .map_err(|e| format!("开始 PostgreSQL 事务失败: {e}"))?;

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
    // Table 第 0 行是表头
    // ------------------------------------------------------------

    for row in 1..table.get_row_count() {
        let values: Vec<String> = table_column_indices
            .iter()
            .map(|&col| {
                table
                    .get_data()
                    .get(
                        row * table.get_column_count() + col
                    )
                    .cloned()
                    .unwrap_or_default()
            })
            .collect();

        // 所有参数都是 String
        //
        // 因为 SQL 中已经通过 $N::text 明确指定参数类型，
        // PostgreSQL driver 不会再尝试把 String 当作 integer/date 等 Rust 类型。
        let params: Vec<&(dyn postgres::types::ToSql + Sync)> =
            values
                .iter()
                .map(|value| {
                    value as &(dyn postgres::types::ToSql + Sync)
                })
                .collect();

        transaction
            .execute(&statement, &params)
            .map_err(|e| {
                format!(
                    "写入 Table 第 {} 行失败: {e}\n数据: {:?}",
                    row + 1,
                    values
                )
            })?;
    }

    drop(statement);

    transaction
        .commit()
        .map_err(|e| format!("提交 PostgreSQL 事务失败: {e}"))?;

    Ok(())
}

/// PostgreSQL 标识符转义
fn quote_identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}