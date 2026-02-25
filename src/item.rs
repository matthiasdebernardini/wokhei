use nostr_sdk::prelude::*;
use serde_json::json;
use std::time::Duration;

use crate::error::AppError;
use crate::keys::load_keys;
use crate::response::{NextAction, Response};

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
) -> Result<(Tag, String), Response> {
    let cmd = "add-item";

    if let Some(coord_str) = header_coordinate {
        match parse_coordinate_str(coord_str) {
            Ok((kind_num, pubkey, d_val)) => {
                let coord = Coordinate::new(Kind::Custom(kind_num), pubkey).identifier(&d_val);
                let tag = Tag::coordinate(coord, None);
                Ok((tag, coord_str.to_string()))
            }
            Err(e) => {
                client.disconnect().await;
                Err(Response::error(cmd, &e, vec![]))
            }
        }
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
) -> Result<(Tag, String), Response> {
    let cmd = "add-item";

    let Ok(event_id) = EventId::parse(header_id_str) else {
        client.disconnect().await;
        return Err(Response::error(
            cmd,
            &AppError::InvalidEventId {
                id: header_id_str.to_string(),
            },
            vec![],
        ));
    };

    let filter = Filter::new().id(event_id).limit(1);
    let Ok(events) = client.fetch_events(filter, Duration::from_secs(10)).await else {
        client.disconnect().await;
        return Err(Response::error(
            cmd,
            &AppError::RelayUnreachable {
                url: relay.to_string(),
            },
            vec![],
        ));
    };

    let Some(header_event) = events.into_iter().next() else {
        client.disconnect().await;
        return Err(Response::error(
            cmd,
            &AppError::HeaderNotFound {
                event_id: header_id_str.to_string(),
            },
            vec![NextAction::simple(
                &format!(
                    "wokhei add-item --relay {relay} --header-coordinate <kind:pubkey:d-tag> --resource \"{resource}\""
                ),
                "Use coordinate reference instead (cross-relay)",
            )],
        ));
    };

    if header_event.kind == Kind::Custom(39998) {
        let d_val = header_event.tags.iter().find_map(|t| {
            let tag_vec = t.as_slice();
            if tag_vec.first().map(String::as_str) == Some("d") {
                tag_vec.get(1).cloned()
            } else {
                None
            }
        });

        let Some(d) = d_val else {
            client.disconnect().await;
            return Err(Response::error(cmd, &AppError::HeaderMissingDTag, vec![]));
        };

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

#[allow(clippy::result_large_err)]
fn validate_item_params(params: &ItemParams) -> Result<(), Response> {
    let cmd = "add-item";
    if params.header.is_none() && params.header_coordinate.is_none() {
        return Err(Response::error(
            cmd,
            &AppError::Io {
                reason: "Specify --header <event-id> or --header-coordinate <kind:pubkey:d-tag>"
                    .to_string(),
            },
            vec![],
        ));
    }
    if params.addressable && params.d_tag.is_none() {
        return Err(Response::error(
            cmd,
            &AppError::Io {
                reason: "--addressable requires --d-tag <identifier>".to_string(),
            },
            vec![],
        ));
    }
    Ok(())
}

pub async fn add_item(params: ItemParams) -> Response {
    let cmd = "add-item";

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

    if let Err(resp) = validate_item_params(&params) {
        return resp;
    }

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
        return Response::error(
            cmd,
            &AppError::RelayUnreachable { url: relay.clone() },
            vec![],
        );
    }
    client.connect().await;

    let (header_ref_tag, header_ref_str) = match resolve_header_ref(
        &client,
        &relay,
        &resource,
        header.as_deref(),
        header_coordinate.as_deref(),
    )
    .await
    {
        Ok(r) => r,
        Err(resp) => return resp,
    };

    let event_tags = build_item_tags(header_ref_tag, &resource, &z_tag, &fields, d_tag.as_deref());
    let builder = EventBuilder::new(item_kind, content.as_deref().unwrap_or("")).tags(event_tags);

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
                NextAction::simple(
                    &format!("wokhei inspect --relay {relay} {event_id}"),
                    "Inspect the created item",
                ),
                NextAction::simple(
                    &format!("wokhei add-item --relay {relay} {header_flag} --resource <url>"),
                    "Add another item to this list",
                ),
                NextAction::simple(
                    &format!(
                        "wokhei list-items --relay {relay} {}",
                        header.as_deref().unwrap_or(&header_ref_str)
                    ),
                    "List all items in this list",
                ),
            ];
            client.disconnect().await;
            Response::success(cmd, result, actions)
        }
        Err(e) => {
            client.disconnect().await;
            Response::error(
                cmd,
                &AppError::RelayRejected {
                    reason: e.to_string(),
                },
                vec![],
            )
        }
    }
}
