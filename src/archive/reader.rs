use std::fs::File;
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};

static BACKUP_SEQ: AtomicU32 = AtomicU32::new(0);

use super::format::{ArchiveEntry, ArchiveFooter, ArchiveHeader};
use crate::archive::CompressionType;

/// 归档读取器（Solid 压缩 v2）
///
/// 打开时仅解析 Header/Footer，不立即解压。首次调用 `prepare()` 或 `extract_entry()`
/// 时将整个压缩块一次性解压到内存（`decompressed` 缓冲），后续每个文件的提取只是
/// 从缓冲中切片写盘，速度极快。
pub struct ArchiveReader {
    file: File,
    header: ArchiveHeader,
    /// 压缩块在文件中的绝对起始偏移 = footer.header_offset - solid_compressed_size
    block_start: u64,
    /// 懒解压缓冲：首次 prepare() 后填充
    decompressed: Option<Vec<u8>>,
}

#[allow(dead_code)]
impl ArchiveReader {
    /// 打开当前运行的 EXE 文件
    pub fn open_current_exe() -> Result<Self, String> {
        let exe_path = std::env::current_exe()
            .map_err(|e| format!("Failed to get current exe path: {}", e))?;
        Self::open(&exe_path)
    }

    /// 打开指定文件并解析归档元数据（不解压数据块）
    pub fn open(path: &Path) -> Result<Self, String> {
        let mut file = File::open(path).map_err(|e| format!("Failed to open file: {}", e))?;

        let file_len = file
            .metadata()
            .map_err(|e| format!("Failed to get file size: {}", e))?
            .len();

        if file_len < 16 {
            return Err("File too small to contain footer".into());
        }

        // 读取尾部 Footer（最后 16 字节）
        file.seek(SeekFrom::End(-16))
            .map_err(|e| format!("Failed to seek to footer: {}", e))?;
        let mut footer_bytes = [0u8; 16];
        file.read_exact(&mut footer_bytes)
            .map_err(|e| format!("Failed to read footer: {}", e))?;
        let footer = ArchiveFooter::from_bytes(&footer_bytes)?;

        // 读取 Header（footer.header_offset 到 footer 之间）
        let header_size = file_len - 16 - footer.header_offset;
        file.seek(SeekFrom::Start(footer.header_offset))
            .map_err(|e| format!("Failed to seek to header: {}", e))?;
        let mut header_bytes = vec![0u8; header_size as usize];
        file.read_exact(&mut header_bytes)
            .map_err(|e| format!("Failed to read header: {}", e))?;
        let header = ArchiveHeader::from_bytes(&header_bytes)?;

        // 压缩块起始 = footer.header_offset - solid_compressed_size
        let block_start = footer
            .header_offset
            .checked_sub(header.solid_compressed_size)
            .ok_or("Invalid archive: block_start underflow")?;

        Ok(Self {
            file,
            header,
            block_start,
            decompressed: None,
        })
    }

    pub fn header(&self) -> &ArchiveHeader {
        &self.header
    }
    pub fn entries(&self) -> &[ArchiveEntry] {
        &self.header.entries
    }
    pub fn compression_type(&self) -> CompressionType {
        self.header.compression
    }

    /// 运行期清单字节（AppManifest 的 TOML 文本），无解压开销。
    pub fn manifest_bytes(&self) -> &[u8] {
        &self.header.manifest
    }
    /// UI logo 图片字节，无解压开销。
    pub fn logo_bytes(&self) -> &[u8] {
        &self.header.logo
    }

    /// 提前将压缩块整体解压到内存缓冲。
    /// 安装向导在进入逐文件循环前调用此方法，可在独立步骤中显示解压进度消息。
    pub fn prepare(&mut self) -> Result<(), String> {
        if self.decompressed.is_none() {
            self.decompress_block()?;
        }
        Ok(())
    }

    /// 从已解压缓冲中提取单个文件到目标路径
    pub fn extract_entry(&mut self, entry: &ArchiveEntry, dest: &Path) -> Result<(), String> {
        if self.decompressed.is_none() {
            self.decompress_block()?;
        }

        let start = entry.offset as usize;
        let size = entry.original_size as usize;
        let expected_crc = entry.crc32;
        let path_str = entry.path.clone();

        // 创建目标目录
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create dir for {}: {}", path_str, e))?;
        }

        // 切片写盘（borrow 限定在此块内）
        {
            let dec = self.decompressed.as_ref().unwrap();
            let end = start + size;
            if end > dec.len() {
                return Err(format!(
                    "Entry '{}' out of bounds in decompressed stream (offset={} size={} total={})",
                    path_str,
                    start,
                    size,
                    dec.len()
                ));
            }
            let slice = &dec[start..end];

            // 验证 CRC32
            let computed = crc32fast::hash(slice);
            if computed != expected_crc {
                return Err(format!(
                    "CRC32 mismatch for '{}': expected {:08x}, got {:08x}",
                    path_str, expected_crc, computed
                ));
            }

            // 写文件（被占用时先改名备份）
            let mut output = BufWriter::new(create_or_backup(dest)?);
            output
                .write_all(slice)
                .map_err(|e| format!("Failed to write '{}': {}", path_str, e))?;
        }

        Ok(())
    }

    /// 将所有文件解压到目标目录（一次性解压块，逐文件写盘）
    pub fn extract_all(&mut self, dest_dir: &Path) -> Result<(), String> {
        self.prepare()?;
        let entries = self.header.entries.clone();
        for entry in &entries {
            let dest = dest_dir.join(&entry.path);
            self.extract_entry(entry, &dest)?;
        }
        Ok(())
    }

    /// 将整个压缩块读入内存并解压
    fn decompress_block(&mut self) -> Result<(), String> {
        self.file
            .seek(SeekFrom::Start(self.block_start))
            .map_err(|e| format!("Failed to seek to compressed block: {}", e))?;

        let compressed_size = self.header.solid_compressed_size as usize;
        let mut compressed = vec![0u8; compressed_size];
        self.file
            .read_exact(&mut compressed)
            .map_err(|e| format!("Failed to read compressed block: {}", e))?;

        let decompressed = match self.header.compression {
            CompressionType::Zstd => zstd::decode_all(&compressed[..])
                .map_err(|e| format!("Zstd decompression failed: {}", e))?,
            CompressionType::Lzma => {
                let mut decoder = xz2::read::XzDecoder::new(&compressed[..]);
                let mut out = Vec::new();
                decoder
                    .read_to_end(&mut out)
                    .map_err(|e| format!("LZMA decompression failed: {}", e))?;
                out
            }
        };

        self.decompressed = Some(decompressed);
        Ok(())
    }
}

/// 创建输出文件；若目标被其他进程占用（os error 5 / 32），将其改名为 .old_<seq> 后重试
fn create_or_backup(path: &Path) -> Result<File, String> {
    match File::create(path) {
        Ok(f) => return Ok(f),
        Err(ref e) if matches!(e.raw_os_error(), Some(5) | Some(32)) => {
            let seq = BACKUP_SEQ.fetch_add(1, Ordering::Relaxed);
            let suffix = std::process::id().wrapping_add(seq);
            let old_name = format!(
                "{}.old_{:08x}",
                path.file_name().unwrap_or_default().to_string_lossy(),
                suffix
            );
            let old_path = path.parent().unwrap_or(Path::new(".")).join(old_name);
            std::fs::rename(path, &old_path)
                .map_err(|e| format!("Failed to rename locked file: {}", e))?;
            // 改名只是让路：那份旧文件仍被进程占用、当前删不掉，排进重启删除队列兜底，
            // 否则 .old_ 会随每次带锁升级永久累积。best-effort——失败不影响新文件写入。
            //
            // 门控到 Windows：`util` 是 cfg(windows) 模块，而 archive 必须保持跨平台
            // ——wind-packer 要在 Linux 原生构建（见 lib.rs 与 ci.yml 的 linux-packer job）。
            // 非 Windows 上这个分支本就走不到：进入它的条件是 os error 5/32，即 Windows 的
            // ACCESS_DENIED / SHARING_VIOLATION，类 Unix 系统允许 unlink 仍被打开的文件。
            #[cfg(windows)]
            if let Err(e) = crate::util::reboot::schedule_delete_on_reboot(&old_path) {
                eprintln!("Warning: {}", e);
            }
        }
        Err(e) => return Err(format!("Failed to create output file: {}", e)),
    }
    File::create(path).map_err(|e| format!("Failed to create file after backup: {}", e))
}
