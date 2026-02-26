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
        CommandError::new(err.to_string(), err.code(), err.fix()).retryable(err.retryable())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // code() returns correct string for each variant
    // -----------------------------------------------------------------------

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
    fn code_relay_rejected() {
        let e = AppError::RelayRejected {
            reason: "nope".into(),
        };
        assert_eq!(e.code(), "RELAY_REJECTED");
    }

    #[test]
    fn code_header_not_found() {
        let e = AppError::HeaderNotFound {
            event_id: "abc".into(),
        };
        assert_eq!(e.code(), "HEADER_NOT_FOUND");
    }

    #[test]
    fn code_header_missing_d_tag() {
        assert_eq!(AppError::HeaderMissingDTag.code(), "HEADER_MISSING_D_TAG");
    }

    #[test]
    fn code_invalid_event_id() {
        let e = AppError::InvalidEventId { id: "bad".into() };
        assert_eq!(e.code(), "INVALID_EVENT_ID");
    }

    #[test]
    fn code_no_results() {
        assert_eq!(AppError::NoResults.code(), "NO_RESULTS");
    }

    #[test]
    fn code_invalid_nsec() {
        assert_eq!(AppError::InvalidNsec.code(), "INVALID_NSEC");
    }

    #[test]
    fn code_keys_save_failed() {
        let e = AppError::KeysSaveFailed {
            reason: "disk".into(),
        };
        assert_eq!(e.code(), "KEYS_SAVE_FAILED");
    }

    #[test]
    fn code_keys_already_exist() {
        let e = AppError::KeysAlreadyExist { path: "/x".into() };
        assert_eq!(e.code(), "KEYS_ALREADY_EXIST");
    }

    #[test]
    fn code_invalid_coordinate() {
        let e = AppError::InvalidCoordinate {
            input: "bad".into(),
        };
        assert_eq!(e.code(), "INVALID_COORDINATE");
    }

    #[test]
    fn code_io_error() {
        let e = AppError::Io {
            reason: "fail".into(),
        };
        assert_eq!(e.code(), "IO_ERROR");
    }

    #[test]
    fn code_invalid_json() {
        let e = AppError::InvalidJson {
            reason: "parse".into(),
        };
        assert_eq!(e.code(), "INVALID_JSON");
    }

    // -----------------------------------------------------------------------
    // retryable() — only RelayUnreachable is true
    // -----------------------------------------------------------------------

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
        assert!(!AppError::RelayRejected { reason: "x".into() }.retryable());
        assert!(
            !AppError::HeaderNotFound {
                event_id: "x".into()
            }
            .retryable()
        );
        assert!(!AppError::HeaderMissingDTag.retryable());
        assert!(!AppError::InvalidEventId { id: "x".into() }.retryable());
        assert!(!AppError::NoResults.retryable());
        assert!(!AppError::InvalidNsec.retryable());
        assert!(!AppError::KeysSaveFailed { reason: "x".into() }.retryable());
        assert!(!AppError::KeysAlreadyExist { path: "x".into() }.retryable());
        assert!(!AppError::InvalidCoordinate { input: "x".into() }.retryable());
        assert!(!AppError::Io { reason: "x".into() }.retryable());
        assert!(!AppError::InvalidJson { reason: "x".into() }.retryable());
    }

    // -----------------------------------------------------------------------
    // fix() — non-empty, contains interpolated fields where applicable
    // -----------------------------------------------------------------------

    #[test]
    fn fix_keys_not_found_suggests_init() {
        let fix = AppError::KeysNotFound {
            path: "/tmp".into(),
        }
        .fix();
        assert!(fix.contains("init"));
    }

    #[test]
    fn fix_relay_unreachable_contains_url() {
        let fix = AppError::RelayUnreachable {
            url: "ws://myrelay".into(),
        }
        .fix();
        assert!(fix.contains("ws://myrelay"));
    }

    #[test]
    fn fix_keys_already_exist_contains_path() {
        let fix = AppError::KeysAlreadyExist {
            path: "/home/.wokhei/keys".into(),
        }
        .fix();
        assert!(fix.contains("/home/.wokhei/keys"));
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
            AppError::NoResults,
            AppError::InvalidNsec,
            AppError::KeysSaveFailed { reason: "r".into() },
            AppError::KeysAlreadyExist { path: "p".into() },
            AppError::InvalidCoordinate { input: "i".into() },
            AppError::Io { reason: "r".into() },
            AppError::InvalidJson { reason: "r".into() },
        ];
        for v in variants {
            assert!(!v.fix().is_empty(), "fix() empty for {}", v.code());
        }
    }

    // -----------------------------------------------------------------------
    // From<AppError> for CommandError — preserves code, retryable, message
    // -----------------------------------------------------------------------

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
    fn command_error_from_app_error_preserves_message() {
        let app_err = AppError::Io {
            reason: "disk full".into(),
        };
        let cmd_err = CommandError::from(app_err);
        assert!(cmd_err.message.contains("disk full"));
    }
}
