use std::fs;
use std::io;

use nostr_sdk::prelude::*;
use serde_json::json;

use agcli::{CommandError, CommandOutput, NextAction};

use crate::error::AppError;
use crate::keys::load_keys;

fn read_json_input<R: io::Read>(input: &str, stdin: R) -> Result<String, CommandError> {
    if input == "-" {
        let mut buf = String::new();
        let mut reader = stdin;
        reader.read_to_string(&mut buf).map_err(|e| {
            CommandError::from(AppError::Io {
                reason: e.to_string(),
            })
        })?;
        Ok(buf)
    } else {
        fs::read_to_string(input).map_err(|_| {
            CommandError::from(AppError::Io {
                reason: format!("Failed to read {input}"),
            })
        })
    }
}

pub async fn publish(relay: String, input: String) -> Result<CommandOutput, CommandError> {
    let keys = load_keys().map_err(|e| {
        CommandError::from(e).next_actions(vec![NextAction::new(
            "wokhei init --generate",
            "Generate a keypair first",
        )])
    })?;

    // Read JSON input
    let json_str = read_json_input(&input, io::stdin())?;

    // Parse as unsigned event JSON
    let raw: serde_json::Value = serde_json::from_str(&json_str).map_err(|_| {
        CommandError::from(AppError::InvalidJson {
            reason: "Failed to parse JSON input".to_string(),
        })
    })?;

    // Extract kind, content, tags
    #[allow(clippy::cast_possible_truncation)] // Nostr kinds fit in u16
    let kind_num = raw["kind"].as_u64().unwrap_or(1) as u16;
    let content = raw["content"].as_str().unwrap_or("");

    let event_tags: Vec<Tag> = raw["tags"]
        .as_array()
        .map(|tags_arr| {
            tags_arr
                .iter()
                .filter_map(|tag_val| {
                    tag_val.as_array().and_then(|tag_arr| {
                        let parts: Vec<String> = tag_arr
                            .iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect();
                        (parts.len() >= 2).then(|| {
                            let kind = TagKind::custom(&parts[0]);
                            let values: Vec<&str> = parts[1..].iter().map(String::as_str).collect();
                            Tag::custom(kind, values)
                        })
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let builder = EventBuilder::new(Kind::Custom(kind_num), content).tags(event_tags);

    let client = Client::builder().signer(keys).build();
    if client.add_relay(&relay).await.is_err() {
        return Err(CommandError::from(AppError::RelayUnreachable {
            url: relay.clone(),
        }));
    }
    client.connect().await;

    let result = match client.send_event_builder(builder).await {
        Ok(output) => {
            let event_id = output.val.to_hex();
            let result = json!({
                "event_id": event_id,
                "kind": kind_num,
            });

            let actions = vec![NextAction::new(
                format!("wokhei inspect --relay={relay} {event_id}"),
                "Inspect the published event",
            )];

            Ok(CommandOutput::new(result).next_actions(actions))
        }
        Err(e) => Err(CommandError::from(AppError::RelayRejected {
            reason: e.to_string(),
        })),
    };

    client.disconnect().await;
    result
}
