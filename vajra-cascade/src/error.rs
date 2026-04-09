use thiserror::Error;
#[derive(Debug, Error)]
pub enum CascadeError {
    #[error("expected a JSON array of records, got {kind}")]
    NotAnArray { kind: String },
    #[error("failed to read input: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse JSON: {0}")]
    Json(#[from] serde_json::Error),
}
