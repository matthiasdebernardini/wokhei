use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use agcli::{CommandError, CommandOutput, NextAction};
use nostr_sdk::prelude::*;
use serde_json::json;

use crate::error::AppError;

// ---------------------------------------------------------------------------
// Parameterized path helpers (testable without touching $HOME)
// ---------------------------------------------------------------------------

fn keys_dir_from(base: &Path) -> PathBuf {
    base.join(".wokhei")
}

fn keys_path_from(base: &Path) -> PathBuf {
    keys_dir_from(base).join("keys")
}

fn home_base() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

fn keys_path() -> PathBuf {
    keys_path_from(&home_base())
}

pub fn keys_exist() -> bool {
    keys_path().exists()
}

fn load_keys_from(base: &Path) -> Result<Keys, AppError> {
    let path = keys_path_from(base);
    if !path.exists() {
        return Err(AppError::KeysNotFound {
            path: path.display().to_string(),
        });
    }
    let nsec = fs::read_to_string(&path).map_err(|e| AppError::Io {
        reason: e.to_string(),
    })?;
    Keys::parse(nsec.trim()).map_err(|_| AppError::InvalidNsec)
}

pub fn load_keys() -> Result<Keys, AppError> {
    load_keys_from(&home_base())
}

fn save_keys_at(base: &Path, keys: &Keys) -> Result<(), AppError> {
    let dir = keys_dir_from(base);
    fs::create_dir_all(&dir).map_err(|e| AppError::KeysSaveFailed {
        reason: e.to_string(),
    })?;

    let path = keys_path_from(base);
    let nsec = keys
        .secret_key()
        .to_bech32()
        .map_err(|e| AppError::KeysSaveFailed {
            reason: e.to_string(),
        })?;

    fs::write(&path, &nsec).map_err(|e| AppError::KeysSaveFailed {
        reason: e.to_string(),
    })?;

    // chmod 0600
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).map_err(|e| {
            AppError::KeysSaveFailed {
                reason: e.to_string(),
            }
        })?;
    }

    Ok(())
}

fn save_keys(keys: &Keys) -> Result<(), AppError> {
    save_keys_at(&home_base(), keys)
}

fn keys_result(keys: &Keys) -> serde_json::Value {
    let pubkey_hex = keys.public_key().to_hex();
    let npub = keys
        .public_key()
        .to_bech32()
        .unwrap_or_else(|_| pubkey_hex.clone());
    json!({
        "pubkey": pubkey_hex,
        "npub": npub,
        "keys_path": keys_path().display().to_string()
    })
}

fn post_init_actions(pubkey_hex: &str) -> Vec<NextAction> {
    vec![
        NextAction::new("wokhei whoami", "Verify your identity"),
        NextAction::new(
            "wokhei create-header --name=<singular> --plural=<plural>",
            "Create your first list header",
        ),
        NextAction::new(
            format!("wokhei list-headers --author={pubkey_hex}"),
            "List your headers",
        ),
    ]
}

fn read_nsec<R: io::Read>(source: &str, stdin: R) -> Result<String, CommandError> {
    let raw = if source == "-" {
        let mut buf = String::new();
        let mut reader = stdin;
        reader.read_to_string(&mut buf).map_err(|e| {
            CommandError::from(AppError::Io {
                reason: e.to_string(),
            })
        })?;
        buf
    } else {
        fs::read_to_string(source).map_err(|e| {
            CommandError::from(AppError::Io {
                reason: format!("Failed to read {source}: {e}"),
            })
        })?
    };
    Ok(raw.trim().to_string())
}

fn read_nsec_from_source(source: &str) -> Result<String, CommandError> {
    read_nsec(source, io::stdin())
}

pub fn init(generate: bool, import: Option<&str>) -> Result<CommandOutput, CommandError> {
    if !generate && import.is_none() {
        return Err(CommandError::new(
            "Specify --generate or --import <source>",
            "MISSING_ARG",
            "Use --generate to create a new keypair, or --import - (stdin) / --import <file>",
        )
        .next_actions(vec![
            NextAction::new("wokhei init --generate", "Generate a new keypair"),
            NextAction::new("wokhei init --import -", "Import nsec from stdin"),
        ]));
    }

    let path = keys_path();
    if path.exists() {
        return Err(CommandError::from(AppError::KeysAlreadyExist {
            path: path.display().to_string(),
        })
        .next_actions(vec![NextAction::new(
            "wokhei whoami",
            "Check current identity",
        )]));
    }

    let keys = if generate {
        Keys::generate()
    } else if let Some(source) = import {
        let nsec = read_nsec_from_source(source)?;
        Keys::parse(&nsec).map_err(|_| {
            CommandError::from(AppError::InvalidNsec).next_actions(vec![NextAction::new(
                "wokhei init --generate",
                "Generate a new keypair instead",
            )])
        })?
    } else {
        unreachable!()
    };

    save_keys(&keys).map_err(CommandError::from)?;

    let pubkey_hex = keys.public_key().to_hex();
    let actions = post_init_actions(&pubkey_hex);
    Ok(CommandOutput::new(keys_result(&keys)).next_actions(actions))
}

pub fn whoami() -> Result<CommandOutput, CommandError> {
    let keys = load_keys().map_err(|e| {
        CommandError::from(e).next_actions(vec![NextAction::new(
            "wokhei init --generate",
            "Generate a new keypair",
        )])
    })?;

    let pubkey_hex = keys.public_key().to_hex();
    let actions = vec![
        NextAction::new(
            format!("wokhei list-headers --author={pubkey_hex}"),
            "List your headers",
        ),
        NextAction::new(
            "wokhei create-header --name=<singular> --plural=<plural>",
            "Create a new list header",
        ),
    ];
    Ok(CommandOutput::new(keys_result(&keys)).next_actions(actions))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    // -----------------------------------------------------------------------
    // keys_dir_from / keys_path_from — pure path helpers
    // -----------------------------------------------------------------------

    #[test]
    fn keys_dir_from_appends_wokhei() {
        let base = Path::new("/tmp/test-home");
        assert_eq!(keys_dir_from(base), PathBuf::from("/tmp/test-home/.wokhei"));
    }

    #[test]
    fn keys_path_from_appends_keys() {
        let base = Path::new("/tmp/test-home");
        assert_eq!(
            keys_path_from(base),
            PathBuf::from("/tmp/test-home/.wokhei/keys")
        );
    }

    // -----------------------------------------------------------------------
    // keys_result — pure function
    // -----------------------------------------------------------------------

    #[test]
    fn keys_result_contains_pubkey() {
        let keys = Keys::generate();
        let j = keys_result(&keys);
        assert_eq!(j["pubkey"].as_str().unwrap(), keys.public_key().to_hex());
    }

    #[test]
    fn keys_result_npub_starts_with_npub1() {
        let keys = Keys::generate();
        let j = keys_result(&keys);
        assert!(j["npub"].as_str().unwrap().starts_with("npub1"));
    }

    #[test]
    fn keys_result_has_keys_path() {
        let keys = Keys::generate();
        let j = keys_result(&keys);
        assert!(j["keys_path"].as_str().unwrap().contains(".wokhei/keys"));
    }

    // -----------------------------------------------------------------------
    // post_init_actions — pure function
    // -----------------------------------------------------------------------

    #[test]
    fn post_init_actions_non_empty() {
        let actions = post_init_actions("abc123");
        assert!(!actions.is_empty());
    }

    #[test]
    fn post_init_actions_contains_whoami() {
        let actions = post_init_actions("abc123");
        assert!(actions.iter().any(|a| a.command.contains("whoami")));
    }

    #[test]
    fn post_init_actions_contains_pubkey() {
        let actions = post_init_actions("abc123");
        assert!(actions.iter().any(|a| a.command.contains("abc123")));
    }

    // -----------------------------------------------------------------------
    // read_nsec with impl Read — uses Cursor for stdin mock
    // -----------------------------------------------------------------------

    #[test]
    fn read_nsec_from_stdin_cursor() {
        let cursor = Cursor::new(b"nsec1test\n");
        let result = read_nsec("-", cursor).unwrap();
        assert_eq!(result, "nsec1test");
    }

    #[test]
    fn read_nsec_trims_whitespace() {
        let cursor = Cursor::new(b"  nsec1spaced  \n");
        let result = read_nsec("-", cursor).unwrap();
        assert_eq!(result, "nsec1spaced");
    }

    #[test]
    fn read_nsec_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nsec.txt");
        fs::write(&path, "nsec1fromfile\n").unwrap();
        let cursor = Cursor::new(b""); // unused when reading from file
        let result = read_nsec(path.to_str().unwrap(), cursor).unwrap();
        assert_eq!(result, "nsec1fromfile");
    }

    #[test]
    fn read_nsec_nonexistent_file_errors() {
        let cursor = Cursor::new(b"");
        let err = read_nsec("/nonexistent/path/file.txt", cursor).unwrap_err();
        assert_eq!(err.code, "IO_ERROR");
    }

    // -----------------------------------------------------------------------
    // save_keys_at / load_keys_from — filesystem tests with tempfile
    // -----------------------------------------------------------------------

    #[test]
    fn save_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let keys = Keys::generate();
        save_keys_at(dir.path(), &keys).unwrap();
        let loaded = load_keys_from(dir.path()).unwrap();
        assert_eq!(loaded.public_key(), keys.public_key());
    }

    #[test]
    fn load_from_nonexistent_path_errors() {
        let dir = tempfile::tempdir().unwrap();
        let err = load_keys_from(dir.path()).unwrap_err();
        assert_eq!(err.code(), "KEYS_NOT_FOUND");
    }

    #[test]
    fn save_creates_directory_and_file() {
        let dir = tempfile::tempdir().unwrap();
        let keys = Keys::generate();
        save_keys_at(dir.path(), &keys).unwrap();
        assert!(keys_path_from(dir.path()).exists());
        assert!(keys_dir_from(dir.path()).is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn save_sets_permissions_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let keys = Keys::generate();
        save_keys_at(dir.path(), &keys).unwrap();
        let metadata = fs::metadata(keys_path_from(dir.path())).unwrap();
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
    }

    // -----------------------------------------------------------------------
    // init — neither flag errors
    // -----------------------------------------------------------------------

    #[test]
    fn init_neither_flag_errors() {
        let err = init(false, None).unwrap_err();
        assert_eq!(err.code, "MISSING_ARG");
    }

    #[test]
    fn init_generate_does_not_return_missing_arg() {
        // With generate=true the guard must be skipped.
        // It may fail for other reasons (keys already exist, etc.) but NOT MISSING_ARG.
        match init(true, None) {
            Ok(_) => {} // generated keys successfully
            Err(e) => assert_ne!(e.code, "MISSING_ARG"),
        }
    }
}
