//! Recursive `.gguf` discovery for the model library.

use std::path::Path;

use shared::ModelEntry;

const MAX_DEPTH: usize = 8;
const MAX_ENTRIES: usize = 500;

/// Scans `dir` for `.gguf` files. Missing folders yield an empty list.
pub fn scan(dir: &Path) -> Vec<ModelEntry> {
    let mut entries = Vec::new();
    walk(dir, 0, &mut entries);
    entries.sort_by_key(|entry| entry.name.to_lowercase());
    entries.truncate(MAX_ENTRIES);
    entries
}

fn walk(dir: &Path, depth: usize, entries: &mut Vec<ModelEntry>) {
    if depth > MAX_DEPTH || entries.len() >= MAX_ENTRIES {
        return;
    }

    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in read_dir.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };

        if file_type.is_dir() {
            // Hugging Face keeps the real weights in `blobs/` (hash names, no
            // `.gguf` extension); walking it wastes time on multi-GB files.
            let skip = path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| matches!(name, "blobs" | ".locks" | ".no_exist"));
            if !skip {
                walk(&path, depth + 1, entries);
            }
            continue;
        }

        // Follow file symlinks: Hugging Face caches store snapshots as links
        // to blobs, and refusing them would hide every downloaded model.
        if !path.is_file() {
            continue;
        }

        let is_gguf = path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("gguf"));
        if !is_gguf {
            continue;
        }

        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("modelo")
            .to_owned();

        // Split GGUF (`-00002-of-00002.gguf`) only loads through its first
        // shard; listing the rest would offer models that always fail.
        if split_index(&name).is_some_and(|index| index > 1) {
            continue;
        }
        let size_mb = entry
            .metadata()
            .map(|metadata| metadata.len() / (1024 * 1024))
            .unwrap_or(0);

        entries.push(ModelEntry {
            path: path.display().to_string(),
            name,
            size_mb,
        });
    }
}

fn split_index(name: &str) -> Option<u32> {
    let (prefix, _) = name.rsplit_once("-of-")?;
    let (_, index) = prefix.rsplit_once('-')?;
    index.parse().ok()
}
