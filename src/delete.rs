use nostr_sdk::prelude::*;
use serde_json::json;

use crate::error::AppError;
use crate::keys::load_keys;
use crate::response::{NextAction, Response};

pub async fn delete(relay: String, event_id_strs: Vec<String>) -> Response {
    let cmd = "delete";

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

    let mut event_ids = Vec::new();
    for id_str in &event_id_strs {
        let Ok(id) = EventId::parse(id_str) else {
            return Response::error(
                cmd,
                &AppError::InvalidEventId { id: id_str.clone() },
                vec![],
            );
        };
        event_ids.push(id);
    }

    let client = Client::builder().signer(keys).build();
    if client.add_relay(&relay).await.is_err() {
        let err = AppError::RelayUnreachable { url: relay.clone() };
        return Response::error(cmd, &err, vec![]);
    }
    client.connect().await;

    let mut request = EventDeletionRequest::new();
    for id in event_ids {
        request = request.id(id);
    }
    let builder = EventBuilder::delete(request);

    match client.send_event_builder(builder).await {
        Ok(output) => {
            let deletion_id = output.val.to_hex();

            let result = json!({
                "deletion_event_id": deletion_id,
                "deleted_ids": event_id_strs,
                "note": "NIP-09: deletion is a REQUEST — relays may or may not honor it"
            });

            let actions = vec![NextAction::simple(
                &format!("wokhei list-headers --relay {relay}"),
                "List headers to verify deletion",
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
