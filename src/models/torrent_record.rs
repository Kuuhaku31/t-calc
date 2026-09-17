
// src/models/torrent_record.rs

use bitors::{parse_torrent, torrent::FileMode};
use std::collections::HashSet;


pub(crate) struct TorrentRecord {

    /// 种子文件的二进制数据
    data: Vec<u8>,

    /// 种子文件大小 (data 的长度)
    size: usize,

    /// 种子文件的 info_hash, 作为种子唯一标识
    info_hash: [u8; 20],

    /// 种子文件标题
    /// 如果标题为空, 用 info_hash 代替
    title: String,

    /// 文件总大小
    file_size: usize,

    /// 文件数量
    file_count: usize,

    /// 文件夹数量
    ///
    /// 多文件 Torrent 中, 统计所有唯一目录路径.
    /// 不包含 info.name 所代表的最外层根目录.
    folder_count: usize,
}

impl TorrentRecord {
    pub(crate) fn get_data         (&self) ->&Vec<u8>  {&self.data         }
    #[allow(dead_code)]
    pub(crate) fn get_size         (&self) -> usize    { self.size         }
    pub(crate) fn get_info_hash    (&self) ->&[u8; 20] {&self.info_hash    }
    pub(crate) fn get_title        (&self) ->&str      {&self.title        }
    pub(crate) fn get_file_size    (&self) -> usize    { self.file_size    }
    pub(crate) fn get_file_count   (&self) -> usize    { self.file_count   }
    pub(crate) fn get_folder_count (&self) -> usize    { self.folder_count }

}

impl TorrentRecord {

    pub(crate) fn new(data: Vec<u8>) -> Result<Self, String> {

        if data.is_empty() { return Err("torrent 数据为空".to_string()); }

        let torrent = parse_torrent(&data).map_err(|e| format!("解析 torrent 失败: {e}"))?;

        let info_hash = torrent.info_hash();

        let title = if torrent.info.name.is_empty() {
            {
                let hash: &[u8; 20] = &info_hash;
                let mut result = String::with_capacity(40);
                for byte in hash {
                    result.push_str(&format!("{byte:02x}"));
                }
                result
            }
        } else {
            torrent.info.name.to_string()
        };

        let file_size = usize::try_from(torrent.total_size())
            .map_err(|_| "torrent 文件总大小超出 usize 范围".to_string())?;

        let file_count = torrent.file_count();

        let folder_count = match &torrent.info.file_mode {
            FileMode::Single { .. } => 0,

            FileMode::Multi { files } => {
                let mut folders: HashSet<Vec<&str>> = HashSet::new();

                for file in files {
                    // path 的最后一个元素是文件名，
                    // 前面的元素是目录。
                    //
                    // ["dir", "sub", "file.txt"]
                    // =>
                    // ["dir"]
                    // ["dir", "sub"]

                    if file.path.len() <= 1 {
                        continue;
                    }

                    let mut folder_path = Vec::with_capacity(file.path.len() - 1);

                    for component in &file.path[..file.path.len() - 1] {
                        folder_path.push(component.as_ref());
                        folders.insert(folder_path.clone());
                    }
                }

                folders.len()
            }
        };

        Ok(Self {
            size: data.len(),
            data,
            info_hash,
            title,
            file_size,
            file_count,
            folder_count,
        })
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use bitors::parse_torrent;
    use std::fs;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join(name)
    }

    fn read_fixture(name: &str) -> Vec<u8> {
        fs::read(fixture(name)).unwrap_or_else(|e| panic!("读取 fixture {name:?} 失败: {e}"))
    }

    fn hash_hex(hash: &[u8; 20]) -> String {
        hash.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn single_file_fixture() {
        let data = read_fixture("single-file.torrent");

        let record = TorrentRecord::new(data.clone()).unwrap();

        assert_eq!(record.get_data(), &data);
        assert_eq!(record.get_size(), data.len());

        assert_eq!(
            hash_hex(record.get_info_hash()),
            "0eb2d544ef0c9ae3c0fbba90a71cd742a44aa81d"
        );

        assert_eq!(record.get_title(), "hello.txt");
        assert_eq!(record.get_file_size(), 5);
        assert_eq!(record.get_file_count(), 1);
        assert_eq!(record.get_folder_count(), 0);
    }

    #[test]
    fn multi_file_fixture() {
        let data = read_fixture("multi-file.torrent");

        let record = TorrentRecord::new(data.clone()).unwrap();

        assert_eq!(record.get_data(), &data);
        assert_eq!(record.get_size(), data.len());

        assert_eq!(
            hash_hex(record.get_info_hash()),
            "21b3752489e0c0728818bcdeb3a63adf67c3832e"
        );

        assert_eq!(record.get_title(), "fixture-directory");

        // 3 + 5 + 3 = 11
        assert_eq!(record.get_file_size(), 11);

        assert_eq!(record.get_file_count(), 3);

        // root.txt
        // dir/b.txt
        // dir/sub/c.txt
        //
        // 唯一目录：
        // dir
        // dir/sub
        assert_eq!(record.get_folder_count(), 2);
    }

    #[test]
    fn fixture_info_hash_matches_bitors() {
        for name in ["single-file.torrent", "multi-file.torrent"] {
            let data = read_fixture(name);

            let record = TorrentRecord::new(data.clone()).unwrap();

            let parsed = parse_torrent(&data).unwrap();

            assert_eq!(record.get_info_hash(), &parsed.info_hash());

            assert_eq!(record.get_file_size() as u64, parsed.total_size());

            assert_eq!(record.get_file_count(), parsed.file_count());
        }
    }

    #[test]
    fn corrupted_fixture_is_rejected() {
        let mut data = read_fixture("single-file.torrent");

        data.pop();

        assert!(TorrentRecord::new(data).is_err());
    }
}
