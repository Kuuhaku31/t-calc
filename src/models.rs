
// src/models.rs
// 数据模型

mod torrent_record;
pub(crate) use torrent_record::TorrentRecord;

pub struct Table {

    column_count: usize, // 列数
    row_count: usize,    // 行数

    /// 数据, 按行优先存储, 大小为 column_count * row_count,
    /// 第一行数据是表头
    data: Vec<Option<String>>,

}

impl Table {

    pub(crate) fn new(column_count: usize, row_count: usize) -> Self {
        let data = vec![None; column_count * row_count];
        Self {
            column_count,
            row_count,
            data,
        }
    }

    pub(crate) fn set(&mut self, row: usize, col: usize, value: String) {

        // 如果为空字符串, 则不设置
        if value.trim().is_empty() { return; }

        let index = row * self.column_count + col;
        if index < self.data.len() {
            self.data[index] = Some(value);
        }
    }

    pub fn get_row_count(&self) -> usize { self.row_count }
    pub fn get_column_count(&self) -> usize { self.column_count }
    pub fn get_data(&self) -> &Vec<Option<String>> { &self.data }

    pub fn as_string(&self) -> String {
        let mut result = String::new();
        for row in 0..self.row_count {
            for col in 0..self.column_count {
                if let Some(value) =
                self.get_data().get(row * self.column_count + col) {
                    result.push_str(value.as_ref().unwrap());
                }
                if col < self.column_count - 1 {
                    result.push('\t');
                }
            }
            result.push('\n');
        }
        result
    }
}
