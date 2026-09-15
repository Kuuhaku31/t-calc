
// src/main.rs
// 主程序入口

mod utils;
mod models;
mod sqlite;
mod ods;
mod cli;
mod postgres;

use std::env;

use crate::cli::Command;


fn main() {

    let cmd = cli::parse_command().unwrap_or_else(|e| {
        eprintln!("错误: {e}");
        print_help();
        std::process::exit(1);
    });
    // 打印命令和选项
    println!("命令: {:?}", cmd.as_string());

    match env::args().nth(1).as_deref() {
        Some("print") => {
            if let Err(e) = run_print(cmd) {
                eprintln!("错误: {e}");
                print_help();
                std::process::exit(1);
            }
        }

        Some("to-sqlite") => {
                if let Err(e) = run_to_sqlite(cmd) {
                eprintln!("错误: {e}");
                print_help();
                std::process::exit(1);
            }
        }

        Some("to-postgres") => {
            if let Err(e) = run_to_postgres(cmd) {
                eprintln!("错误: {e}");
                print_help();
                std::process::exit(1);
            }
        }

        _ => {
            print_help();
            std::process::exit(1);
        }
    }
}

fn run_print(cmd: Command) -> Result<(), String> {

    print!("开始打印 ODS 数据");

    let path = cmd.command.get(1).ok_or("缺少 ODS 文件路径")?;
    let sheet_name = cmd.command.get(2).ok_or("缺少工作表名")?;
    let range = cmd.command.get(3).ok_or("缺少区域, 例如 B3:E373")?;

    let table = ods::read_ods_table(path, sheet_name, range)?;

    println!(
        "Table:\n行数: {}, 列数: {}\n{}",
        table.get_row_count(),
        table.get_column_count(),
        table.as_string()
    );

    Ok(())
}

fn run_to_sqlite(cmd: Command) -> Result<(), String> {

    println!("开始插入更新 SQLite");

    let path = cmd.command.get(1).ok_or("缺少 ODS 文件路径")?;
    let sheet_name = cmd.command.get(2).ok_or("缺少工作表名")?;
    let range = cmd.command.get(3).ok_or("缺少区域, 例如 B3:E373")?;
    let db_path = cmd.command.get(4).ok_or("缺少 SQLite 数据库路径")?;
    let table_name = cmd.command.get(5).ok_or("缺少表名")?;

    let primary_keys: Vec<String> =
        cmd.command.iter().skip(6).cloned().collect();

    if primary_keys.is_empty() {
        return Err("至少需要一个主键列名".to_string());
    }

    let table = ods::read_ods_table(path, sheet_name, range)?;

    sqlite::upsert_sqlite(
        db_path,
        table_name,
        table,
        primary_keys,
    )?;

    Ok(())
}

fn run_to_postgres(cmd: Command) -> Result<(), String> {

    println!("开始插入更新 PostgreSQL");

    // ods 文件路径从环境变量读取
    dotenvy::dotenv().map_err(|e| format!("加载 .env 失败: {e}"))?;
    let path = env::var("ODS_PATH").map_err(|_| "缺少环境变量 ODS_PATH")?;
    let sheet_name = cmd.command.get(1).ok_or("缺少工作表名")?;
    let range = cmd.command.get(2).ok_or("缺少区域, 例如 B3:E373")?;
    let table_name = cmd.command.get(3).ok_or("缺少数据库表名")?;

    let primary_keys: Vec<String> =
        cmd.command.iter().skip(4).cloned().collect();
    if primary_keys.is_empty() {
        return Err("至少需要一个主键列名".to_string());
    }

    let table = ods::read_ods_table(&path, sheet_name, range)?;

    postgres::upsert_postgresql(
        table,
        table_name,
        primary_keys,
    )?;

    Ok(())
}


fn print_help() {
    println!(
"
用法:
    cargo run print <ods 文件路径> <工作表名> <区域>
        -- 打印指定区域的数据

    cargo run to-sqlite <ods 文件路径> <工作表名> <区域> <sqlite 数据库路径> <表名> <主键列名1 主键列名2 ...>
        -- 将指定区域的数据导入 SQLite 数据库

    cargo run to-postgres <工作表名> <区域> <数据库表名> <列名1 列名2 ...>
        -- 将指定区域的数据导入 PostgreSQL, ODS文件路径/数据库连接参数从环境变量读取:
            ODS_PATH, PG_HOST, PG_PORT, PG_USER, PG_PASSWORD, PG_DATABASE, PG_TABLE
"
    );
}
