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

    #[error("Invalid public key: {pubkey}")]
    InvalidPubkey { pubkey: String },

    #[error("No results for query")]
    NoResults,

    #[error("Invalid nsec format")]
    InvalidNsec,

    #[error("Failed to save keys: {reason}")]
    KeysSaveFailed { reason: String },

    #[error("Keys already exist at {path}")]
    KeysAlreadyExist { path: String },

    #[error("Invalid coordinate format: {input} — expected kind:pubkey:d-tag")]
    InvalidCoordinate { input: String },

    #[error("IO error: {reason}")]
    Io { reason: String },

    #[error("Invalid JSON: {reason}")]
    InvalidJson { reason: String },

    #[error("Event not found: {event_id}")]
    EventNotFound { event_id: String },
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
            Self::InvalidPubkey { .. } => "INVALID_PUBKEY",
            Self::NoResults => "NO_RESULTS",
            Self::InvalidNsec => "INVALID_NSEC",
            Self::KeysSaveFailed { .. } => "KEYS_SAVE_FAILED",
            Self::KeysAlreadyExist { .. } => "KEYS_ALREADY_EXIST",
            Self::InvalidCoordinate { .. } => "INVALID_COORDINATE",
            Self::Io { .. } => "IO_ERROR",
            Self::InvalidJson { .. } => "INVALID_JSON",
            Self::EventNotFound { .. } => "EVENT_NOT_FOUND",
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
            Self::InvalidPubkey { .. } => {
                "Use a hex or bech32 (npub1...) public key".to_string()
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
            Self::EventNotFound { .. } => {
                "Verify the event ID, or use `wokhei list-headers` to find valid events"
                    .to_string()
            }
        }
    }
}

impl From<AppError> for CommandError {
    fn from(err: AppError) -> Self {
        CommandError::new(err.to_string(), err.code(), err.fix()).retryable(err.retryable())
    }
}

// Allow converting dcosl-core protocol errors into AppError
impl From<dcosl_core::DcoslError> for AppError {
    fn from(err: dcosl_core::DcoslError) -> Self {
        match err {
            dcosl_core::DcoslError::InvalidCoordinate { input } => {
                AppError::InvalidCoordinate { input }
            }
            dcosl_core::DcoslError::HeaderMissingDTag => AppError::HeaderMissingDTag,
            dcosl_core::DcoslError::InvalidEventId { id } => AppError::InvalidEventId { id },
            dcosl_core::DcoslError::InvalidPubkey { pubkey } => AppError::InvalidPubkey { pubkey },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_keys_not_found() {
        let e = AppError::KeysNotFound {
            path: "/tmp".into(),
        };
        assert_eq!(e.code(), "KEYS_NOT_FOUND");
    }

    #[test]
    fn code_relay_unreachable() {
        let e = AppError::RelayUnreachable {
            url: "ws://x".into(),
        };
        assert_eq!(e.code(), "RELAY_UNREACHABLE");
    }

    #[test]
    fn code_invalid_coordinate() {
        let e = AppError::InvalidCoordinate {
            input: "bad".into(),
        };
        assert_eq!(e.code(), "INVALID_COORDINATE");
    }

    #[test]
    fn relay_unreachable_is_retryable() {
        let e = AppError::RelayUnreachable {
            url: "ws://x".into(),
        };
        assert!(e.retryable());
    }

    #[test]
    fn non_relay_errors_are_not_retryable() {
        assert!(!AppError::KeysNotFound { path: "/x".into() }.retryable());
        assert!(!AppError::HeaderMissingDTag.retryable());
        assert!(!AppError::InvalidCoordinate { input: "x".into() }.retryable());
    }

    #[test]
    fn fix_strings_are_all_non_empty() {
        let variants: Vec<AppError> = vec![
            AppError::KeysNotFound { path: "p".into() },
            AppError::RelayUnreachable { url: "u".into() },
            AppError::RelayRejected { reason: "r".into() },
            AppError::HeaderNotFound {
                event_id: "e".into(),
            },
            AppError::HeaderMissingDTag,
            AppError::InvalidEventId { id: "i".into() },
            AppError::InvalidPubkey { pubkey: "p".into() },
            AppError::NoResults,
            AppError::InvalidNsec,
            AppError::KeysSaveFailed { reason: "r".into() },
            AppError::KeysAlreadyExist { path: "p".into() },
            AppError::InvalidCoordinate { input: "i".into() },
            AppError::Io { reason: "r".into() },
            AppError::InvalidJson { reason: "r".into() },
            AppError::EventNotFound {
                event_id: "e".into(),
            },
        ];
        for v in variants {
            assert!(!v.fix().is_empty(), "fix() empty for {}", v.code());
        }
    }

    #[test]
    fn command_error_from_app_error_preserves_code() {
        let app_err = AppError::InvalidNsec;
        let cmd_err = CommandError::from(app_err);
        assert_eq!(cmd_err.code, "INVALID_NSEC");
    }

    #[test]
    fn command_error_from_app_error_preserves_retryable() {
        let retryable_err = AppError::RelayUnreachable {
            url: "ws://x".into(),
        };
        let cmd_err = CommandError::from(retryable_err);
        assert!(cmd_err.retryable);

        let non_retryable = AppError::NoResults;
        let cmd_err2 = CommandError::from(non_retryable);
        assert!(!cmd_err2.retryable);
    }

    #[test]
    fn dcosl_error_converts_to_app_error() {
        let dcosl_err = dcosl_core::DcoslError::InvalidCoordinate {
            input: "bad".into(),
        };
        let app_err = AppError::from(dcosl_err);
        assert_eq!(app_err.code(), "INVALID_COORDINATE");
    }
}
