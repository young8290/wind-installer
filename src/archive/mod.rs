pub mod format;
pub mod reader;
pub mod writer;

#[allow(unused_imports)]
pub use format::{ArchiveEntry, ArchiveFooter, ArchiveHeader, CompressionType, FORMAT_VERSION, MAGIC_FOOTER, MAGIC_HEADER};
pub use reader::ArchiveReader;
pub use writer::ArchiveWriter;

use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;

/// 将 stub EXE 和归档 .bin 捆绑为最终安装程序。
///
/// Solid 格式下 entry.offset 是解压后流中的偏移，与 stub 大小无关，
/// 因此 Header 字节可原样复制，只需更新 Footer 中的 header_offset。
///
/// .bin 布局：[compressed_block: N bytes][header_bytes: H bytes][footer: 16 bytes]
///   footer.header_offset = N
///
/// 最终 .exe 布局：[stub: S bytes][compressed_block: N bytes][header_bytes: H bytes][footer: 16 bytes]
///   footer.header_offset = S + N
#[allow(dead_code)]
pub fn bundle_exe(stub_path: &Path, archive_path: &Path, output_path: &Path) -> Result<(), String> {
    let stub_size = std::fs::metadata(stub_path)
        .map_err(|e| format!("Failed to stat stub: {}", e))?.len();

    let arc_reader = ArchiveReader::open(archive_path)?;
    let solid_size = arc_reader.header().solid_compressed_size;

    let archive_size = std::fs::metadata(archive_path)
        .map_err(|e| format!("Failed to stat archive: {}", e))?.len();

    // header_bytes 在 .bin 中占的字节数（compressed_block 与 footer 之间）
    let header_bytes_size = archive_size - solid_size - 16;

    let mut out = BufWriter::new(
        std::fs::File::create(output_path)
            .map_err(|e| format!("Failed to create output: {}", e))?
    );

    let mut arc_src = BufReader::new(
        std::fs::File::open(archive_path)
            .map_err(|e| format!("Failed to open archive: {}", e))?
    );

    // 1. stub
    let mut stub_src = BufReader::new(
        std::fs::File::open(stub_path)
            .map_err(|e| format!("Failed to open stub: {}", e))?
    );
    std::io::copy(&mut stub_src, &mut out)
        .map_err(|e| format!("Failed to write stub: {}", e))?;

    // 2. 压缩块（原样复制，无需任何修改）
    let mut block_src = (&mut arc_src).take(solid_size);
    std::io::copy(&mut block_src, &mut out)
        .map_err(|e| format!("Failed to write compressed block: {}", e))?;

    // 3. Header 字节（原样复制，entry.offset 不依赖 stub 大小）
    let mut header_src = (&mut arc_src).take(header_bytes_size);
    std::io::copy(&mut header_src, &mut out)
        .map_err(|e| format!("Failed to write header: {}", e))?;

    // 4. 更新后的 Footer（只有 header_offset 变化：加上 stub_size）
    let new_footer = ArchiveFooter::new(stub_size + solid_size);
    out.write_all(&new_footer.to_bytes())
        .map_err(|e| format!("Failed to write footer: {}", e))?;
    out.flush().ok();

    Ok(())
}
