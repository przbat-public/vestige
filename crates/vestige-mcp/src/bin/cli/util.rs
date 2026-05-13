//! Shared CLI helpers — default DB path, paginated fetch, UTF-8 safe truncation.

use std::path::PathBuf;

use directories::ProjectDirs;
use vestige_core::Storage;

pub(super) fn get_default_db_path() -> anyhow::Result<PathBuf> {
    let proj_dirs = ProjectDirs::from("com", "vestige", "core")
        .ok_or_else(|| anyhow::anyhow!("Could not determine project directories"))?;
    Ok(proj_dirs.data_dir().join("vestige.db"))
}

pub(super) fn fetch_all_nodes(storage: &Storage) -> anyhow::Result<Vec<vestige_core::KnowledgeNode>> {
    let mut all_nodes = Vec::new();
    let page_size = 500;
    let mut offset = 0;

    loop {
        let batch = storage.get_all_nodes(page_size, offset)?;
        let batch_len = batch.len();
        all_nodes.extend(batch);
        if batch_len < page_size as usize {
            break;
        }
        offset += page_size;
    }

    Ok(all_nodes)
}

/// Truncate a string for display (UTF-8 safe)
pub(super) fn truncate(s: &str, max_chars: usize) -> String {
    let s = s.replace('\n', " ");
    if s.chars().count() <= max_chars {
        s
    } else {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{}...", truncated)
    }
}
