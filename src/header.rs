use nostr_sdk::prelude::*;
use serde_json::json;

use crate::error::AppError;
use crate::keys::load_keys;
use crate::response::{NextAction, Response};

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

pub async fn create_header(params: HeaderParams) -> Response {
    let cmd = "create-header";

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

    if params.addressable && params.d_tag.is_none() {
        return Response::error(
            cmd,
            &AppError::Io {
                reason: "--addressable requires --d-tag <identifier>".to_string(),
            },
            vec![NextAction::simple(
                &format!(
                    "wokhei create-header --relay {} --name {} --title \"{}\" --addressable --d-tag <identifier>",
                    params.relay, params.name, params.title,
                ),
                "Re-run with --d-tag",
            )],
        );
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
        let err = AppError::RelayUnreachable {
            url: params.relay.clone(),
        };
        return Response::error(cmd, &err, vec![]);
    }
    client.connect().await;

    match client.send_event_builder(builder).await {
        Ok(output) => {
            let event_id = output.val.to_hex();
            let pubkey_hex = keys.public_key().to_hex();
            let relay = &params.relay;
            let mut result = json!({
                "event_id": event_id,
                "kind": kind.as_u16(),
                "pubkey": pubkey_hex,
                "created_at": chrono::Utc::now().to_rfc3339(),
                "tags_count": params.tags_list.len(),
            });

            if let Some(ref d) = params.d_tag {
                let coord = format!("{}:{}:{}", kind.as_u16(), pubkey_hex, d);
                result["coordinate"] = json!(coord);
            }

            let mut actions = vec![
                NextAction::simple(
                    &format!(
                        "wokhei add-item --relay {relay} --header {event_id} --resource <url>"
                    ),
                    "Add an item to this list",
                ),
                NextAction::simple(
                    &format!("wokhei inspect --relay {relay} {event_id}"),
                    "Inspect the created header",
                ),
                NextAction::simple(
                    &format!("wokhei list-headers --relay {relay}"),
                    "List all headers on this relay",
                ),
            ];

            if let Some(ref d) = params.d_tag {
                let coord = format!("{}:{}:{}", kind.as_u16(), pubkey_hex, d);
                actions.insert(
                    1,
                    NextAction::simple(
                        &format!(
                            "wokhei add-item --relay {relay} --header-coordinate \"{coord}\" --resource <url>"
                        ),
                        "Add item using coordinate reference",
                    ),
                );
            }

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
