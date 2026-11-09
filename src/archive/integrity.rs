//! Preflight checks run entirely in memory, before touching the installation.
use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::format::ArchiveEntry;

pub const MAX_UNCOMPRESSED_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub const MAX_HEADER_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_ENTRIES: usize = 100_000;

/// Validate using Windows rules even when packing on another operating system.
pub fn validate_path(path: &str) -> Result<(), String> {
    if path.is_empty()
        || path.contains('\\')
        || path.encode_utf16().count() > 32_000
        || path.len() > u16::MAX as usize
    {
        return Err(format!("Unsafe archive path: {path:?}"));
    }
    for part in path.split('/') {
        let stem = part.split('.').next().unwrap_or("").to_uppercase();
        let device = matches!(
            stem.as_str(),
            "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
        ) || ["COM", "LPT"].iter().any(|prefix| {
            stem.strip_prefix(prefix).is_some_and(|n| {
                matches!(
                    n,
                    "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )
            })
        });
        if part.is_empty()
            || part == "."
            || part == ".."
            || part.ends_with(['.', ' '])
            || part
                .chars()
                .any(|c| c.is_control() || "<>:\"|?*".contains(c))
            || device
        {
            return Err(format!("Unsafe archive path: {path:?}"));
        }
    }
    Ok(())
}

pub fn declared_size(entries: &[ArchiveEntry]) -> Result<u64, String> {
    if entries.len() > MAX_ENTRIES {
        return Err("Too many archive entries".into());
    }
    let mut size = 0u64;
    let mut names = BTreeSet::new();
    for entry in entries {
        validate_path(&entry.path)?;
        if !names.insert(entry.path.to_lowercase()) {
            return Err(format!("Duplicate archive path: {}", entry.path));
        }
        // Writer uses one contiguous solid stream. Reject gaps, overlaps and overflow.
        if entry.offset != size {
            return Err(format!("Invalid archive offset: {}", entry.path));
        }
        size = size
            .checked_add(entry.original_size)
            .filter(|n| *n <= MAX_UNCOMPRESSED_BYTES)
            .ok_or("Archive exceeds the 2 GiB uncompressed limit")?;
    }
    for name in &names {
        for (i, _) in name.match_indices('/') {
            if names.contains(&name[..i]) {
                return Err(format!("File/directory collision: {name}"));
            }
        }
    }
    Ok(size)
}

pub fn entry_bytes<'a>(entry: &ArchiveEntry, data: &'a [u8]) -> Result<&'a [u8], String> {
    let start = usize::try_from(entry.offset).map_err(|_| "Archive offset overflow")?;
    let len = usize::try_from(entry.original_size).map_err(|_| "Archive size overflow")?;
    let end = start.checked_add(len).ok_or("Archive range overflow")?;
    let bytes = data
        .get(start..end)
        .ok_or_else(|| format!("Entry out of bounds: {}", entry.path))?;
    if crc32fast::hash(bytes) != entry.crc32 {
        return Err(format!("CRC32 mismatch: {}", entry.path));
    }
    Ok(bytes)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DataManifest {
    version: u32,
    #[serde(deserialize_with = "unique_files")]
    files: BTreeMap<String, FileDigest>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileDigest {
    sha256: String,
    size: u64,
}

fn unique_files<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<String, FileDigest>, D::Error> {
    struct Visitor;
    impl<'de> serde::de::Visitor<'de> for Visitor {
        type Value = BTreeMap<String, FileDigest>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("an object with unique file paths")
        }
        fn visit_map<M: serde::de::MapAccess<'de>>(
            self,
            mut map: M,
        ) -> Result<Self::Value, M::Error> {
            let mut files = BTreeMap::new();
            while let Some((key, value)) = map.next_entry::<String, FileDigest>()? {
                if files.insert(key, value).is_some() {
                    return Err(serde::de::Error::custom("Duplicate manifest path"));
                }
            }
            Ok(files)
        }
    }
    deserializer.deserialize_map(Visitor)
}

pub fn verify_manifest(path: &str, entries: &[ArchiveEntry], data: &[u8]) -> Result<(), String> {
    validate_path(path)?;
    let entry = entries
        .iter()
        .find(|e| e.path == path)
        .ok_or_else(|| format!("Required data manifest missing: {path}"))?;
    if entry.original_size > 16 * 1024 * 1024 {
        return Err("Data manifest exceeds 16 MiB".into());
    }
    let manifest: DataManifest = serde_json::from_slice(entry_bytes(entry, data)?)
        .map_err(|e| format!("Invalid data manifest: {e}"))?;
    if manifest.version != 1 {
        return Err(format!(
            "Unsupported data manifest version: {}",
            manifest.version
        ));
    }
    let prefix = path
        .rsplit_once('/')
        .map(|(dir, _)| format!("{dir}/"))
        .unwrap_or_default();
    let files: BTreeMap<_, _> = entries
        .iter()
        .filter(|e| e.path != path)
        .filter_map(|e| e.path.strip_prefix(&prefix).map(|rel| (rel, e)))
        .collect();
    if files.len() != manifest.files.len() {
        return Err("Data inventory contains missing or extra files".into());
    }
    for (rel, digest) in manifest.files {
        validate_path(&rel)?;
        let entry = files
            .get(rel.as_str())
            .ok_or_else(|| format!("Missing data file: {rel}"))?;
        let bytes = entry_bytes(entry, data)?;
        if digest.size != entry.original_size
            || digest.sha256.len() != 64
            || !digest.sha256.bytes().all(|b| b.is_ascii_hexdigit())
            || !format!("{:x}", Sha256::digest(bytes)).eq_ignore_ascii_case(&digest.sha256)
        {
            return Err(format!("SHA-256/size mismatch: {rel}"));
        }
    }
    Ok(())
}
