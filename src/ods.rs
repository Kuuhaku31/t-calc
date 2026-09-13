
use calamine::{Ods, OdsError, Reader};

use crate::models::Table;


/// 读取 ODS 指定 Sheet 的指定区域, 并转换成 Table
pub(crate) fn read_ods_table(
    path: &str,
    sheet_name: &str,
    range: &str,
) -> Result<Table, String> {
    // 打开 ODS
    let mut workbook: Ods<_> =
        calamine::open_workbook(path)
            .map_err(|e: OdsError| e.to_string())?;

    // 获取指定工作表
    let data = workbook
        .worksheet_range(sheet_name)
        .map_err(|e| e.to_string())?;

    // 解析用户输入的区域
    //
    // 这里得到的是相对于整个 Sheet 的 0-based 坐标
    let (start_col, start_row, end_col, end_row) =
        parse_range(range).map_err(|e| e.to_string())?;

    println!(
        "读取区域: {start_col}, {start_row}, {end_col}, {end_row}"
    );

    // ------------------------------------------------------------
    // 修正 calamine Range 的起始偏移
    // ------------------------------------------------------------

    let (data_start_row, data_start_col) = data
        .start()
        .ok_or_else(|| "工作表为空".to_string())?;

    let row_offset = data_start_row as usize;
    let col_offset = data_start_col as usize;

    let start_row = start_row.saturating_sub(row_offset);
    let start_col = start_col.saturating_sub(col_offset);
    let end_row = end_row.saturating_sub(row_offset);
    let end_col = end_col.saturating_sub(col_offset);

    // ------------------------------------------------------------
    // 构造 Table
    // ------------------------------------------------------------

    let row_count = end_row - start_row + 1;
    let column_count = end_col - start_col + 1;

    let mut table = Table::new(
        column_count,
        row_count,
    );

    // ------------------------------------------------------------
    // 读取数据
    // ------------------------------------------------------------

    for row in start_row..=end_row {
        for col in start_col..=end_col {
            let value = match data.get((row, col)) {
                Some(value) => value.to_string(),
                None => String::new(),
            };

            table.set(
                row - start_row,
                col - start_col,
                value,
            );
        }
    }

    Ok(table)
}



// 工具函数

/// 把 B3:E373 转换成:
/// (1, 2, 4, 372)
/// 行列均使用 0-based
fn parse_range(s: &str) -> Result<(usize, usize, usize, usize), Box<dyn std::error::Error>> {
    let (a, b) = s.split_once(':').ok_or("区域格式错误, 例如 B3:E373")?;

    let (c1, r1) = parse_cell(a)?;
    let (c2, r2) = parse_cell(b)?;

    Ok((c1, r1, c2, r2))
}

/// 解析单元格, 例如 B3 -> (1, 2)
fn parse_cell(s: &str) -> Result<(usize, usize), Box<dyn std::error::Error>> {

    let split = s.find(|c: char| c.is_ascii_digit())
        .ok_or("缺少行号")?;

    let col_str = &s[..split];
    let row_str = &s[split..];

    let mut col = 0usize;

    for c in col_str.chars() {
        col = col * 26 + (c.to_ascii_uppercase() as usize - 'A' as usize + 1);
    }

    let row: usize = row_str.parse()?;

    Ok((col - 1, row - 1))
}
