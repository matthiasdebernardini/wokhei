use nostr_sdk::prelude::*;
use serde_json::json;

use agcli::{CommandError, CommandOutput, NextAction};

use crate::error::AppError;
use crate::keys::load_keys;

pub async fn delete(
    relay: String,
    event_id_strs: Vec<String>,
) -> Result<CommandOutput, CommandError> {
    let keys = load_keys().map_err(|e| {
        CommandError::from(e).next_actions(vec![NextAction::new(
            "wokhei init --generate",
            "Generate a keypair first",
        )])
    })?;

    let mut event_ids = Vec::new();
    for id_str in &event_id_strs {
        let id = EventId::parse(id_str)
            .map_err(|_| CommandError::from(AppError::InvalidEventId { id: id_str.clone() }))?;
        event_ids.push(id);
    }

    let client = Client::builder().signer(keys).build();
    if client.add_relay(&relay).await.is_err() {
        return Err(CommandError::from(AppError::RelayUnreachable {
            url: relay.clone(),
        }));
    }
    client.connect().await;

    let mut request = EventDeletionRequest::new();
    for id in event_ids {
        request = request.id(id);
    }
    let builder = EventBuilder::delete(request);

    let result = match client.send_event_builder(builder).await {
        Ok(output) => {
            let deletion_id = output.val.to_hex();

            let result = json!({
                "deletion_event_id": deletion_id,
                "deleted_ids": event_id_strs,
                "note": "NIP-09: deletion is a REQUEST — relays may or may not honor it"
            });

            let actions = vec![NextAction::new(
                format!("wokhei list-headers --relay={relay}"),
                "List headers to verify deletion",
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
