use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use moodflow_installer::archive::{
    integrity::{declared_size, validate_path, verify_manifest},
    ArchiveEntry, ArchiveFooter, ArchiveHeader, ArchiveReader, CompressionType,
};
use sha2::{Digest, Sha256};

fn inventory(files: &[(&str, &[u8])]) -> (Vec<ArchiveEntry>, Vec<u8>) {
    let mut data = Vec::new();
    let entries = files
        .iter()
        .map(|(path, bytes)| {
            let e = ArchiveEntry {
                path: path.to_string(),
                offset: data.len() as u64,
                compressed_size: 0,
                original_size: bytes.len() as u64,
                crc32: crc32fast::hash(bytes),
            };
            data.extend_from_slice(bytes);
            e
        })
        .collect();
    (entries, data)
}

fn manifest(content: &[u8]) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "version": 1, "files": { "词典.txt": {
            "size": content.len(), "sha256": format!("{:x}", Sha256::digest(content))
        }}
    }))
    .unwrap()
}

#[test]
fn sha_inventory_accepts_unicode_and_rejects_same_size_corruption() {
    let m = manifest(b"hello");
    let (entries, data) = inventory(&[
        ("data/manifest.json", &m),
        ("data/词典.txt", b"hello"),
        ("app.exe", b"stub"),
    ]);
    verify_manifest("data/manifest.json", &entries, &data).unwrap();
    let (entries, data) = inventory(&[("data/manifest.json", &m), ("data/词典.txt", b"jello")]);
    assert!(verify_manifest("data/manifest.json", &entries, &data)
        .unwrap_err()
        .contains("SHA-256"));
}

#[test]
fn sha_inventory_rejects_missing_extra_and_wrong_version() {
    let m = manifest(b"hello");
    for files in [
        vec![("data/manifest.json", m.as_slice())],
        vec![
            ("data/manifest.json", m.as_slice()),
            ("data/词典.txt", b"hello"),
            ("data/extra.txt", b"extra"),
        ],
    ] {
        let (entries, data) = inventory(&files);
        assert!(verify_manifest("data/manifest.json", &entries, &data).is_err());
    }
    let mut value: serde_json::Value = serde_json::from_slice(&m).unwrap();
    value["version"] = 2.into();
    let m = serde_json::to_vec(&value).unwrap();
    let (entries, data) = inventory(&[("data/manifest.json", &m), ("data/词典.txt", b"hello")]);
    assert!(verify_manifest("data/manifest.json", &entries, &data)
        .unwrap_err()
        .contains("version"));
    assert!(verify_manifest("missing.json", &entries, &data).is_err());
}

#[test]
fn duplicate_json_paths_are_rejected() {
    let m = br#"{"version":1,"files":{"a":{"size":0,"sha256":""},"a":{"size":0,"sha256":""}}}"#;
    let (entries, data) = inventory(&[("data/manifest.json", m)]);
    assert!(verify_manifest("data/manifest.json", &entries, &data)
        .unwrap_err()
        .contains("Duplicate"));
}

#[test]
fn windows_paths_cannot_escape_or_alias() {
    for path in [
        "",
        "../a",
        "/a",
        "a/../b",
        "C:/a",
        "a/C:b",
        "a\\b",
        "a//b",
        "a/.",
        "a. ",
        "a:",
        "nul.txt",
        "COM1",
        "com¹.txt",
        "a\0b",
        "a\nb",
        "a?b",
    ] {
        assert!(validate_path(path).is_err(), "{path:?}");
    }
    validate_path("data/中文/words.txt").unwrap();
    for files in [
        vec![("a", b"".as_slice()), ("A", b"")],
        vec![("a", b"".as_slice()), ("a/b", b"")],
    ] {
        assert!(declared_size(&inventory(&files).0).is_err());
    }
}

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static SEQ: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "moodflow-preflight-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&dir).unwrap();
        Self(dir)
    }
    fn archive(&self, entries: Vec<ArchiveEntry>, data: &[u8]) -> PathBuf {
        let block = zstd::encode_all(data, 1).unwrap();
        let mut header = ArchiveHeader::new(CompressionType::Zstd);
        header.entry_count = entries.len() as u32;
        header.entries = entries;
        header.solid_compressed_size = block.len() as u64;
        let mut bytes = block.clone();
        bytes.extend(header.to_bytes());
        bytes.extend(ArchiveFooter::new(block.len() as u64).to_bytes());
        let path = self.0.join("payload.bin");
        std::fs::write(&path, bytes).unwrap();
        path
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn corrupt_late_entry_prevents_every_write() {
    let scratch = Scratch::new();
    let (mut entries, data) = inventory(&[("first.txt", b"good"), ("last.txt", b"broken")]);
    entries[1].crc32 ^= 1;
    let path = scratch.archive(entries, &data);
    let output = scratch.0.join("installed");
    assert!(ArchiveReader::open(&path)
        .unwrap()
        .extract_all(&output)
        .unwrap_err()
        .contains("CRC32"));
    assert!(!output.exists());
}

#[test]
fn invalid_offsets_and_decompression_expansion_are_bounded() {
    let scratch = Scratch::new();
    let (mut entries, data) = inventory(&[("a", b"abcd")]);
    entries[0].offset = u64::MAX;
    assert!(ArchiveReader::open(&scratch.archive(entries, &data))
        .unwrap()
        .prepare()
        .is_err());
    let (mut entries, data) = inventory(&[("a", b"abcd")]);
    entries[0].original_size = 1;
    assert!(ArchiveReader::open(&scratch.archive(entries, &data))
        .unwrap()
        .prepare()
        .unwrap_err()
        .contains("size"));
}

#[test]
fn malformed_entry_count_does_not_allocate_unbounded_memory() {
    let mut header = ArchiveHeader::new(CompressionType::Zstd);
    header.entry_count = u32::MAX;
    assert!(ArchiveHeader::from_bytes(&header.to_bytes()).is_err());
}
