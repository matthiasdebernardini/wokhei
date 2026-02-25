use agcli::CommandError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Keys not found at {path}")]
    KeysNotFound { path: String },

    #[error("Relay unreachable: {url}")]
    RelayUnreachable { url: String },

    #[error("Relay rejected event: {reason}")]
    RelayRejected { reason: String },

    #[error("Header not found: {event_id}")]
    HeaderNotFound { event_id: String },

    #[error("Header missing d-tag (required for addressable events)")]
    HeaderMissingDTag,

    #[error("Invalid event ID: {id}")]
    InvalidEventId { id: String },

    #[error("No results for query")]
    NoResults,

    #[error("Invalid nsec format")]
    InvalidNsec,

    #[error("Failed to save keys: {reason}")]
    KeysSaveFailed { reason: String },

    #[error("Keys already exist at {path} — use --force to overwrite")]
    KeysAlreadyExist { path: String },

    #[error("Invalid coordinate format: {input} — expected kind:pubkey:d-tag")]
    InvalidCoordinate { input: String },

    #[error("IO error: {reason}")]
    Io { reason: String },

    #[error("Invalid JSON: {reason}")]
    InvalidJson { reason: String },
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::KeysNotFound { .. } => "KEYS_NOT_FOUND",
            Self::RelayUnreachable { .. } => "RELAY_UNREACHABLE",
            Self::RelayRejected { .. } => "RELAY_REJECTED",
            Self::HeaderNotFound { .. } => "HEADER_NOT_FOUND",
            Self::HeaderMissingDTag => "HEADER_MISSING_D_TAG",
            Self::InvalidEventId { .. } => "INVALID_EVENT_ID",
            Self::NoResults => "NO_RESULTS",
            Self::InvalidNsec => "INVALID_NSEC",
            Self::KeysSaveFailed { .. } => "KEYS_SAVE_FAILED",
            Self::KeysAlreadyExist { .. } => "KEYS_ALREADY_EXIST",
            Self::InvalidCoordinate { .. } => "INVALID_COORDINATE",
            Self::Io { .. } => "IO_ERROR",
            Self::InvalidJson { .. } => "INVALID_JSON",
        }
    }

    pub fn retryable(&self) -> bool {
        matches!(self, Self::RelayUnreachable { .. })
    }

    pub fn fix(&self) -> String {
        match self {
            Self::KeysNotFound { .. } => {
                "Run `wokhei init --generate` to create a new keypair".to_string()
            }
            Self::RelayUnreachable { url } => {
                format!("Check that the relay at {url} is running. For local dev: `cd strfry && docker compose up -d`")
            }
            Self::RelayRejected { .. } => {
                "Check event format and relay write policy".to_string()
            }
            Self::HeaderNotFound { .. } => {
                "Verify the event ID, or use `--header-coordinate` for cross-relay references"
                    .to_string()
            }
            Self::HeaderMissingDTag => {
                "The header event is malformed (no d-tag). Create a new addressable header with `--d-tag`".to_string()
            }
            Self::InvalidEventId { .. } => {
                "Use a hex event ID from a previous command's result".to_string()
            }
            Self::NoResults => {
                "Try different filters, or check that the relay has data".to_string()
            }
            Self::InvalidNsec => "Use bech32 nsec1... format".to_string(),
            Self::KeysSaveFailed { .. } => {
                "Check file permissions on ~/.wokhei/".to_string()
            }
            Self::KeysAlreadyExist { path } => {
                format!("Keys already exist at {path}. Back up and remove to regenerate.")
            }
            Self::InvalidCoordinate { .. } => {
                "Format: kind:pubkey:d-tag (e.g., 39998:abc123:my-list)".to_string()
            }
            Self::Io { .. } => "Check file permissions and disk space".to_string(),
            Self::InvalidJson { .. } => {
                "Provide valid JSON input".to_string()
            }
        }
    }
}

impl From<AppError> for CommandError {
    fn from(err: AppError) -> Self {
        CommandError::new(err.to_string(), err.code(), err.fix())
    }
}
