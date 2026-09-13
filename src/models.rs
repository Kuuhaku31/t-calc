
// src/models.rs
// 数据模型


pub struct Table {

    column_count: usize, // 列数
    row_count: usize,    // 行数

    data: Vec<String>,   // 数据, 按行优先存储, 大小为 column_count * row_count

}

impl Table {

    pub(crate) fn new(column_count: usize, row_count: usize) -> Self {
        let data = vec![String::new(); column_count * row_count];
        Self {
            column_count,
            row_count,
            data,
        }
    }

    pub(crate) fn set(&mut self, row: usize, col: usize, value: String) {
        let index = row * self.column_count + col;
        if index < self.data.len() {
            self.data[index] = value;
        }
    }

    pub fn get_row_count(&self) -> usize { self.row_count }
    pub fn get_column_count(&self) -> usize { self.column_count }
    pub fn get_data(&self) -> &Vec<String> { &self.data }

    pub fn as_string(&self) -> String {
        let mut result = String::new();
        for row in 0..self.row_count {
            for col in 0..self.column_count {
                if let Some(value) =
                self.get_data().get(row * self.column_count + col) {
                    result.push_str(value);
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
