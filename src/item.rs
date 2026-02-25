use nostr_sdk::prelude::*;
use serde_json::json;
use std::time::Duration;

use agcli::{CommandError, CommandOutput, NextAction};

use crate::error::AppError;
use crate::keys::load_keys;

fn parse_coordinate_str(input: &str) -> Result<(u16, PublicKey, String), AppError> {
    let parts: Vec<&str> = input.splitn(3, ':').collect();
    if parts.len() != 3 {
        return Err(AppError::InvalidCoordinate {
            input: input.to_string(),
        });
    }
    let kind_num: u16 = parts[0].parse().map_err(|_| AppError::InvalidCoordinate {
        input: input.to_string(),
    })?;
    let pubkey = PublicKey::parse(parts[1]).map_err(|_| AppError::InvalidCoordinate {
        input: input.to_string(),
    })?;
    let d_tag = parts[2].to_string();
    Ok((kind_num, pubkey, d_tag))
}

pub struct ItemParams {
    pub relay: String,
    pub header: Option<String>,
    pub header_coordinate: Option<String>,
    pub resource: String,
    pub content: Option<String>,
    pub fields: Vec<String>,
    pub z_tag: String,
    pub addressable: bool,
    pub d_tag: Option<String>,
}

async fn resolve_header_ref(
    client: &Client,
    relay: &str,
    resource: &str,
    header: Option<&str>,
    header_coordinate: Option<&str>,
) -> Result<(Tag, String), CommandError> {
    if let Some(coord_str) = header_coordinate {
        let (kind_num, pubkey, d_val) =
            parse_coordinate_str(coord_str).map_err(CommandError::from)?;
        let coord = Coordinate::new(Kind::Custom(kind_num), pubkey).identifier(&d_val);
        let tag = Tag::coordinate(coord, None);
        Ok((tag, coord_str.to_string()))
    } else if let Some(header_id_str) = header {
        resolve_header_by_id(client, relay, resource, header_id_str).await
    } else {
        unreachable!()
    }
}

async fn resolve_header_by_id(
    client: &Client,
    relay: &str,
    resource: &str,
    header_id_str: &str,
) -> Result<(Tag, String), CommandError> {
    let event_id = EventId::parse(header_id_str).map_err(|_| {
        CommandError::from(AppError::InvalidEventId {
            id: header_id_str.to_string(),
        })
    })?;

    let filter = Filter::new().id(event_id).limit(1);
    let events = client
        .fetch_events(filter, Duration::from_secs(10))
        .await
        .map_err(|_| {
            CommandError::from(AppError::RelayUnreachable {
                url: relay.to_string(),
            })
        })?;

    let header_event = events.into_iter().next().ok_or_else(|| {
        CommandError::from(AppError::HeaderNotFound {
            event_id: header_id_str.to_string(),
        })
        .next_actions(vec![NextAction::new(
            format!(
                "wokhei add-item --relay {relay} --header-coordinate <kind:pubkey:d-tag> --resource \"{resource}\""
            ),
            "Use coordinate reference instead (cross-relay)",
        )])
    })?;

    if header_event.kind == Kind::Custom(39998) {
        let d_val = header_event.tags.iter().find_map(|t| {
            let tag_vec = t.as_slice();
            if tag_vec.first().map(String::as_str) == Some("d") {
                tag_vec.get(1).cloned()
            } else {
                None
            }
        });

        let d = d_val.ok_or_else(|| CommandError::from(AppError::HeaderMissingDTag))?;

        let coord = Coordinate::new(Kind::Custom(39998), header_event.pubkey).identifier(&d);
        let ref_str = format!("39998:{}:{}", header_event.pubkey.to_hex(), d);
        Ok((Tag::coordinate(coord, None), ref_str))
    } else {
        Ok((Tag::event(event_id), header_id_str.to_string()))
    }
}

fn build_item_tags(
    header_ref_tag: Tag,
    resource: &str,
    z_tag: &str,
    fields: &[String],
    d_tag: Option<&str>,
) -> Vec<Tag> {
    let mut event_tags: Vec<Tag> = vec![header_ref_tag];
    event_tags.push(Tag::custom(TagKind::custom("r"), [resource]));
    event_tags.push(Tag::custom(TagKind::custom("z"), [z_tag]));
    event_tags.push(Tag::custom(TagKind::custom("client"), ["wokhei"]));

    for field in fields {
        if let Some((key, val)) = field.split_once('=') {
            event_tags.push(Tag::custom(TagKind::custom(key), [val]));
        }
    }

    if let Some(d) = d_tag {
        event_tags.push(Tag::identifier(d));
    }

    event_tags
}

fn validate_item_params(params: &ItemParams) -> Result<(), CommandError> {
    if params.header.is_none() && params.header_coordinate.is_none() {
        return Err(CommandError::new(
            "Specify --header <event-id> or --header-coordinate <kind:pubkey:d-tag>",
            "MISSING_ARG",
            "Use --header with an event ID, or --header-coordinate with kind:pubkey:d-tag",
        ));
    }
    if params.addressable && params.d_tag.is_none() {
        return Err(CommandError::new(
            "--addressable requires --d-tag <identifier>",
            "MISSING_ARG",
            "Add --d-tag <identifier> when using --addressable",
        ));
    }
    Ok(())
}

pub async fn add_item(params: ItemParams) -> Result<CommandOutput, CommandError> {
    let keys = load_keys().map_err(|e| {
        CommandError::from(e).next_actions(vec![NextAction::new(
            "wokhei init --generate",
            "Generate a keypair first",
        )])
    })?;

    validate_item_params(&params)?;

    let ItemParams {
        relay,
        header,
        header_coordinate,
        resource,
        content,
        fields,
        z_tag,
        addressable,
        d_tag,
    } = params;

    let item_kind = if addressable {
        Kind::Custom(39999)
    } else {
        Kind::Custom(9999)
    };

    let client = Client::builder().signer(keys.clone()).build();
    if client.add_relay(&relay).await.is_err() {
        return Err(CommandError::from(AppError::RelayUnreachable {
            url: relay.clone(),
        }));
    }
    client.connect().await;

    let result = async {
        let (header_ref_tag, header_ref_str) = resolve_header_ref(
            &client,
            &relay,
            &resource,
            header.as_deref(),
            header_coordinate.as_deref(),
        )
        .await?;

        let event_tags =
            build_item_tags(header_ref_tag, &resource, &z_tag, &fields, d_tag.as_deref());
        let builder =
            EventBuilder::new(item_kind, content.as_deref().unwrap_or("")).tags(event_tags);

        match client.send_event_builder(builder).await {
            Ok(output) => {
                let event_id = output.val.to_hex();
                let result = json!({
                    "event_id": event_id, "kind": item_kind.as_u16(),
                    "header_ref": header_ref_str, "resource": resource,
                });
                let header_flag = if header_coordinate.is_some() {
                    format!("--header-coordinate \"{header_ref_str}\"")
                } else {
                    format!("--header {}", header.as_deref().unwrap_or(""))
                };
                let actions = vec![
                    NextAction::new(
                        format!("wokhei inspect --relay {relay} {event_id}"),
                        "Inspect the created item",
                    ),
                    NextAction::new(
                        format!("wokhei add-item --relay {relay} {header_flag} --resource <url>"),
                        "Add another item to this list",
                    ),
                    NextAction::new(
                        format!(
                            "wokhei list-items --relay {relay} {}",
                            header.as_deref().unwrap_or(&header_ref_str)
                        ),
                        "List all items in this list",
                    ),
                ];
                Ok(CommandOutput::new(result).next_actions(actions))
            }
            Err(e) => Err(CommandError::from(AppError::RelayRejected {
                reason: e.to_string(),
            })),
        }
    }
    .await;

    client.disconnect().await;
    result
}
