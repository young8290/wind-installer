/// 归档魔数
pub const MAGIC_HEADER: &[u8; 8] = b"WINDPKG\0";
pub const MAGIC_FOOTER: &[u8; 8] = b"WINDEND\0";

/// 当前格式版本
pub const FORMAT_VERSION: u32 = 1;

/// 压缩算法类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[repr(u8)]
pub enum CompressionType {
    /// Zstd - 解压速度快，内存可控
    Zstd = 0,
    /// LZMA - 压缩率最高
    Lzma = 1,
}

impl CompressionType {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Zstd),
            1 => Some(Self::Lzma),
            _ => None,
        }
    }
}

/// 单个文件条目
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ArchiveEntry {
    /// 相对路径（如 "wind_tsf.dll", "data/schemas/wubi86/wubi86.dict.yaml"）
    pub path: String,
    /// 压缩数据在文件中的绝对偏移
    pub offset: u64,
    /// 压缩后大小
    pub compressed_size: u64,
    /// 原始大小
    pub original_size: u64,
    /// 原始数据的 CRC32
    pub crc32: u32,
}

/// 归档头部
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ArchiveHeader {
    /// 魔数 "WINDPKG\0"
    pub magic: [u8; 8],
    /// 格式版本
    pub version: u32,
    /// 压缩类型
    pub compression: CompressionType,
    /// 文件条目数量
    pub entry_count: u32,
    /// 文件条目列表
    pub entries: Vec<ArchiveEntry>,
}

impl ArchiveHeader {
    /// 创建新的头部
    pub fn new(compression: CompressionType) -> Self {
        Self {
            magic: *MAGIC_HEADER,
            version: FORMAT_VERSION,
            compression,
            entry_count: 0,
            entries: Vec::new(),
        }
    }

    /// 序列化头部为字节
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&self.magic);
        buf.extend_from_slice(&self.version.to_le_bytes());
        buf.push(self.compression as u8);
        buf.extend_from_slice(&self.entry_count.to_le_bytes());

        for entry in &self.entries {
            let path_bytes = entry.path.as_bytes();
            buf.extend_from_slice(&(path_bytes.len() as u16).to_le_bytes());
            buf.extend_from_slice(path_bytes);
            buf.extend_from_slice(&entry.offset.to_le_bytes());
            buf.extend_from_slice(&entry.compressed_size.to_le_bytes());
            buf.extend_from_slice(&entry.original_size.to_le_bytes());
            buf.extend_from_slice(&entry.crc32.to_le_bytes());
        }

        buf
    }

    /// 从字节反序列化头部
    pub fn from_bytes(data: &[u8]) -> Result<Self, String> {
        if data.len() < 17 {
            return Err("Header too short".into());
        }

        let mut magic = [0u8; 8];
        magic.copy_from_slice(&data[0..8]);
        if &magic != MAGIC_HEADER {
            return Err("Invalid header magic".into());
        }

        let version = u32::from_le_bytes(data[8..12].try_into().unwrap());
        if version != FORMAT_VERSION {
            return Err(format!("Unsupported format version: {}", version));
        }

        let compression = CompressionType::from_u8(data[12])
            .ok_or_else(|| "Invalid compression type".to_string())?;

        let entry_count = u32::from_le_bytes(data[13..17].try_into().unwrap());
        let mut entries = Vec::with_capacity(entry_count as usize);
        let mut pos = 17;

        for _ in 0..entry_count {
            if pos + 2 > data.len() {
                return Err("Truncated entry path length".into());
            }
            let path_len = u16::from_le_bytes(data[pos..pos + 2].try_into().unwrap()) as usize;
            pos += 2;

            if pos + path_len > data.len() {
                return Err("Truncated entry path".into());
            }
            let path = String::from_utf8(data[pos..pos + path_len].to_vec())
                .map_err(|_| "Invalid UTF-8 in path")?;
            pos += path_len;

            if pos + 28 > data.len() {
                return Err("Truncated entry fields".into());
            }
            let offset = u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap());
            let compressed_size = u64::from_le_bytes(data[pos + 8..pos + 16].try_into().unwrap());
            let original_size = u64::from_le_bytes(data[pos + 16..pos + 24].try_into().unwrap());
            let crc32 = u32::from_le_bytes(data[pos + 24..pos + 28].try_into().unwrap());
            pos += 28;

            entries.push(ArchiveEntry {
                path,
                offset,
                compressed_size,
                original_size,
                crc32,
            });
        }

        Ok(Self {
            magic,
            version,
            compression,
            entry_count,
            entries,
        })
    }
}

/// 归档尾部（固定 16 字节，位于文件末尾）
#[derive(Debug, Clone)]
pub struct ArchiveFooter {
    /// Header 起始偏移
    pub header_offset: u64,
    /// 魔数 "WINDEND\0"
    pub magic: [u8; 8],
}

impl ArchiveFooter {
    pub fn new(header_offset: u64) -> Self {
        Self {
            header_offset,
            magic: *MAGIC_FOOTER,
        }
    }

    /// 序列化为 16 字节
    pub fn to_bytes(&self) -> [u8; 16] {
        let mut buf = [0u8; 16];
        buf[0..8].copy_from_slice(&self.header_offset.to_le_bytes());
        buf[8..16].copy_from_slice(&self.magic);
        buf
    }

    /// 从 16 字节反序列化
    pub fn from_bytes(data: &[u8; 16]) -> Result<Self, String> {
        let header_offset = u64::from_le_bytes(data[0..8].try_into().unwrap());
        let mut magic = [0u8; 8];
        magic.copy_from_slice(&data[8..16]);

        if &magic != MAGIC_FOOTER {
            return Err("Invalid footer magic".into());
        }

        Ok(Self {
            header_offset,
            magic,
        })
    }
}
