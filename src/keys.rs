use std::fs;
use std::path::PathBuf;

use nostr_sdk::prelude::*;
use serde_json::json;

use crate::error::AppError;
use crate::response::{NextAction, Response};

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
        NextAction::simple("wokhei whoami", "Verify your identity"),
        NextAction::simple(
            "wokhei create-header --relay ws://localhost:7777 --name <name> --title <title>",
            "Create your first list header",
        ),
        NextAction::simple(
            &format!("wokhei list-headers --relay ws://localhost:7777 --author {pubkey_hex}"),
            "List your headers",
        ),
    ]
}

pub fn init(generate: bool, import: Option<String>) -> Response {
    let cmd = "init";

    if !generate && import.is_none() {
        return Response::error(
            cmd,
            &AppError::Io {
                reason: "Specify --generate or --import <nsec>".to_string(),
            },
            vec![
                NextAction::simple("wokhei init --generate", "Generate a new keypair"),
                NextAction::simple("wokhei init --import <nsec>", "Import an existing nsec key"),
            ],
        );
    }

    let path = keys_path();
    if path.exists() {
        let err = AppError::KeysAlreadyExist {
            path: path.display().to_string(),
        };
        return Response::error(
            cmd,
            &err,
            vec![NextAction::simple(
                "wokhei whoami",
                "Check current identity",
            )],
        );
    }

    let keys = if generate {
        Keys::generate()
    } else if let Some(nsec) = import {
        let Ok(k) = Keys::parse(&nsec) else {
            return Response::error(
                cmd,
                &AppError::InvalidNsec,
                vec![NextAction::simple(
                    "wokhei init --generate",
                    "Generate a new keypair instead",
                )],
            );
        };
        k
    } else {
        unreachable!()
    };

    if let Err(e) = save_keys(&keys) {
        return Response::error(cmd, &e, vec![]);
    }

    let pubkey_hex = keys.public_key().to_hex();
    let actions = post_init_actions(&pubkey_hex);
    Response::success(cmd, keys_result(&keys), actions)
}

pub fn whoami() -> Response {
    let cmd = "whoami";
    match load_keys() {
        Ok(keys) => {
            let pubkey_hex = keys.public_key().to_hex();
            let actions = vec![
                NextAction::simple(
                    &format!(
                        "wokhei list-headers --relay ws://localhost:7777 --author {pubkey_hex}"
                    ),
                    "List your headers",
                ),
                NextAction::simple(
                    "wokhei create-header --relay ws://localhost:7777 --name <name> --title <title>",
                    "Create a new list header",
                ),
            ];
            Response::success(cmd, keys_result(&keys), actions)
        }
        Err(e) => Response::error(
            cmd,
            &e,
            vec![NextAction::simple(
                "wokhei init --generate",
                "Generate a new keypair",
            )],
        ),
    }
}
