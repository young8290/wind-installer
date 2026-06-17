use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use super::format::{ArchiveEntry, ArchiveFooter, ArchiveHeader, CompressionType};

/// 归档写入器 - 用于打包文件
pub struct ArchiveWriter {
    header: ArchiveHeader,
    output: File,
    current_offset: u64,
}

impl ArchiveWriter {
    /// 创建新的归档写入器
    pub fn new(output_path: &Path, compression: CompressionType) -> Result<Self, String> {
        let output = File::create(output_path)
            .map_err(|e| format!("Failed to create output file: {}", e))?;

        Ok(Self {
            header: ArchiveHeader::new(compression),
            output,
            current_offset: 0,
        })
    }

    /// 添加文件到归档
    pub fn add_file(&mut self, source_path: &Path, archive_path: &str) -> Result<(), String> {
        let mut input = File::open(source_path)
            .map_err(|e| format!("Failed to open source file: {}", e))?;

        let metadata = input.metadata()
            .map_err(|e| format!("Failed to get file metadata: {}", e))?;
        let original_size = metadata.len();

        // 读取原始数据
        let mut original_data = Vec::new();
        input.read_to_end(&mut original_data)
            .map_err(|e| format!("Failed to read source file: {}", e))?;

        // 计算 CRC32
        let crc32 = crc32fast::hash(&original_data);

        // 压缩数据
        let compressed_data = match self.header.compression {
            CompressionType::Zstd => {
                zstd::encode_all(&original_data[..], 19)
                    .map_err(|e| format!("Failed to compress with Zstd: {}", e))?
            }
            CompressionType::Lzma => {
                let mut encoder = xz2::write::XzEncoder::new(Vec::new(), 9);
                encoder.write_all(&original_data)
                    .map_err(|e| format!("Failed to write to LZMA encoder: {}", e))?;
                encoder.finish()
                    .map_err(|e| format!("Failed to finish LZMA compression: {}", e))?
            }
        };

        let compressed_size = compressed_data.len() as u64;

        // 写入压缩数据
        self.output.write_all(&compressed_data)
            .map_err(|e| format!("Failed to write compressed data: {}", e))?;

        // 记录条目
        let entry = ArchiveEntry {
            path: archive_path.to_string(),
            offset: self.current_offset,
            compressed_size,
            original_size,
            crc32,
        };
        self.header.entries.push(entry);
        self.header.entry_count += 1;
        self.current_offset += compressed_size;

        Ok(())
    }

    /// 添加目录到归档（递归）
    pub fn add_directory(&mut self, dir_path: &Path, base_path: &str) -> Result<(), String> {
        for entry in std::fs::read_dir(dir_path)
            .map_err(|e| format!("Failed to read directory: {}", e))?
        {
            let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
            let path = entry.path();
            let file_name = entry.file_name().to_string_lossy().to_string();
            let archive_path = if base_path.is_empty() {
                file_name.clone()
            } else {
                format!("{}/{}", base_path, file_name)
            };

            if path.is_dir() {
                self.add_directory(&path, &archive_path)?;
            } else {
                self.add_file(&path, &archive_path)?;
            }
        }

        Ok(())
    }

    /// 完成归档写入，返回 Header 偏移量
    pub fn finish(mut self) -> Result<u64, String> {
        // 写入 Header
        let header_bytes = self.header.to_bytes();
        let header_offset = self.current_offset;
        self.output.write_all(&header_bytes)
            .map_err(|e| format!("Failed to write header: {}", e))?;

        // 写入 Footer
        let footer = ArchiveFooter::new(header_offset);
        self.output.write_all(&footer.to_bytes())
            .map_err(|e| format!("Failed to write footer: {}", e))?;

        Ok(header_offset)
    }

    /// 获取当前已添加的文件数量
    pub fn entry_count(&self) -> u32 {
        self.header.entry_count
    }
}
