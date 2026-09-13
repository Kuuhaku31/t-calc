
// 工具函数

/// 把 B3:E373 转换成:
/// (1, 2, 4, 372)
/// 行列均使用 0-based
pub(crate) fn parse_range(s: &str) -> Result<(usize, usize, usize, usize), Box<dyn std::error::Error>> {
    let (a, b) = s.split_once(':').ok_or("区域格式错误, 例如 B3:E373")?;

    let (c1, r1) = parse_cell(a)?;
    let (c2, r2) = parse_cell(b)?;

    Ok((c1, r1, c2, r2))
}

/// 解析单元格, 例如 B3 -> (1, 2)
pub(crate) fn parse_cell(s: &str) -> Result<(usize, usize), Box<dyn std::error::Error>> {

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
