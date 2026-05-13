//! `regenerate_embeddings` tool — backfill or rebuild embeddings.

use std::sync::Arc;

use serde::Deserialize;
use serde_json::Value;

pub fn regenerate_embeddings_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "force": {
                "type": "boolean",
                "description": "If true, regenerate embeddings for ALL memories (overwrites existing). If false (default), only backfill memories where has_embedding = 0 or NULL.",
                "default": false
            },
            "node_ids": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Optional list of memory IDs to regenerate. If omitted, applies to all candidates per `force` flag."
            }
        }
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegenerateEmbeddingsArgs {
    force: Option<bool>,
    #[serde(alias = "node_ids")]
    node_ids: Option<Vec<String>>,
}

/// Regenerate embeddings tool.
///
/// Wraps `Storage::generate_embeddings` to expose uncapped backfill via MCP.
/// `consolidate` runs `generate_missing_embeddings` with a per-call cap (1000),
/// which is fine for incremental maintenance but slow for one-shot recovery.
pub async fn execute_regenerate_embeddings(
    storage: &Arc<vestige_core::Storage>,
    args: Option<Value>,
) -> Result<Value, String> {
    #[cfg(all(feature = "embeddings", feature = "vector-search"))]
    {
        let args: RegenerateEmbeddingsArgs = match args {
            Some(v) => {
                serde_json::from_value(v).map_err(|e| format!("Invalid arguments: {}", e))?
            }
            None => RegenerateEmbeddingsArgs {
                force: None,
                node_ids: None,
            },
        };

        let force = args.force.unwrap_or(false);
        let ids: Option<Vec<String>> = args.node_ids;

        let start = std::time::Instant::now();

        let storage_clone = storage.clone();
        let result = tokio::task::spawn_blocking(move || {
            storage_clone.generate_embeddings(ids.as_deref(), force)
        })
        .await
        .map_err(|e| format!("Embedding regeneration task panicked: {}", e))?
        .map_err(|e| format!("Embedding regeneration failed: {}", e))?;

        Ok(serde_json::json!({
            "tool": "regenerate_embeddings",
            "force": force,
            "successful": result.successful,
            "failed": result.failed,
            "skipped": result.skipped,
            "errors": result.errors,
            "durationMs": start.elapsed().as_millis() as u64,
            "message": format!(
                "Regenerated {} embeddings ({} skipped, {} failed) in {}ms",
                result.successful,
                result.skipped,
                result.failed,
                start.elapsed().as_millis()
            ),
        }))
    }

    #[cfg(not(all(feature = "embeddings", feature = "vector-search")))]
    {
        let _ = (storage, args);
        Err(
            "regenerate_embeddings requires the 'embeddings' and 'vector-search' features."
                .to_string(),
        )
    }
}
