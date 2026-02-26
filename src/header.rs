use agcli::{CommandError, CommandOutput, NextAction};
use nostr_sdk::prelude::*;
use serde_json::json;

use crate::error::AppError;
use crate::keys::load_keys;

pub struct HeaderParams {
    pub relay: String,
    pub name: String,
    pub aliases: Vec<String>,
    pub title: String,
    pub description: Option<String>,
    pub required: Vec<String>,
    pub recommended: Vec<String>,
    pub tags_list: Vec<String>,
    pub alt: Option<String>,
    pub addressable: bool,
    pub d_tag: Option<String>,
}

fn build_header_tags(params: &HeaderParams, kind: Kind) -> Vec<Tag> {
    let HeaderParams {
        name,
        aliases,
        title,
        description,
        required,
        recommended,
        tags_list,
        alt,
        d_tag,
        ..
    } = params;

    let mut event_tags: Vec<Tag> = Vec::new();

    let mut name_values = vec![name.clone()];
    name_values.extend(aliases.clone());
    event_tags.push(Tag::custom(TagKind::custom("names"), name_values));
    event_tags.push(Tag::custom(TagKind::custom("title"), [title.clone()]));

    if let Some(desc) = description {
        event_tags.push(Tag::custom(TagKind::custom("description"), [desc.clone()]));
    }
    if !required.is_empty() {
        event_tags.push(Tag::custom(TagKind::custom("required"), required.clone()));
    }
    if !recommended.is_empty() {
        event_tags.push(Tag::custom(
            TagKind::custom("recommended"),
            recommended.clone(),
        ));
    }
    for tag in tags_list {
        event_tags.push(Tag::hashtag(tag));
    }

    let alt_text = alt
        .clone()
        .unwrap_or_else(|| format!("DCoSL list header: {name} — {title}"));
    event_tags.push(Tag::custom(TagKind::custom("alt"), [alt_text]));
    event_tags.push(Tag::custom(TagKind::custom("client"), ["wokhei"]));

    if let Some(d) = d_tag {
        event_tags.push(Tag::identifier(d));
    }

    // Suppress unused variable warning — kind is used by caller context
    let _ = kind;
    event_tags
}

pub async fn create_header(params: HeaderParams) -> Result<CommandOutput, CommandError> {
    let keys = load_keys().map_err(|e| {
        CommandError::from(e).next_actions(vec![NextAction::new(
            "wokhei init --generate",
            "Generate a keypair first",
        )])
    })?;

    if params.addressable && params.d_tag.is_none() {
        return Err(CommandError::new(
            "--addressable requires --d-tag=<identifier>",
            "MISSING_ARG",
            format!(
                "Re-run with: wokhei create-header --relay={} --name={} --title=\"{}\" --addressable --d-tag=<identifier>",
                params.relay, params.name, params.title,
            ),
        ));
    }

    let kind = if params.addressable {
        Kind::Custom(39998)
    } else {
        Kind::Custom(9998)
    };

    let event_tags = build_header_tags(&params, kind);
    let builder = EventBuilder::new(kind, "").tags(event_tags);

    let client = Client::builder().signer(keys.clone()).build();
    if client.add_relay(&params.relay).await.is_err() {
        return Err(CommandError::from(AppError::RelayUnreachable {
            url: params.relay.clone(),
        }));
    }
    client.connect().await;

    let result = match client.send_event_builder(builder).await {
        Ok(output) => {
            let event_id = output.val.to_hex();
            let pubkey_hex = keys.public_key().to_hex();
            let relay = &params.relay;
            let mut result = json!({
                "event_id": event_id,
                "kind": kind.as_u16(),
                "pubkey": pubkey_hex,
                "created_at": jiff::Timestamp::now().to_string(),
                "tags_count": params.tags_list.len(),
            });

            if let Some(ref d) = params.d_tag {
                let coord = format!("{}:{}:{}", kind.as_u16(), pubkey_hex, d);
                result["coordinate"] = json!(coord);
            }

            let mut actions = vec![
                NextAction::new(
                    format!("wokhei add-item --relay={relay} --header={event_id} --resource=<url>"),
                    "Add an item to this list",
                ),
                NextAction::new(
                    format!("wokhei inspect --relay={relay} {event_id}"),
                    "Inspect the created header",
                ),
                NextAction::new(
                    format!("wokhei list-headers --relay={relay}"),
                    "List all headers on this relay",
                ),
            ];

            if let Some(ref d) = params.d_tag {
                let coord = format!("{}:{}:{}", kind.as_u16(), pubkey_hex, d);
                actions.insert(
                    1,
                    NextAction::new(
                        format!("wokhei add-item --relay={relay} --header-coordinate=\"{coord}\" --resource=<url>"),
                        "Add item using coordinate reference",
                    ),
                );
            }

            Ok(CommandOutput::new(result).next_actions(actions))
        }
        Err(e) => Err(CommandError::from(AppError::RelayRejected {
            reason: e.to_string(),
        })),
    };

    client.disconnect().await;
    result
}
