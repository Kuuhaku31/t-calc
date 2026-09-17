
// src/torrent_to_postgres.rs
// 把种子文件插入 postgres 数据库

use postgres::{Client, Config, NoTls};

use crate::models::TorrentRecord;


/// 批量插入或更新 TorrentRecord.
///
/// 处理规则:
///
/// 1. info_hash 不存在:
///       INSERT
///
/// 2. info_hash 已存在:
///       UPDATE
///
/// 3. 单条记录转换或 SQL 执行失败:
///       打印错误并跳过
///
/// 4. 不使用显式事务, 因此单条失败不会导致之前/之后的数据回滚.
///
/// 5. 函数返回 Err 只表示数据库连接或 SQL prepared statement 初始化失败;
///    单条数据失败不会通过 Err 返回.
pub(crate) fn upsert_torrents(records: &[TorrentRecord]) -> Result<(), String> {
    println!(
        "开始处理 Torrent, 共 {} 条记录.",
        records.len()
    );

    if records.is_empty() {
        println!("没有需要处理的 Torrent.");
        return Ok(());
    }

    let mut client = connect_postgres()?;

    println!("PostgreSQL 连接成功.");

    let statement = client
        .prepare(
            r#"
            INSERT INTO torrent (
                info_hash,
                title,
                data,
                file_size,
                file_count,
                folder_count
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (info_hash)
            DO UPDATE SET
                title        = EXCLUDED.title,
                data         = EXCLUDED.data,
                file_size    = EXCLUDED.file_size,
                file_count   = EXCLUDED.file_count,
                folder_count = EXCLUDED.folder_count
            "#,
        )
        .map_err(|e| {
            format!("准备 Torrent UPSERT SQL 失败: {e}")
        })?;

    let total = records.len();
    let mut success_count = 0usize;
    let mut failed_count = 0usize;

    for (index, record) in records.iter().enumerate() {
        let number = index + 1;
        let hash = info_hash_to_hex(record.get_info_hash());

        println!(
            "[{number}/{total}] 处理 Torrent: {} ({})",
            record.get_title(),
            hash
        );

        // usize -> i64
        let file_size = match i64::try_from(record.get_file_size()) {
            Ok(value) => value,
            Err(_) => {
                println!(
                    "    FAILED: file_size 超出 PostgreSQL BIGINT 范围, 跳过."
                );

                failed_count += 1;
                continue;
            }
        };

        // usize -> i32
        let file_count = match i32::try_from(record.get_file_count()) {
            Ok(value) => value,
            Err(_) => {
                println!(
                    "    FAILED: file_count 超出 PostgreSQL INTEGER 范围, 跳过."
                );

                failed_count += 1;
                continue;
            }
        };

        // usize -> i32
        let folder_count = match i32::try_from(record.get_folder_count()) {
            Ok(value) => value,
            Err(_) => {
                println!(
                    "    FAILED: folder_count 超出 PostgreSQL INTEGER 范围, 跳过."
                );

                failed_count += 1;
                continue;
            }
        };

        let result = client.execute(
            &statement,
            &[
                &record.get_info_hash().as_slice(),
                &record.get_title(),
                &record.get_data().as_slice(),
                &file_size,
                &file_count,
                &folder_count,
            ],
        );

        match result {
            Ok(rows) => {
                println!(
                    "    OK: UPSERT 成功，影响 {} 行。",
                    rows
                );

                success_count += 1;
            }

            Err(error) => {
                println!(
                    "    FAILED: PostgreSQL 执行失败: {}",
                    error
                );

                failed_count += 1;
            }
        }
    }

    println!();
    println!("Torrent 数据库处理完成.");
    println!("----------------------------------------");
    println!("总数:     {}", total);
    println!("成功:     {}", success_count);
    println!("失败:     {}", failed_count);
    println!("----------------------------------------");

    Ok(())
}


/// 从环境变量创建 PostgreSQL 连接.
///
/// 必需环境变量:
///     PG_HOST
///     PG_PORT       默认 5432
///     PG_DATABASE
///     PG_USER
///     PG_PASSWORD
fn connect_postgres() -> Result<Client, String> {
    let host = std::env::var("PG_HOST")
        .map_err(|_| "环境变量 PG_HOST 未设置".to_string())?;

    let port = std::env::var("PG_PORT")
        .unwrap_or_else(|_| "5432".to_string())
        .parse::<u16>()
        .map_err(|_| "环境变量 PG_PORT 不是合法端口号".to_string())?;

    let database = std::env::var("PG_DATABASE")
        .map_err(|_| "环境变量 PG_DATABASE 未设置".to_string())?;

    let user = std::env::var("PG_USER")
        .map_err(|_| "环境变量 PG_USER 未设置".to_string())?;

    let password = std::env::var("PG_PASSWORD")
        .map_err(|_| "环境变量 PG_PASSWORD 未设置".to_string())?;

    let mut config = Config::new();

    config
        .host(&host)
        .port(port)
        .dbname(&database)
        .user(&user)
        .password(&password);

    config
        .connect(NoTls)
        .map_err(|e| format!("连接 PostgreSQL 失败: {e}"))
}

/// 将 [u8; 20] 转换为十六进制字符串, 仅用于日志.
fn info_hash_to_hex(hash: &[u8; 20]) -> String {
    hash.iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

