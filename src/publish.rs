use std::fs;
use std::io::{self, Read as IoRead};

use nostr_sdk::prelude::*;
use serde_json::json;

use crate::error::AppError;
use crate::keys::load_keys;
use crate::response::{NextAction, Response};

pub async fn publish(relay: String, input: String) -> Response {
    let cmd = "publish";

    let Ok(keys) = load_keys() else {
        return Response::error(
            cmd,
            &AppError::KeysNotFound {
                path: "~/.wokhei/keys".to_string(),
            },
            vec![NextAction::simple(
                "wokhei init --generate",
                "Generate a keypair first",
            )],
        );
    };

    // Read JSON input
    let json_str = if input == "-" {
        let mut buf = String::new();
        if let Err(e) = io::stdin().read_to_string(&mut buf) {
            return Response::error(
                cmd,
                &AppError::Io {
                    reason: e.to_string(),
                },
                vec![],
            );
        }
        buf
    } else {
        let Ok(s) = fs::read_to_string(&input) else {
            return Response::error(
                cmd,
                &AppError::Io {
                    reason: format!("Failed to read {input}"),
                },
                vec![],
            );
        };
        s
    };

    // Parse as unsigned event JSON
    let Ok(raw) = serde_json::from_str::<serde_json::Value>(&json_str) else {
        return Response::error(
            cmd,
            &AppError::InvalidJson {
                reason: "Failed to parse JSON input".to_string(),
            },
            vec![],
        );
    };

    // Extract kind, content, tags
    #[allow(clippy::cast_possible_truncation)] // Nostr kinds fit in u16
    let kind_num = raw["kind"].as_u64().unwrap_or(1) as u16;
    let content = raw["content"].as_str().unwrap_or("");

    let mut event_tags: Vec<Tag> = Vec::new();
    if let Some(tags_arr) = raw["tags"].as_array() {
        for tag_val in tags_arr {
            if let Some(tag_arr) = tag_val.as_array() {
                let parts: Vec<String> = tag_arr
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect();
                if parts.len() >= 2 {
                    let kind = TagKind::custom(&parts[0]);
                    let values: Vec<&str> = parts[1..].iter().map(String::as_str).collect();
                    event_tags.push(Tag::custom(kind, values));
                }
            }
        }
    }

    let builder = EventBuilder::new(Kind::Custom(kind_num), content).tags(event_tags);

    let client = Client::builder().signer(keys).build();
    if client.add_relay(&relay).await.is_err() {
        let err = AppError::RelayUnreachable { url: relay.clone() };
        return Response::error(cmd, &err, vec![]);
    }
    client.connect().await;

    match client.send_event_builder(builder).await {
        Ok(output) => {
            let event_id = output.val.to_hex();
            let result = json!({
                "event_id": event_id,
                "kind": kind_num,
            });

            let actions = vec![NextAction::simple(
                &format!("wokhei inspect --relay {relay} {event_id}"),
                "Inspect the published event",
            )];

            client.disconnect().await;
            Response::success(cmd, result, actions)
        }
        Err(e) => {
            client.disconnect().await;
            let err = AppError::RelayRejected {
                reason: e.to_string(),
            };
            Response::error(cmd, &err, vec![])
        }
    }
}
