use std::collections::HashSet;
use std::time::Duration;

use nostr_sdk::prelude::*;
use serde_json::json;

use agcli::{CommandError, CommandOutput, NextAction};

use crate::error::AppError;

const QUERY_TIMEOUT: Duration = Duration::from_secs(10);
const FETCH_PAGE_SIZE: usize = 500;

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
                    obj["name"] = json!(parts[1].as_str());
                    if parts.len() >= 3 {
                        obj["plural_name"] = json!(parts[2].as_str());
                        obj["names"] = json!([parts[1].as_str(), parts[2].as_str()]);
                    }
                }
                "titles" => {
                    obj["title"] = json!(parts[1].as_str());
                    if parts.len() >= 3 {
                        obj["plural_title"] = json!(parts[2].as_str());
                        obj["titles"] = json!([parts[1].as_str(), parts[2].as_str()]);
                    }
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

fn sort_event_json_desc(events: &mut [serde_json::Value]) {
    events.sort_by(|a, b| {
        let a_created = a["created_at"].as_u64().unwrap_or(0);
        let b_created = b["created_at"].as_u64().unwrap_or(0);
        let a_id = a["event_id"].as_str().unwrap_or("");
        let b_id = b["event_id"].as_str().unwrap_or("");

        b_created.cmp(&a_created).then_with(|| a_id.cmp(b_id))
    });
}

fn sort_events_desc(events: &mut [Event]) {
    events.sort_by(|a, b| {
        b.created_at
            .as_secs()
            .cmp(&a.created_at.as_secs())
            .then_with(|| a.id.to_hex().cmp(&b.id.to_hex()))
    });
}

fn paginate<T: Clone>(values: &[T], offset: usize, limit: usize) -> Vec<T> {
    if offset >= values.len() || limit == 0 {
        return Vec::new();
    }

    let end = offset.saturating_add(limit).min(values.len());
    values[offset..end].to_vec()
}

fn header_query_command(
    relay: &str,
    author: Option<&String>,
    tag: Option<&String>,
    name: Option<&String>,
    offset: usize,
    limit: usize,
) -> String {
    let mut parts = vec![
        "wokhei list-headers".to_string(),
        format!("--relay={relay}"),
    ];

    if let Some(author) = author {
        parts.push(format!("--author={author}"));
    }
    if let Some(tag) = tag {
        parts.push(format!("--tag={tag}"));
    }
    if let Some(name) = name {
        parts.push(format!("--name={name}"));
    }

    parts.push(format!("--offset={offset}"));
    parts.push(format!("--limit={limit}"));

    parts.join(" ")
}

fn item_add_command(relay: &str, header_ref: &str, coordinate_mode: bool) -> String {
    if coordinate_mode {
        format!("wokhei add-item --relay={relay} --header-coordinate={header_ref} --resource=<url>")
    } else {
        format!("wokhei add-item --relay={relay} --header={header_ref} --resource=<url>")
    }
}

fn header_d_tag(header_event: &Event) -> Option<String> {
    header_event.tags.iter().find_map(|t| {
        let parts = t.as_slice();
        if parts.first().map(String::as_str) == Some("d") {
            parts.get(1).cloned()
        } else {
            None
        }
    })
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

fn build_header_filter(
    author: Option<&String>,
    tag: Option<&String>,
) -> Result<Filter, CommandError> {
    let mut filter = Filter::new().kinds(vec![Kind::Custom(9998), Kind::Custom(39998)]);

    if let Some(author_hex) = author {
        let pk = PublicKey::parse(author_hex).map_err(|_| {
            CommandError::from(AppError::InvalidEventId {
                id: author_hex.clone(),
            })
        })?;
        filter = filter.author(pk);
    }

    if let Some(t) = tag {
        filter = filter.hashtag(t);
    }

    Ok(filter)
}

async fn fetch_all_events(
    client: &Client,
    relay: &str,
    base_filter: Filter,
) -> Result<Vec<Event>, CommandError> {
    let mut all_events: Vec<Event> = Vec::new();
    let mut seen_ids: HashSet<String> = HashSet::new();
    let mut until_secs: Option<u64> = None;

    loop {
        let mut filter = base_filter.clone().limit(FETCH_PAGE_SIZE);
        if let Some(secs) = until_secs {
            filter = filter.until(Timestamp::from_secs(secs));
        }

        let batch = client
            .fetch_events(filter, QUERY_TIMEOUT)
            .await
            .map_err(|_| {
                CommandError::from(AppError::RelayUnreachable {
                    url: relay.to_string(),
                })
            })?;

        if batch.is_empty() {
            break;
        }

        let mut oldest_created_at = u64::MAX;
        for event in batch.iter() {
            oldest_created_at = oldest_created_at.min(event.created_at.as_secs());
            let event_id = event.id.to_hex();
            if seen_ids.insert(event_id) {
                all_events.push(event.clone());
            }
        }

        if batch.len() < FETCH_PAGE_SIZE || oldest_created_at == 0 {
            break;
        }

        let next_until = oldest_created_at.saturating_sub(1);
        if until_secs == Some(next_until) {
            break;
        }
        until_secs = Some(next_until);
    }

    Ok(all_events)
}

async fn count_filter(client: &Client, relay: &str, filter: Filter) -> Result<usize, CommandError> {
    let relay_handle = client.relay(relay).await.map_err(|_| {
        CommandError::from(AppError::RelayUnreachable {
            url: relay.to_string(),
        })
    })?;

    if let Ok(count) = relay_handle
        .count_events(filter.clone(), QUERY_TIMEOUT)
        .await
    {
        return Ok(count);
    }

    let events = fetch_all_events(client, relay, filter).await?;
    Ok(events.len())
}

pub async fn list_headers(
    relay: String,
    author: Option<String>,
    tag: Option<String>,
    name: Option<String>,
    offset: usize,
    limit: usize,
) -> Result<CommandOutput, CommandError> {
    let client = connect_client(&relay).await.map_err(CommandError::from)?;

    let headers_result = async {
        let filter = build_header_filter(author.as_ref(), tag.as_ref())?;
        let events = fetch_all_events(&client, &relay, filter).await?;

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

        sort_event_json_desc(&mut headers);

        let total = headers.len();

        if total == 0 && offset == 0 {
            return Err(CommandError::from(AppError::NoResults).next_actions(vec![
                NextAction::new(
                    format!(
                        "wokhei create-header --relay={relay} --name=<singular> --plural=<plural>"
                    ),
                    "Create a new list header",
                ),
            ]));
        }

        let page_headers = paginate(&headers, offset, limit);
        let has_more = limit > 0 && offset.saturating_add(limit) < total;
        let page_count = page_headers.len();

        let mut actions = Vec::new();

        if let Some(first) = page_headers.first() {
            let first_id = first["event_id"].as_str().unwrap_or("");
            if !first_id.is_empty() {
                actions.push(NextAction::new(
                    format!("wokhei list-items --relay={relay} {first_id}"),
                    "List items for the first header in this page",
                ));
            }
        }

        if offset > 0 {
            let step = limit.max(1);
            let prev_offset = offset.saturating_sub(step);
            actions.push(NextAction::new(
                header_query_command(
                    &relay,
                    author.as_ref(),
                    tag.as_ref(),
                    name.as_ref(),
                    prev_offset,
                    limit,
                ),
                "Go to the previous page",
            ));
        }

        if has_more {
            actions.push(NextAction::new(
                header_query_command(
                    &relay,
                    author.as_ref(),
                    tag.as_ref(),
                    name.as_ref(),
                    offset.saturating_add(limit),
                    limit,
                ),
                "Go to the next page",
            ));
        }

        if total > 0 && page_count == 0 {
            let step = limit.max(1);
            let last_offset = ((total - 1) / step) * step;
            actions.push(NextAction::new(
                header_query_command(
                    &relay,
                    author.as_ref(),
                    tag.as_ref(),
                    name.as_ref(),
                    last_offset,
                    limit,
                ),
                "Jump to the last non-empty page",
            ));
        }

        actions.push(NextAction::new(
            format!("wokhei create-header --relay={relay} --name=<singular> --plural=<plural>"),
            "Create a new list header",
        ));

        Ok(CommandOutput::new(json!({
            "total": total,
            "count": page_count,
            "offset": offset,
            "limit": limit,
            "has_more": has_more,
            "headers": page_headers,
        }))
        .next_actions(actions))
    }
    .await;

    client.disconnect().await;
    headers_result
}

pub async fn list_items(
    relay: String,
    header_id: Option<String>,
    header_coordinate: Option<String>,
    limit: usize,
) -> Result<CommandOutput, CommandError> {
    let client = connect_client(&relay).await.map_err(CommandError::from)?;

    let (all_items, header_ref, coordinate_mode) = if let Some(ref coord_str) = header_coordinate {
        let normalized_ref = normalize_coordinate_ref(coord_str)?;
        let items = fetch_items_by_z(&client, &normalized_ref, limit).await;
        (items, normalized_ref, true)
    } else {
        let id_str = header_id.as_deref().unwrap_or("");
        let event_id = EventId::parse(id_str).map_err(|_| {
            CommandError::from(AppError::InvalidEventId {
                id: id_str.to_string(),
            })
        })?;
        let header_event = fetch_header_event_by_id(&client, &relay, event_id).await?;
        let (resolved_ref, resolved_coordinate_mode) = z_ref_for_header_event(&header_event)?;
        let items = fetch_items_by_z(&client, &resolved_ref, limit).await;
        (items, resolved_ref, resolved_coordinate_mode)
    };

    client.disconnect().await;

    let add_item_cmd = item_add_command(&relay, &header_ref, coordinate_mode);

    if all_items.is_empty() {
        return Err(
            CommandError::from(AppError::NoResults).next_actions(vec![NextAction::new(
                add_item_cmd,
                "Add an item to this list",
            )]),
        );
    }

    let actions = vec![
        NextAction::new(add_item_cmd, "Add another item to this list"),
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

fn normalize_coordinate_ref(coord_str: &str) -> Result<String, CommandError> {
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
    if kind_num != 39998 {
        return Err(CommandError::from(AppError::InvalidCoordinate {
            input: coord_str.to_string(),
        }));
    }
    let pubkey = PublicKey::parse(parts[1]).map_err(|_| {
        CommandError::from(AppError::InvalidCoordinate {
            input: coord_str.to_string(),
        })
    })?;
    let d_tag = parts[2];

    Ok(format!("39998:{}:{}", pubkey.to_hex(), d_tag))
}

async fn fetch_header_event_by_id(
    client: &Client,
    relay: &str,
    event_id: EventId,
) -> Result<Event, CommandError> {
    let filter = Filter::new().id(event_id).limit(1);
    let header_events = client
        .fetch_events(filter, QUERY_TIMEOUT)
        .await
        .map_err(|_| {
            CommandError::from(AppError::RelayUnreachable {
                url: relay.to_string(),
            })
        })?;

    header_events.into_iter().next().ok_or_else(|| {
        CommandError::from(AppError::HeaderNotFound {
            event_id: event_id.to_hex(),
        })
    })
}

fn z_ref_for_header_event(header_event: &Event) -> Result<(String, bool), CommandError> {
    match header_event.kind {
        Kind::Custom(9998) => Ok((header_event.id.to_hex(), false)),
        Kind::Custom(39998) => {
            let d_val = header_d_tag(header_event)
                .ok_or_else(|| CommandError::from(AppError::HeaderMissingDTag))?;
            Ok((
                format!("39998:{}:{}", header_event.pubkey.to_hex(), d_val),
                true,
            ))
        }
        _ => Err(CommandError::new(
            "header reference must point to a list header (kind 9998 or 39998)",
            "INVALID_ARGS",
            "Provide a list header ID, or use --header-coordinate=<39998:pubkey:d-tag>",
        )),
    }
}

async fn fetch_items_by_z(client: &Client, z_ref: &str, limit: usize) -> Vec<serde_json::Value> {
    let filter = Filter::new()
        .kinds(vec![Kind::Custom(9999), Kind::Custom(39999)])
        .custom_tag(SingleLetterTag::lowercase(Alphabet::Z), z_ref.to_string())
        .limit(limit);

    let events = client
        .fetch_events(filter, QUERY_TIMEOUT)
        .await
        .unwrap_or_default();

    events.iter().map(event_to_json).collect()
}

async fn fetch_items_for_header_event(
    client: &Client,
    relay: &str,
    header_event: &Event,
) -> Result<Vec<Event>, CommandError> {
    let (z_ref, _) = z_ref_for_header_event(header_event)?;
    let z_filter = Filter::new()
        .kinds(vec![Kind::Custom(9999), Kind::Custom(39999)])
        .custom_tag(SingleLetterTag::lowercase(Alphabet::Z), z_ref);
    let mut items = fetch_all_events(client, relay, z_filter).await?;
    sort_events_desc(&mut items);
    Ok(items)
}

pub async fn count(relay: String) -> Result<CommandOutput, CommandError> {
    let client = connect_client(&relay).await.map_err(CommandError::from)?;

    let result = async {
        let headers_total = count_filter(
            &client,
            &relay,
            Filter::new().kinds(vec![Kind::Custom(9998), Kind::Custom(39998)]),
        )
        .await?;
        let headers_regular = count_filter(
            &client,
            &relay,
            Filter::new().kinds(vec![Kind::Custom(9998)]),
        )
        .await?;
        let headers_addressable = count_filter(
            &client,
            &relay,
            Filter::new().kinds(vec![Kind::Custom(39998)]),
        )
        .await?;

        let items_total = count_filter(
            &client,
            &relay,
            Filter::new().kinds(vec![Kind::Custom(9999), Kind::Custom(39999)]),
        )
        .await?;
        let items_regular = count_filter(
            &client,
            &relay,
            Filter::new().kinds(vec![Kind::Custom(9999)]),
        )
        .await?;
        let items_addressable = count_filter(
            &client,
            &relay,
            Filter::new().kinds(vec![Kind::Custom(39999)]),
        )
        .await?;

        let actions = vec![
            NextAction::new(
                format!("wokhei list-headers --relay={relay}"),
                "List headers on this relay",
            ),
            NextAction::new(
                format!("wokhei export --relay={relay}"),
                "Export full header and item backup",
            ),
        ];

        Ok(CommandOutput::new(json!({
            "relay": relay,
            "headers": {
                "total": headers_total,
                "regular": headers_regular,
                "addressable": headers_addressable,
            },
            "items": {
                "total": items_total,
                "regular": items_regular,
                "addressable": items_addressable,
            }
        }))
        .next_actions(actions))
    }
    .await;

    client.disconnect().await;
    result
}

pub async fn export(relay: String) -> Result<CommandOutput, CommandError> {
    let client = connect_client(&relay).await.map_err(CommandError::from)?;

    let result = async {
        let header_filter = Filter::new().kinds(vec![Kind::Custom(9998), Kind::Custom(39998)]);
        let mut header_events = fetch_all_events(&client, &relay, header_filter).await?;
        sort_events_desc(&mut header_events);

        let mut exported_headers = Vec::with_capacity(header_events.len());
        let mut total_items = 0usize;

        for header_event in &header_events {
            let items = fetch_items_for_header_event(&client, &relay, header_event).await?;
            total_items = total_items.saturating_add(items.len());

            let item_json: Vec<serde_json::Value> = items.iter().map(event_to_json).collect();

            exported_headers.push(json!({
                "header": event_to_json(header_event),
                "items_count": item_json.len(),
                "items": item_json,
            }));
        }

        let actions = vec![
            NextAction::new(
                format!("wokhei count --relay={relay}"),
                "Get quick relay counts",
            ),
            NextAction::new(
                format!("wokhei list-headers --relay={relay}"),
                "Inspect exported headers via paged query",
            ),
        ];

        Ok(CommandOutput::new(json!({
            "relay": relay,
            "exported_at": Timestamp::now().as_secs(),
            "counts": {
                "headers": exported_headers.len(),
                "items": total_items,
            },
            "headers": exported_headers,
        }))
        .next_actions(actions))
    }
    .await;

    client.disconnect().await;
    result
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
        .fetch_events(filter, QUERY_TIMEOUT)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_event(kind: Kind, content: &str, tags: Vec<Tag>) -> Event {
        let keys = Keys::generate();
        EventBuilder::new(kind, content)
            .tags(tags)
            .sign_with_keys(&keys)
            .unwrap()
    }

    #[test]
    fn event_to_json_basic_fields() {
        let event = make_event(Kind::Custom(9998), "hello", vec![]);
        let j = event_to_json(&event);
        assert!(j["event_id"].is_string());
        assert_eq!(j["kind"], 9998);
        assert!(j["pubkey"].is_string());
        assert!(j["created_at"].is_number());
        assert!(j["sig"].is_string());
        assert_eq!(j["content"], "hello");
        assert!(j["tags"].is_array());
    }

    #[test]
    fn event_to_json_names_tag_extracts_singular_and_plural() {
        let tags = vec![Tag::custom(TagKind::custom("names"), ["mylist", "mylists"])];
        let event = make_event(Kind::Custom(9998), "", tags);
        let j = event_to_json(&event);
        assert_eq!(j["name"], "mylist");
        assert_eq!(j["plural_name"], "mylists");
        assert_eq!(j["names"], json!(["mylist", "mylists"]));
    }

    #[test]
    fn event_to_json_single_name_sets_only_singular_name() {
        let tags = vec![Tag::custom(TagKind::custom("names"), ["mylist"])];
        let event = make_event(Kind::Custom(9998), "", tags);
        let j = event_to_json(&event);
        assert_eq!(j["name"], "mylist");
        assert!(j.get("plural_name").is_none());
        assert!(j.get("names").is_none());
    }

    #[test]
    fn event_to_json_titles_tag_extracted() {
        let tags = vec![Tag::custom(
            TagKind::custom("titles"),
            ["My List", "My Lists"],
        )];
        let event = make_event(Kind::Custom(9998), "", tags);
        let j = event_to_json(&event);
        assert_eq!(j["title"], "My List");
        assert_eq!(j["plural_title"], "My Lists");
        assert_eq!(j["titles"], json!(["My List", "My Lists"]));
    }

    #[test]
    fn event_to_json_description_extracted() {
        let tags = vec![Tag::custom(
            TagKind::custom("description"),
            ["A description"],
        )];
        let event = make_event(Kind::Custom(9998), "", tags);
        let j = event_to_json(&event);
        assert_eq!(j["description"], "A description");
    }

    #[test]
    fn event_to_json_d_tag_creates_coordinate() {
        let keys = Keys::generate();
        let tags = vec![Tag::identifier("my-list")];
        let event = EventBuilder::new(Kind::Custom(39998), "")
            .tags(tags)
            .sign_with_keys(&keys)
            .unwrap();
        let j = event_to_json(&event);
        let coord = j["coordinate"].as_str().unwrap();
        assert!(coord.starts_with("39998:"));
        assert!(coord.ends_with(":my-list"));
        assert!(coord.contains(&keys.public_key().to_hex()));
    }

    #[test]
    fn event_to_json_unknown_tags_dont_pollute_top_level() {
        let tags = vec![Tag::custom(TagKind::custom("weird"), ["val"])];
        let event = make_event(Kind::Custom(9998), "", tags);
        let j = event_to_json(&event);
        assert!(j.get("weird").is_none());
    }

    #[test]
    fn event_to_json_content_preserved() {
        let event = make_event(Kind::Custom(9999), r#"{"key":"val"}"#, vec![]);
        let j = event_to_json(&event);
        assert_eq!(j["content"], r#"{"key":"val"}"#);
    }

    #[test]
    fn event_to_json_tags_array_structure() {
        let tags = vec![
            Tag::custom(TagKind::custom("r"), ["https://example.com"]),
            Tag::custom(TagKind::custom("z"), ["39998:deadbeef:my-list"]),
        ];
        let event = make_event(Kind::Custom(9999), "", tags);
        let j = event_to_json(&event);
        let tags_arr = j["tags"].as_array().unwrap();
        assert_eq!(tags_arr.len(), 2);
        assert_eq!(tags_arr[0][0], "r");
        assert_eq!(tags_arr[0][1], "https://example.com");
        assert_eq!(tags_arr[1][0], "z");
        assert_eq!(tags_arr[1][1], "39998:deadbeef:my-list");
    }

    #[test]
    fn paginate_returns_expected_window() {
        let values = vec![1, 2, 3, 4, 5];
        assert_eq!(paginate(&values, 1, 2), vec![2, 3]);
    }

    #[test]
    fn paginate_returns_empty_when_offset_out_of_range() {
        let values = vec![1, 2, 3];
        assert!(paginate(&values, 3, 10).is_empty());
    }

    #[test]
    fn paginate_returns_empty_when_limit_zero() {
        let values = vec![1, 2, 3];
        assert!(paginate(&values, 0, 0).is_empty());
    }

    #[test]
    fn sort_event_json_orders_by_created_at_desc_then_id() {
        let mut rows = vec![
            json!({"event_id": "b", "created_at": 100}),
            json!({"event_id": "a", "created_at": 100}),
            json!({"event_id": "c", "created_at": 120}),
        ];

        sort_event_json_desc(&mut rows);

        assert_eq!(rows[0]["event_id"], "c");
        assert_eq!(rows[1]["event_id"], "a");
        assert_eq!(rows[2]["event_id"], "b");
    }
}
