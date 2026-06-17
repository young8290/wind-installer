use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use super::format::{ArchiveEntry, ArchiveFooter, ArchiveHeader, CompressionType, MAGIC_HEADER};

/// 归档写入器（Solid 压缩）
///
/// 先将所有文件数据收集到内存，`finish()` 时将全部原始数据拼成一条流整体压缩一次，
/// 再写入 Header 和 Footer。相比逐文件压缩，压缩器可跨文件复用字典，对含大量
/// 相似文件（词典 YAML 等）的归档压缩率提升显著。
pub struct ArchiveWriter {
    compression: CompressionType,
    output: File,
    /// (archive_path, raw_data)
    pending: Vec<(String, Vec<u8>)>,
}

impl ArchiveWriter {
    pub fn new(output_path: &Path, compression: CompressionType) -> Result<Self, String> {
        let output = File::create(output_path)
            .map_err(|e| format!("Failed to create output file: {}", e))?;
        Ok(Self { compression, output, pending: Vec::new() })
    }

    /// 将文件加入待压缩队列（暂存到内存，不立即写盘）
    pub fn add_file(&mut self, source_path: &Path, archive_path: &str) -> Result<(), String> {
        let mut f = File::open(source_path)
            .map_err(|e| format!("Failed to open {}: {}", source_path.display(), e))?;
        let mut data = Vec::new();
        f.read_to_end(&mut data)
            .map_err(|e| format!("Failed to read {}: {}", source_path.display(), e))?;
        self.pending.push((archive_path.to_string(), data));
        Ok(())
    }

    /// 递归添加目录
    pub fn add_directory(&mut self, dir_path: &Path, base_path: &str) -> Result<(), String> {
        for entry in std::fs::read_dir(dir_path)
            .map_err(|e| format!("Failed to read directory {}: {}", dir_path.display(), e))?
        {
            let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
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

    /// 固实压缩并写入归档，返回 Header 在文件中的偏移（= solid_compressed_size）
    pub fn finish(mut self) -> Result<u64, String> {
        // 1. 构建 entry 表，同时将所有原始数据拼接为一条流
        let mut combined: Vec<u8> = Vec::new();
        let mut entries: Vec<ArchiveEntry> = Vec::new();
        let mut decompressed_offset: u64 = 0;

        for (path, data) in &self.pending {
            let original_size = data.len() as u64;
            let crc32 = crc32fast::hash(data);
            entries.push(ArchiveEntry {
                path: path.clone(),
                offset: decompressed_offset,   // 解压后流中的字节起点
                compressed_size: 0,            // solid 模式无意义
                original_size,
                crc32,
            });
            combined.extend_from_slice(data);
            decompressed_offset += original_size;
        }

        // 2. 整体压缩一次
        let compressed = compress_solid(&combined, self.compression)?;
        let solid_compressed_size = compressed.len() as u64;

        // 3. 写入压缩块
        self.output.write_all(&compressed)
            .map_err(|e| format!("Failed to write compressed block: {}", e))?;

        // 4. 构建并写入 Header
        let mut header = ArchiveHeader::new(self.compression);
        header.magic = *MAGIC_HEADER;
        header.entry_count = entries.len() as u32;
        header.solid_compressed_size = solid_compressed_size;
        header.entries = entries;

        let header_bytes = header.to_bytes();
        let header_offset = solid_compressed_size; // .bin 中 Header 从此处开始
        self.output.write_all(&header_bytes)
            .map_err(|e| format!("Failed to write header: {}", e))?;

        // 5. 写入 Footer
        let footer = ArchiveFooter::new(header_offset);
        self.output.write_all(&footer.to_bytes())
            .map_err(|e| format!("Failed to write footer: {}", e))?;

        Ok(header_offset)
    }

    pub fn entry_count(&self) -> u32 {
        self.pending.len() as u32
    }
}

fn compress_solid(data: &[u8], compression: CompressionType) -> Result<Vec<u8>, String> {
    match compression {
        CompressionType::Zstd => {
            zstd::encode_all(data, 19)
                .map_err(|e| format!("Zstd compression failed: {}", e))
        }
        CompressionType::Lzma => {
            let mut encoder = xz2::write::XzEncoder::new(Vec::new(), 9);
            encoder.write_all(data)
                .map_err(|e| format!("LZMA write failed: {}", e))?;
            encoder.finish()
                .map_err(|e| format!("LZMA finish failed: {}", e))
        }
    }
}
