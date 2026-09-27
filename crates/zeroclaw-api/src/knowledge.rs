//! Versioned Knowledge representation and provenance models.

use serde::{Deserialize, Serialize};

/// Identifies the original source of knowledge (e.g. file, URL, user input, external integration).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KnowledgeSource {
    pub id: String,
    pub title: String,
    pub uri: Option<String>,
    pub metadata: std::collections::HashMap<String, String>,
}

/// A parsed document or logical unit extracted from a `KnowledgeSource`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeDocument {
    pub id: String,
    pub source_id: String,
    pub format: String, // "markdown", "pdf-text", "code"
    pub content: String,
}

/// Tracks the versioning and history of a specific document or extraction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeVersion {
    pub version_id: String,
    pub document_id: String,
    pub timestamp: String,
    pub author_or_extractor: String,
    pub content_hash: String,
}

/// A smaller embeddable or retrieval-friendly chunk of a document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeChunk {
    pub id: String,
    pub document_id: String,
    pub version_id: String,
    pub chunk_index: usize,
    pub text: String,
    pub embedding_id: Option<String>,
}

/// Provenance metadata indicating how a chunk or lesson was derived.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeProvenance {
    pub derived_from_chunk_ids: Vec<String>,
    pub extractor_model: Option<String>,
    pub confidence_score: f64,
}

/// A synthetic, generalized learning or policy derived from raw knowledge chunks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lesson {
    pub id: String,
    pub summary: String,
    pub category: String,
    pub scope: crate::memory_traits::MemoryScope,
    pub provenance: KnowledgeProvenance,
    pub owner_agent_id: Option<String>,
}

/// Lifecycle status for a document or folder indexing operation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum IndexStatus {
    Pending,
    Indexing,
    Complete,
    Failed(String),
}

/// Tracks the lifecycle of document/folder indexing and re-indexing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexJob {
    pub id: String,
    pub source_id: String,
    pub status: IndexStatus,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

/// Policies for TTL, retention, archival, and cascading deletions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionPolicy {
    pub ttl_seconds: Option<u64>,
    pub archive_after_seconds: Option<u64>,
    pub cascade_delete: bool,
}

/// Controls for hybrid FTS/vector/metadata search with source-awareness.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchControls {
    pub query: String,
    pub vector: Option<Vec<f32>>,
    pub metadata_filters: std::collections::HashMap<String, String>,
    pub fts_weight: f32,
    pub vector_weight: f32,
    pub source_aware: bool,
}

/// Result from a hybrid search, including source provenance if requested.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub chunk: KnowledgeChunk,
    pub score: f32,
    pub source: Option<KnowledgeSource>,
}

/// Audit log entry for edits, forgets, and delete-by-source corrections.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrectionAuditLog {
    pub id: String,
    pub target_id: String,
    pub action: String, // "edit", "forget", "delete"
    pub original_content: Option<String>,
    pub new_content: Option<String>,
    pub timestamp: String,
    pub author: String,
    pub reason: String,
}

/// API surface for dashboard inspection and correction controls.
#[async_trait::async_trait]
pub trait DashboardKnowledgeApi {
    async fn inspect_source(&self, source_id: &str) -> anyhow::Result<KnowledgeSource>;
    async fn trigger_reindex(&self, source_id: &str) -> anyhow::Result<IndexJob>;
    async fn edit_chunk(
        &self,
        chunk_id: &str,
        new_content: &str,
        reason: &str,
    ) -> anyhow::Result<CorrectionAuditLog>;
    async fn forget_document(
        &self,
        document_id: &str,
        reason: &str,
    ) -> anyhow::Result<CorrectionAuditLog>;
    async fn delete_by_source(
        &self,
        source_id: &str,
        cascade: bool,
    ) -> anyhow::Result<CorrectionAuditLog>;
}
