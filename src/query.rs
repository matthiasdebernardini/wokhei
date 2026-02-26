use nostr_sdk::prelude::*;
use serde_json::json;
use std::time::Duration;

use agcli::{CommandError, CommandOutput, NextAction};

use crate::error::AppError;

fn event_to_json(event: &Event) -> serde_json::Value {
    let tags: Vec<Vec<String>> = event
        .tags
        .iter()
        .map(|t| t.as_slice().iter().map(ToString::to_string).collect())
        .collect();

    let mut obj = json!({
        "event_id": event.id.to_hex(),
        "kind": event.kind.as_u16(),
        "pubkey": event.pubkey.to_hex(),
        "created_at": event.created_at.as_secs(),
        "tags": tags,
        "content": event.content,
        "sig": event.sig.to_string(),
    });

    // Extract common DCoSL fields from tags for convenience
    for tag in event.tags.iter() {
        let parts = tag.as_slice();
        if parts.len() >= 2 {
            let key = parts[0].as_str();
            match key {
                "names" => {
                    let names: Vec<&str> = parts[1..].iter().map(String::as_str).collect();
                    obj["name"] = json!(names.first().unwrap_or(&""));
                    if names.len() > 1 {
                        obj["aliases"] = json!(&names[1..]);
                    }
                }
                "title" => {
                    obj["title"] = json!(parts[1].as_str());
                }
                "description" => {
                    obj["description"] = json!(parts[1].as_str());
                }
                "d" => {
                    let pubkey_hex = event.pubkey.to_hex();
                    let d_val = parts[1].as_str();
                    obj["coordinate"] =
                        json!(format!("{}:{}:{}", event.kind.as_u16(), pubkey_hex, d_val));
                }
                _ => {}
            }
        }
    }

    obj
}

async fn connect_client(relay: &str) -> Result<Client, AppError> {
    let client = Client::default();
    client
        .add_relay(relay)
        .await
        .map_err(|_| AppError::RelayUnreachable {
            url: relay.to_string(),
        })?;
    client.connect().await;
    Ok(client)
}

pub async fn list_headers(
    relay: String,
    author: Option<String>,
    tag: Option<String>,
    name: Option<String>,
    limit: usize,
) -> Result<CommandOutput, CommandError> {
    let client = connect_client(&relay).await.map_err(CommandError::from)?;

    let mut filter = Filter::new()
        .kinds(vec![Kind::Custom(9998), Kind::Custom(39998)])
        .limit(limit);

    if let Some(ref author_hex) = author {
        let pk = PublicKey::parse(author_hex).map_err(|_| {
            CommandError::from(AppError::InvalidEventId {
                id: author_hex.clone(),
            })
        })?;
        filter = filter.author(pk);
    }

    if let Some(ref t) = tag {
        filter = filter.hashtag(t);
    }

    let events = client
        .fetch_events(filter, Duration::from_secs(10))
        .await
        .map_err(|_| CommandError::from(AppError::RelayUnreachable { url: relay.clone() }));

    // Ensure disconnect on all paths after connect
    let events = match events {
        Ok(ev) => {
            client.disconnect().await;
            ev
        }
        Err(e) => {
            client.disconnect().await;
            return Err(e);
        }
    };

    let mut headers: Vec<serde_json::Value> = events.iter().map(event_to_json).collect();

    // Client-side name substring filter (Nostr can't do substring search)
    if let Some(ref name_filter) = name {
        let lower = name_filter.to_lowercase();
        headers.retain(|h| {
            h["name"]
                .as_str()
                .is_some_and(|n| n.to_lowercase().contains(&lower))
        });
    }

    if headers.is_empty() {
        return Err(
            CommandError::from(AppError::NoResults).next_actions(vec![NextAction::new(
                format!("wokhei create-header --relay={relay} --name=<name> --title=<title>"),
                "Create a new list header",
            )]),
        );
    }

    let first_id = headers[0]["event_id"].as_str().unwrap_or("").to_string();
    let actions = vec![
        NextAction::new(
            format!("wokhei list-items --relay={relay} {first_id}"),
            "List items for the first header",
        ),
        NextAction::new(
            format!("wokhei create-header --relay={relay} --name=<name> --title=<title>"),
            "Create a new list header",
        ),
    ];

    Ok(CommandOutput::new(json!({
        "count": headers.len(),
        "headers": headers
    }))
    .next_actions(actions))
}

pub async fn list_items(
    relay: String,
    header_id: Option<String>,
    header_coordinate: Option<String>,
    limit: usize,
) -> Result<CommandOutput, CommandError> {
    let client = connect_client(&relay).await.map_err(CommandError::from)?;

    let all_items = if let Some(ref coord_str) = header_coordinate {
        fetch_items_by_coordinate(&client, &relay, coord_str, limit).await?
    } else {
        let id_str = header_id.as_deref().unwrap_or("");
        let event_id = EventId::parse(id_str).map_err(|_| {
            CommandError::from(AppError::InvalidEventId {
                id: id_str.to_string(),
            })
        })?;
        fetch_all_items(&client, &relay, event_id, limit).await
    };

    client.disconnect().await;

    let header_ref = header_coordinate
        .as_deref()
        .or(header_id.as_deref())
        .unwrap_or("");

    if all_items.is_empty() {
        return Err(
            CommandError::from(AppError::NoResults).next_actions(vec![NextAction::new(
                format!("wokhei add-item --relay={relay} --header={header_ref} --resource=<url>"),
                "Add an item to this list",
            )]),
        );
    }

    let actions = vec![
        NextAction::new(
            format!("wokhei add-item --relay={relay} --header={header_ref} --resource=<url>"),
            "Add another item to this list",
        ),
        NextAction::new(
            format!(
                "wokhei inspect --relay={relay} {}",
                all_items[0]["event_id"].as_str().unwrap_or("")
            ),
            "Inspect the first item",
        ),
    ];

    Ok(CommandOutput::new(json!({
        "count": all_items.len(),
        "header_ref": header_ref,
        "items": all_items,
    }))
    .next_actions(actions))
}

async fn fetch_items_by_coordinate(
    client: &Client,
    _relay: &str,
    coord_str: &str,
    limit: usize,
) -> Result<Vec<serde_json::Value>, CommandError> {
    let parts: Vec<&str> = coord_str.splitn(3, ':').collect();
    if parts.len() != 3 {
        return Err(CommandError::from(AppError::InvalidCoordinate {
            input: coord_str.to_string(),
        }));
    }
    let kind_num: u16 = parts[0].parse().map_err(|_| {
        CommandError::from(AppError::InvalidCoordinate {
            input: coord_str.to_string(),
        })
    })?;
    let pubkey = PublicKey::parse(parts[1]).map_err(|_| {
        CommandError::from(AppError::InvalidCoordinate {
            input: coord_str.to_string(),
        })
    })?;
    let d_tag = parts[2];

    let coord = Coordinate::new(Kind::Custom(kind_num), pubkey).identifier(d_tag);
    let filter = Filter::new()
        .kinds(vec![Kind::Custom(9999), Kind::Custom(39999)])
        .custom_tag(SingleLetterTag::lowercase(Alphabet::A), coord.to_string())
        .limit(limit);

    let events = client
        .fetch_events(filter, Duration::from_secs(10))
        .await
        .unwrap_or_default();

    Ok(events.iter().map(event_to_json).collect())
}

async fn fetch_all_items(
    client: &Client,
    relay: &str,
    event_id: EventId,
    limit: usize,
) -> Vec<serde_json::Value> {
    // Fetch items that reference this header via e-tag
    let filter = Filter::new()
        .kinds(vec![Kind::Custom(9999), Kind::Custom(39999)])
        .event(event_id)
        .limit(limit);

    let events = client
        .fetch_events(filter, Duration::from_secs(10))
        .await
        .unwrap_or_default();

    // Also try fetching by coordinate reference (for addressable headers)
    let header_filter = Filter::new().id(event_id).limit(1);
    let header_events = client
        .fetch_events(header_filter, Duration::from_secs(5))
        .await
        .unwrap_or_default();

    let mut all_items: Vec<serde_json::Value> = events.iter().map(event_to_json).collect();

    // If header is addressable, also search by coordinate
    if let Some(header_event) = header_events.into_iter().next() {
        if header_event.kind == Kind::Custom(39998) {
            fetch_coordinate_items(client, relay, &header_event, limit, &mut all_items).await;
        }
    }

    all_items
}

async fn fetch_coordinate_items(
    client: &Client,
    _relay: &str,
    header_event: &Event,
    limit: usize,
    all_items: &mut Vec<serde_json::Value>,
) {
    let d_val = header_event.tags.iter().find_map(|t| {
        let parts = t.as_slice();
        if parts.first().map(String::as_str) == Some("d") {
            parts.get(1).cloned()
        } else {
            None
        }
    });

    let Some(d_val) = d_val else { return };

    let coord = Coordinate::new(Kind::Custom(39998), header_event.pubkey).identifier(&d_val);
    let coord_filter = Filter::new()
        .kinds(vec![Kind::Custom(9999), Kind::Custom(39999)])
        .custom_tag(SingleLetterTag::lowercase(Alphabet::A), coord.to_string())
        .limit(limit);

    let Ok(coord_events) = client
        .fetch_events(coord_filter, Duration::from_secs(10))
        .await
    else {
        return;
    };

    // Deduplicate by event_id
    let existing_ids: std::collections::HashSet<String> = all_items
        .iter()
        .filter_map(|i| i["event_id"].as_str().map(String::from))
        .collect();

    for ev in coord_events.iter() {
        let id = ev.id.to_hex();
        if !existing_ids.contains(&id) {
            all_items.push(event_to_json(ev));
        }
    }
}

pub async fn inspect(relay: String, event_id_str: String) -> Result<CommandOutput, CommandError> {
    let event_id = EventId::parse(&event_id_str).map_err(|_| {
        CommandError::from(AppError::InvalidEventId {
            id: event_id_str.clone(),
        })
    })?;

    let client = connect_client(&relay).await.map_err(CommandError::from)?;

    let filter = Filter::new().id(event_id).limit(1);
    let events = client
        .fetch_events(filter, Duration::from_secs(10))
        .await
        .map_err(|_| CommandError::from(AppError::RelayUnreachable { url: relay.clone() }));

    let events = match events {
        Ok(ev) => {
            client.disconnect().await;
            ev
        }
        Err(e) => {
            client.disconnect().await;
            return Err(e);
        }
    };

    let event = events.into_iter().next().ok_or_else(|| {
        CommandError::from(AppError::HeaderNotFound {
            event_id: event_id_str.clone(),
        })
        .next_actions(vec![NextAction::new(
            format!("wokhei list-headers --relay={relay}"),
            "List available headers",
        )])
    })?;

    let ev_json = event_to_json(&event);
    let kind = event.kind.as_u16();

    let mut actions = vec![];

    // Context-sensitive next actions
    if kind == 9998 || kind == 39998 {
        actions.push(NextAction::new(
            format!("wokhei list-items --relay={relay} {event_id_str}"),
            "List items in this list",
        ));
        actions.push(NextAction::new(
            format!("wokhei add-item --relay={relay} --header={event_id_str} --resource=<url>"),
            "Add an item to this list",
        ));
    }

    actions.push(NextAction::new(
        format!("wokhei delete --relay={relay} {event_id_str}"),
        "Delete this event (NIP-09 request)",
    ));

    Ok(CommandOutput::new(ev_json).next_actions(actions))
}
