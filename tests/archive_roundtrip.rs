//! 归档读写集成测试
//!
//! 关键回归测试：`*_full_content_not_truncated` 系列
//! 复现修复前的 Bug：`Read::take(decoder, compressed_size)` 用压缩大小截断解压输出，
//! 导致任何能被压缩的文件解压后都是残缺的，CRC 校验必然失败。

use std::path::{Path, PathBuf};
use wind_installer::archive::{self, ArchiveReader, ArchiveWriter, CompressionType};

// ── 测试辅助 ─────────────────────────────────────────────────────────────────

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(suffix: &str) -> Self {
        let path = std::env::temp_dir()
            .join(format!("wind_test_{}_{}", std::process::id(), suffix));
        std::fs::create_dir_all(&path).expect("创建临时目录失败");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn write_file(&self, name: &str, content: &[u8]) -> PathBuf {
        let path = self.path.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("创建父目录失败");
        }
        std::fs::write(&path, content).expect("写入测试文件失败");
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn pack(dir: &TempDir, archive_name: &str, files: &[(&str, &[u8])], compression: CompressionType) -> PathBuf {
    let archive_path = dir.path().join(archive_name);
    let mut writer = ArchiveWriter::new(&archive_path, compression).expect("创建 ArchiveWriter 失败");
    for (name, content) in files {
        let src = dir.write_file(name, content);
        writer.add_file(&src, name).expect("add_file 失败");
    }
    writer.finish().expect("finish 失败");
    archive_path
}

fn extract_all_to(archive_path: &Path, out_dir: &Path) {
    std::fs::create_dir_all(out_dir).unwrap();
    let mut reader = ArchiveReader::open(archive_path).expect("打开归档失败");
    let entries = reader.entries().to_vec();
    for entry in &entries {
        let dest = out_dir.join(&entry.path);
        reader.extract_entry(entry, &dest).expect("extract_entry 失败");
    }
}

// ── 核心回归测试：捕获 Read::take 截断 Bug ───────────────────────────────────

/// Zstd 关键回归测试。
///
/// 10KB 重复字节 → zstd 压缩后约 20-30 字节（compressed_size ≈ 25）。
/// 旧 Bug：`take(decoder, 25)` 只允许解压器输出 25 字节，文件被截断。
/// 正确行为：`take` 限制压缩输入消耗，解压器输出完整 10240 字节。
#[test]
fn zstd_decompresses_full_content_when_compression_ratio_is_high() {
    let dir = TempDir::new("zstd_full");
    let original = vec![0xABu8; 10 * 1024];

    let archive = pack(&dir, "test.pkg", &[("data.bin", &original)], CompressionType::Zstd);
    let out_dir = dir.path().join("out");
    extract_all_to(&archive, &out_dir);

    let extracted = std::fs::read(out_dir.join("data.bin")).expect("读取解压文件失败");
    assert_eq!(
        extracted.len(), original.len(),
        "Zstd 解压后大小 {} 与原始 {} 不符（可能被截断为 compressed_size）",
        extracted.len(), original.len()
    );
    assert_eq!(extracted, original, "Zstd 解压内容应与原始完全一致");
}

/// LZMA 关键回归测试（与 Zstd 相同场景）。
#[test]
fn lzma_decompresses_full_content_when_compression_ratio_is_high() {
    let dir = TempDir::new("lzma_full");
    let original = vec![0xCDu8; 10 * 1024];

    let archive = pack(&dir, "test.pkg", &[("data.bin", &original)], CompressionType::Lzma);
    let out_dir = dir.path().join("out");
    extract_all_to(&archive, &out_dir);

    let extracted = std::fs::read(out_dir.join("data.bin")).expect("读取解压文件失败");
    assert_eq!(
        extracted.len(), original.len(),
        "LZMA 解压后大小 {} 与原始 {} 不符", extracted.len(), original.len()
    );
    assert_eq!(extracted, original, "LZMA 解压内容应与原始完全一致");
}

// ── 多文件测试 ────────────────────────────────────────────────────────────────

/// 多个文件全部正确解压，内容互不干扰。
#[test]
fn multiple_entries_all_extracted_correctly() {
    let dir = TempDir::new("multi");
    let files: &[(&str, &[u8])] = &[
        ("a.txt", b"hello world"),
        ("sub/b.bin", &[0u8; 4096]), // 4KB 零字节，高压缩率
        ("c.dat", b"the quick brown fox jumps over the lazy dog"),
    ];

    let archive = pack(&dir, "multi.pkg", files, CompressionType::Zstd);
    let out_dir = dir.path().join("out");
    extract_all_to(&archive, &out_dir);

    for (name, content) in files {
        let extracted = std::fs::read(out_dir.join(name))
            .unwrap_or_else(|_| panic!("读取 {} 失败", name));
        assert_eq!(&extracted, content, "文件 {} 内容不一致", name);
    }
}

/// 验证顺序提取时文件指针在每个条目后正确推进。
/// 三个大小各异的文件，任何一个错位都会导致内容错乱。
#[test]
fn sequential_entries_file_pointer_advances_correctly() {
    let dir = TempDir::new("seq");
    let file_a = vec![0x01u8; 5000];
    let file_b = vec![0x02u8; 2000];
    let file_c = vec![0x03u8; 8000];

    let archive = pack(
        &dir, "seq.pkg",
        &[("a.bin", &file_a), ("b.bin", &file_b), ("c.bin", &file_c)],
        CompressionType::Zstd,
    );
    let out_dir = dir.path().join("out");
    extract_all_to(&archive, &out_dir);

    assert_eq!(std::fs::read(out_dir.join("a.bin")).unwrap(), file_a, "a.bin 内容错误");
    assert_eq!(std::fs::read(out_dir.join("b.bin")).unwrap(), file_b, "b.bin 内容错误");
    assert_eq!(std::fs::read(out_dir.join("c.bin")).unwrap(), file_c, "c.bin 内容错误");
}

// ── 边界情况 ──────────────────────────────────────────────────────────────────

/// 所有字节值 0x00-0xFF 都被精确保留（验证二进制安全）。
#[test]
fn binary_content_all_byte_values_preserved() {
    let dir = TempDir::new("binary");
    let original: Vec<u8> = (0u8..=255).cycle().take(2048).collect();

    let archive = pack(&dir, "bin.pkg", &[("all_bytes.bin", &original)], CompressionType::Zstd);
    let out_dir = dir.path().join("out");
    extract_all_to(&archive, &out_dir);

    let extracted = std::fs::read(out_dir.join("all_bytes.bin")).unwrap();
    assert_eq!(extracted, original, "二进制内容应逐字节一致");
}

/// 空文件打包后可正常解压，结果仍为空。
#[test]
fn empty_file_roundtrip() {
    let dir = TempDir::new("empty");

    let archive = pack(&dir, "empty.pkg", &[("empty.bin", b"")], CompressionType::Zstd);
    let out_dir = dir.path().join("out");
    extract_all_to(&archive, &out_dir);

    let extracted = std::fs::read(out_dir.join("empty.bin")).unwrap();
    assert!(extracted.is_empty(), "空文件解压后应为空");
}

// ── CRC 校验 ──────────────────────────────────────────────────────────────────

/// 篡改归档压缩数据后，extract_entry 应返回错误（解压失败或 CRC 不匹配）。
#[test]
fn crc_mismatch_or_decompress_error_when_archive_corrupted() {
    let dir = TempDir::new("crc");
    let original = vec![0xAAu8; 1024];

    let archive_path = pack(&dir, "crc.pkg", &[("data.bin", &original)], CompressionType::Zstd);

    // 篡改 offset=0 处的压缩块前 4 字节
    let mut raw = std::fs::read(&archive_path).unwrap();
    raw[0] ^= 0xFF;
    raw[1] ^= 0xFF;
    raw[2] ^= 0xFF;
    raw[3] ^= 0xFF;
    std::fs::write(&archive_path, &raw).unwrap();

    let out_dir = dir.path().join("out");
    std::fs::create_dir_all(&out_dir).unwrap();

    let mut reader = ArchiveReader::open(&archive_path).unwrap();
    let entries = reader.entries().to_vec();
    let result = reader.extract_entry(&entries[0], &out_dir.join("data.bin"));

    assert!(result.is_err(), "篡改归档后应返回错误，实际返回 Ok");
}

// ── Bundle 流程测试 ───────────────────────────────────────────────────────────

/// 核心回归测试：复现 dist/ 下安装包 "Invalid header magic" 的根本原因。
///
/// bundle_exe 直接拼接 stub+archive 时，archive 内所有偏移都是相对 .bin 起点的。
/// 若不修正偏移（各加 stub_size），读取器会寻址到 stub 内部并读出垃圾数据。
#[test]
fn bundle_exe_fixes_offsets_so_content_readable_after_prepending_stub() {
    let dir = TempDir::new("bundle");
    let content_a = vec![0xAAu8; 4096]; // 高压缩率：compressed_size << original_size
    let content_b = b"hello from file B".as_slice();

    // 1. Pack → .bin
    let archive = pack(
        &dir, "data.bin",
        &[("a.dat", &content_a), ("b.txt", content_b)],
        CompressionType::Zstd,
    );

    // 2. 创建一个假 stub（模拟 wind-installer.exe 的二进制头部）
    //    使用随机字节确保偏移计算不依赖内容
    let stub_content: Vec<u8> = (0u8..=255).cycle().take(8192).collect();
    let stub_path = dir.write_file("stub.exe", &stub_content);

    // 3. Bundle：stub + 修正偏移后的 archive
    let installer = dir.path().join("installer.exe");
    archive::bundle_exe(&stub_path, &archive, &installer)
        .expect("bundle_exe 应成功");

    // 4. 从合并后的 installer.exe 中读取归档
    let out_dir = dir.path().join("out");
    extract_all_to(&installer, &out_dir);

    // 5. 验证内容完整
    let extracted_a = std::fs::read(out_dir.join("a.dat")).expect("读取 a.dat 失败");
    let extracted_b = std::fs::read(out_dir.join("b.txt")).expect("读取 b.txt 失败");

    assert_eq!(extracted_a, content_a,
        "a.dat 内容不一致（高压缩率文件在偏移修正前会被截断）");
    assert_eq!(extracted_b, content_b,
        "b.txt 内容不一致");
}

/// 核心回归测试：清单 + logo 经 pack → bundle → read 全程保持完整。
///
/// 验证通用安装器的关键链路：打包时嵌入的 manifest/logo 字节，在追加 stub overlay
/// 后仍能被运行期从最终 exe 的头部正确读出（bundle_exe 原样复制头部字节）。
#[test]
fn manifest_and_logo_survive_pack_and_bundle() {
    let dir = TempDir::new("manifest_bundle");
    let manifest = b"[app]\nid = \"WindInput\"\ndisplay_name = \"\xe6\xb8\x85\xe9\xa3\x8e\"\n".to_vec();
    let logo = vec![0x89u8, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x11, 0x22]; // 伪 PNG

    // pack：写入归档并设置清单/logo
    let archive_path = dir.path().join("data.bin");
    let mut writer = ArchiveWriter::new(&archive_path, CompressionType::Lzma).unwrap();
    writer.set_manifest(manifest.clone(), logo.clone());
    let src = dir.write_file("payload.bin", &vec![0x5Au8; 3000]);
    writer.add_file(&src, "payload.bin").unwrap();
    writer.finish().unwrap();

    // 直接从 .bin 读回
    let reader = ArchiveReader::open(&archive_path).unwrap();
    assert_eq!(reader.manifest_bytes(), &manifest[..], ".bin 清单应一致");
    assert_eq!(reader.logo_bytes(), &logo[..], ".bin logo 应一致");

    // bundle：追加 stub overlay 后从最终 exe 读回
    let stub: Vec<u8> = (0u8..=255).cycle().take(6000).collect();
    let stub_path = dir.write_file("stub.exe", &stub);
    let installer = dir.path().join("setup.exe");
    archive::bundle_exe(&stub_path, &archive_path, &installer).unwrap();

    let reader2 = ArchiveReader::open(&installer).unwrap();
    assert_eq!(reader2.manifest_bytes(), &manifest[..], "bundle 后清单应一致");
    assert_eq!(reader2.logo_bytes(), &logo[..], "bundle 后 logo 应一致");
    // 数据仍可正确解压
    let out_dir = dir.path().join("out");
    extract_all_to(&installer, &out_dir);
    assert_eq!(std::fs::read(out_dir.join("payload.bin")).unwrap(), vec![0x5Au8; 3000]);
}

/// bundle_exe 对 stub_size=0（无前缀）应与直接读取 .bin 等价。
#[test]
fn bundle_exe_with_empty_stub_reads_identically_to_plain_archive() {
    let dir = TempDir::new("bundle_empty_stub");
    let content = b"stub size zero test content";

    let archive = pack(&dir, "data.bin", &[("f.txt", content)], CompressionType::Zstd);

    // 空 stub
    let stub_path = dir.write_file("empty.exe", b"");
    let bundled = dir.path().join("out.exe");
    archive::bundle_exe(&stub_path, &archive, &bundled).expect("bundle_exe 失败");

    // 从 bundled 读取
    let out_dir = dir.path().join("out");
    extract_all_to(&bundled, &out_dir);
    let extracted = std::fs::read(out_dir.join("f.txt")).unwrap();
    assert_eq!(&extracted, content);
}
