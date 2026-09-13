
mod utils;
mod models;

use calamine::{Ods, OdsError, Reader};
use std::env;

use crate::models::Table;


fn main() {
    if let Err(e) = run() {
        eprintln!("错误: {e}");
        print_help();
        std::process::exit(1);
    }
}


fn run() -> Result<(), String> {

    let path       = env::args().nth(1).ok_or("缺少 ODS 文件路径")?;
    let sheet_name = env::args().nth(2).ok_or("缺少工作表名")?;
    let range      = env::args().nth(3).ok_or("缺少区域, 例如 B3:E373")?;

    // 打开 ODS
    let mut workbook: Ods<_> = calamine::open_workbook(path)
    .map_err(|e: OdsError| e.to_string())?;

    // 读取指定的工作表
    let sheet = sheet_name.as_str();

    // 获取工作表数据
    let data = workbook.worksheet_range(&sheet).map_err(|e| e.to_string())?;

    // 解析区域
    // 这里是基于整个 sheet 的 0-based 索引
    let (start_col, start_row, end_col, end_row) = utils::parse_range(&range).map_err(|e| e.to_string())?;
    println!("读取区域: {start_col}, {start_row}, {end_col}, {end_row}");

    // Range 在工作表中的实际起始位置
    let (data_start_row, data_start_col) = data.start()
    .ok_or_else(|| "工作表为空".to_string())?;
    let row_offset = data_start_row as usize;
    let col_offset = data_start_col as usize;

    // 数据区域内的起始和结束行列
    let start_row = start_row.saturating_sub(row_offset);
    let start_col = start_col.saturating_sub(col_offset);
    let end_row = end_row.saturating_sub(row_offset);
    let end_col = end_col.saturating_sub(col_offset);

    // 获取数据
    let mut table = Table::new(end_col - start_col + 1, end_row - start_row + 1);
    for row in start_row..=end_row {
        for col in start_col..=end_col {
            match data.get((row, col)) {
                Some(value) => table.set(row - start_row, col - start_col, value.to_string()),
                None        => table.set(row - start_row, col - start_col, String::new()),
            }
        }
    }

    println!("Table:\n行数: {}, 列数: {}\n{}",
        table.get_row_count(), table.get_column_count(), table.as_string()
    );

    Ok(())
}


fn print_help() {
    println!(
"
用法:
    cargo run -- <ods 文件路径> <工作表名> <区域>
");
}
