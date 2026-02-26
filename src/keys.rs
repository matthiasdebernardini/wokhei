use std::fs;
use std::io::{self, Read as IoRead};
use std::path::PathBuf;

use agcli::{CommandError, CommandOutput, NextAction};
use nostr_sdk::prelude::*;
use serde_json::json;

use crate::error::AppError;

fn keys_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".wokhei")
}

fn keys_path() -> PathBuf {
    keys_dir().join("keys")
}

pub fn keys_exist() -> bool {
    keys_path().exists()
}

pub fn load_keys() -> Result<Keys, AppError> {
    let path = keys_path();
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

fn save_keys(keys: &Keys) -> Result<(), AppError> {
    let dir = keys_dir();
    fs::create_dir_all(&dir).map_err(|e| AppError::KeysSaveFailed {
        reason: e.to_string(),
    })?;

    let path = keys_path();
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
            "wokhei create-header --name=<name> --title=<title>",
            "Create your first list header",
        ),
        NextAction::new(
            format!("wokhei list-headers --author={pubkey_hex}"),
            "List your headers",
        ),
    ]
}

fn read_nsec_from_source(source: &str) -> Result<String, CommandError> {
    let raw = if source == "-" {
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf).map_err(|e| {
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
            "wokhei create-header --name=<name> --title=<title>",
            "Create a new list header",
        ),
    ];
    Ok(CommandOutput::new(keys_result(&keys)).next_actions(actions))
}
