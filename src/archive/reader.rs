use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use super::format::{ArchiveEntry, ArchiveFooter, ArchiveHeader};
use crate::archive::CompressionType;

/// 归档读取器 - 从 EXE 文件中读取并流式解压
pub struct ArchiveReader {
    file: File,
    header: ArchiveHeader,
}

#[allow(dead_code)]
impl ArchiveReader {
    /// 打开当前 EXE 文件并解析归档
    pub fn open_current_exe() -> Result<Self, String> {
        let exe_path = std::env::current_exe()
            .map_err(|e| format!("Failed to get current exe path: {}", e))?;
        Self::open(&exe_path)
    }

    /// 打开指定文件并解析归档
    pub fn open(path: &Path) -> Result<Self, String> {
        let mut file = File::open(path)
            .map_err(|e| format!("Failed to open file: {}", e))?;

        // 读取尾部 Footer（最后 16 字节）
        let file_len = file.metadata()
            .map_err(|e| format!("Failed to get file size: {}", e))?
            .len();

        if file_len < 16 {
            return Err("File too small to contain footer".into());
        }

        file.seek(SeekFrom::End(-16))
            .map_err(|e| format!("Failed to seek to footer: {}", e))?;

        let mut footer_bytes = [0u8; 16];
        file.read_exact(&mut footer_bytes)
            .map_err(|e| format!("Failed to read footer: {}", e))?;

        let footer = ArchiveFooter::from_bytes(&footer_bytes)?;

        // 读取 Header
        let header_size = file_len - 16 - footer.header_offset;
        file.seek(SeekFrom::Start(footer.header_offset))
            .map_err(|e| format!("Failed to seek to header: {}", e))?;

        let mut header_bytes = vec![0u8; header_size as usize];
        file.read_exact(&mut header_bytes)
            .map_err(|e| format!("Failed to read header: {}", e))?;

        let header = ArchiveHeader::from_bytes(&header_bytes)?;

        Ok(Self { file, header })
    }

    /// 获取归档头部信息
    pub fn header(&self) -> &ArchiveHeader {
        &self.header
    }

    /// 获取所有文件条目
    pub fn entries(&self) -> &[ArchiveEntry] {
        &self.header.entries
    }

    /// 获取压缩类型
    pub fn compression_type(&self) -> CompressionType {
        self.header.compression
    }

    /// 流式解压单个文件到目标路径
    pub fn extract_entry(&mut self, entry: &ArchiveEntry, dest: &Path) -> Result<(), String> {
        // 定位到压缩数据
        self.file.seek(SeekFrom::Start(entry.offset))
            .map_err(|e| format!("Failed to seek to entry data: {}", e))?;

        // 创建目标文件的父目录
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create parent directory: {}", e))?;
        }

        // 创建目标文件
        let mut output = File::create(dest)
            .map_err(|e| format!("Failed to create output file: {}", e))?;

        // 根据压缩类型选择解压器
        match self.header.compression {
            CompressionType::Zstd => {
                let decoder = zstd::Decoder::new(&mut self.file)
                    .map_err(|e| format!("Failed to create Zstd decoder: {}", e))?;
                let mut limited = Read::take(decoder, entry.compressed_size);
                std::io::copy(&mut limited, &mut output)
                    .map_err(|e| format!("Failed to decompress: {}", e))?;
            }
            CompressionType::Lzma => {
                let decoder = xz2::read::XzDecoder::new(&mut self.file);
                let mut limited = Read::take(decoder, entry.compressed_size);
                std::io::copy(&mut limited, &mut output)
                    .map_err(|e| format!("Failed to decompress: {}", e))?;
            }
        }

        // 验证 CRC32
        let output_file = File::open(dest)
            .map_err(|e| format!("Failed to reopen output for verification: {}", e))?;
        let mut hasher = crc32fast::Hasher::new();
        let mut buf_reader = BufReader::new(output_file);
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = buf_reader.read(&mut buf)
                .map_err(|e| format!("Failed to read for CRC: {}", e))?;
            if n == 0 { break; }
            hasher.update(&buf[..n]);
        }
        let computed_crc = hasher.finalize();
        if computed_crc != entry.crc32 {
            return Err(format!(
                "CRC32 mismatch for {}: expected {:08x}, got {:08x}",
                entry.path, entry.crc32, computed_crc
            ));
        }

        Ok(())
    }

    /// 流式解压所有文件到目标目录
    pub fn extract_all(&mut self, dest_dir: &Path) -> Result<(), String> {
        let entries = self.header.entries.clone();
        for entry in &entries {
            let dest = dest_dir.join(&entry.path);
            self.extract_entry(entry, &dest)?;
        }
        Ok(())
    }
}
