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
    pub addressable: bool,
    pub d_tag: Option<String>,
}

async fn resolve_header_ref(
    client: &Client,
    relay: &str,
    resource: &str,
    header: Option<&str>,
    header_coordinate: Option<&str>,
) -> Result<String, CommandError> {
    if let Some(coord_str) = header_coordinate {
        let (kind_num, pubkey, d_val) =
            parse_coordinate_str(coord_str).map_err(CommandError::from)?;
        if kind_num != 39998 {
            return Err(CommandError::from(AppError::InvalidCoordinate {
                input: coord_str.to_string(),
            }));
        }
        Ok(format!("39998:{}:{d_val}", pubkey.to_hex()))
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
) -> Result<String, CommandError> {
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
                "wokhei add-item --relay={relay} --header-coordinate=<kind:pubkey:d-tag> --resource=\"{resource}\""
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
        Ok(format!("39998:{}:{d}", header_event.pubkey.to_hex()))
    } else if header_event.kind == Kind::Custom(9998) {
        Ok(event_id.to_hex())
    } else {
        Err(CommandError::new(
            "--header must reference a list header event (kind 9998 or 39998)",
            "INVALID_ARGS",
            "Provide a header event ID, or use --header-coordinate=<39998:pubkey:d-tag>",
        ))
    }
}

fn build_item_tags(
    parent_z_ref: &str,
    resource: &str,
    fields: &[String],
    d_tag: Option<&str>,
) -> Vec<Tag> {
    let mut event_tags: Vec<Tag> = Vec::new();
    event_tags.push(Tag::custom(TagKind::custom("z"), [parent_z_ref]));
    event_tags.push(Tag::custom(TagKind::custom("r"), [resource]));
    event_tags.push(Tag::custom(TagKind::custom("client"), ["wokhei"]));

    event_tags.extend(fields.iter().filter_map(|field| {
        field
            .split_once('=')
            .map(|(key, val)| Tag::custom(TagKind::custom(key), [val]))
    }));

    if let Some(d) = d_tag {
        event_tags.push(Tag::identifier(d));
    }

    event_tags
}

fn validate_item_params(params: &ItemParams) -> Result<(), CommandError> {
    if params.header.is_none() && params.header_coordinate.is_none() {
        return Err(CommandError::new(
            "Specify --header=<event-id> or --header-coordinate=<kind:pubkey:d-tag>",
            "MISSING_ARG",
            "Use --header with an event ID, or --header-coordinate with kind:pubkey:d-tag",
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
        let parent_z_ref = resolve_header_ref(
            &client,
            &relay,
            &resource,
            header.as_deref(),
            header_coordinate.as_deref(),
        )
        .await?;

        let d_tag = if addressable && d_tag.is_none() {
            Some(crate::dtag::item_dtag(&parent_z_ref, &resource))
        } else {
            d_tag
        };

        let event_tags = build_item_tags(&parent_z_ref, &resource, &fields, d_tag.as_deref());
        let builder =
            EventBuilder::new(item_kind, content.as_deref().unwrap_or("")).tags(event_tags);

        match client.send_event_builder(builder).await {
            Ok(output) => {
                let event_id = output.val.to_hex();
                let mut result = json!({
                    "event_id": event_id, "kind": item_kind.as_u16(),
                    "header_ref": parent_z_ref, "resource": resource,
                });
                if let Some(ref d) = d_tag {
                    result["d_tag"] = json!(d);
                }
                let coordinate_mode =
                    header_coordinate.is_some() || parent_z_ref.starts_with("39998:");
                let header_flag = if coordinate_mode {
                    format!("--header-coordinate=\"{parent_z_ref}\"")
                } else {
                    format!("--header={}", header.as_deref().unwrap_or(&parent_z_ref))
                };
                let list_items_cmd = if coordinate_mode {
                    format!(
                        "wokhei list-items --relay={relay} --header-coordinate=\"{parent_z_ref}\""
                    )
                } else {
                    format!(
                        "wokhei list-items --relay={relay} {}",
                        header.as_deref().unwrap_or(&parent_z_ref)
                    )
                };
                let actions = vec![
                    NextAction::new(
                        format!("wokhei inspect --relay={relay} {event_id}"),
                        "Inspect the created item",
                    ),
                    NextAction::new(
                        format!("wokhei add-item --relay={relay} {header_flag} --resource=<url>"),
                        "Add another item to this list",
                    ),
                    NextAction::new(list_items_cmd, "List all items in this list"),
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

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // parse_coordinate_str
    // -----------------------------------------------------------------------

    fn test_pubkey_hex() -> String {
        Keys::generate().public_key().to_hex()
    }

    #[test]
    fn parse_coordinate_valid() {
        let pk = test_pubkey_hex();
        let input = format!("39998:{pk}:my-list");
        let (kind, pubkey, d_tag) = parse_coordinate_str(&input).unwrap();
        assert_eq!(kind, 39998);
        assert_eq!(pubkey.to_hex(), pk);
        assert_eq!(d_tag, "my-list");
    }

    #[test]
    fn parse_coordinate_too_few_parts() {
        let err = parse_coordinate_str("39998:abc").unwrap_err();
        assert_eq!(err.code(), "INVALID_COORDINATE");
    }

    #[test]
    fn parse_coordinate_single_part() {
        let err = parse_coordinate_str("just-one").unwrap_err();
        assert_eq!(err.code(), "INVALID_COORDINATE");
    }

    #[test]
    fn parse_coordinate_invalid_kind() {
        let pk = test_pubkey_hex();
        let input = format!("notnum:{pk}:d");
        let err = parse_coordinate_str(&input).unwrap_err();
        assert_eq!(err.code(), "INVALID_COORDINATE");
    }

    #[test]
    fn parse_coordinate_invalid_pubkey() {
        let err = parse_coordinate_str("39998:not-a-pubkey:d").unwrap_err();
        assert_eq!(err.code(), "INVALID_COORDINATE");
    }

    #[test]
    fn parse_coordinate_d_tag_with_colons_preserved() {
        let pk = test_pubkey_hex();
        let input = format!("39998:{pk}:d:tag:with:colons");
        let (_, _, d_tag) = parse_coordinate_str(&input).unwrap();
        assert_eq!(d_tag, "d:tag:with:colons");
    }

    #[test]
    fn parse_coordinate_empty_d_tag() {
        let pk = test_pubkey_hex();
        let input = format!("39998:{pk}:");
        let (_, _, d_tag) = parse_coordinate_str(&input).unwrap();
        assert_eq!(d_tag, "");
    }

    // -----------------------------------------------------------------------
    // validate_item_params
    // -----------------------------------------------------------------------

    fn base_params(header: Option<String>, header_coordinate: Option<String>) -> ItemParams {
        ItemParams {
            relay: "ws://localhost:7777".into(),
            header,
            header_coordinate,
            resource: "https://example.com".into(),
            content: None,
            fields: vec![],
            addressable: false,
            d_tag: None,
        }
    }

    #[test]
    fn validate_header_only_ok() {
        let p = base_params(Some("abc123".into()), None);
        assert!(validate_item_params(&p).is_ok());
    }

    #[test]
    fn validate_coordinate_only_ok() {
        let p = base_params(None, Some("39998:pk:d".into()));
        assert!(validate_item_params(&p).is_ok());
    }

    #[test]
    fn validate_neither_header_nor_coordinate_errors() {
        let p = base_params(None, None);
        let err = validate_item_params(&p).unwrap_err();
        assert_eq!(err.code, "MISSING_ARG");
    }

    #[test]
    fn validate_addressable_without_d_tag_ok() {
        let mut p = base_params(Some("abc".into()), None);
        p.addressable = true;
        assert!(validate_item_params(&p).is_ok());
    }

    #[test]
    fn validate_addressable_with_d_tag_ok() {
        let mut p = base_params(Some("abc".into()), None);
        p.addressable = true;
        p.d_tag = Some("my-id".into());
        assert!(validate_item_params(&p).is_ok());
    }

    #[test]
    fn validate_non_addressable_without_d_tag_ok() {
        let p = base_params(Some("abc".into()), None);
        assert!(validate_item_params(&p).is_ok());
    }

    // -----------------------------------------------------------------------
    // build_item_tags
    // -----------------------------------------------------------------------

    fn find_tag<'a>(tags: &'a [Tag], kind_str: &str) -> Option<&'a Tag> {
        tags.iter()
            .find(|t| t.as_slice().first().map(String::as_str) == Some(kind_str))
    }

    fn tag_values(tag: &Tag) -> Vec<String> {
        tag.as_slice().iter().map(ToString::to_string).collect()
    }

    #[test]
    fn build_item_tags_has_parent_z_ref() {
        let tags = build_item_tags("abc123", "https://example.com", &[], None);
        let z = find_tag(&tags, "z").expect("z tag missing");
        assert_eq!(tag_values(z), vec!["z", "abc123"]);
    }

    #[test]
    fn build_item_tags_has_resource() {
        let tags = build_item_tags("abc123", "https://example.com", &[], None);
        let r = find_tag(&tags, "r").expect("r tag missing");
        assert_eq!(tag_values(r), vec!["r", "https://example.com"]);
    }

    #[test]
    fn build_item_tags_has_client() {
        let tags = build_item_tags("abc123", "https://example.com", &[], None);
        let c = find_tag(&tags, "client").expect("client tag missing");
        assert_eq!(tag_values(c), vec!["client", "wokhei"]);
    }

    #[test]
    fn build_item_tags_does_not_emit_legacy_parent_tags() {
        let tags = build_item_tags("abc123", "https://example.com", &[], None);
        assert!(find_tag(&tags, "e").is_none());
        assert!(find_tag(&tags, "a").is_none());
    }

    #[test]
    fn build_item_tags_accepts_coordinate_parent_ref() {
        let coord = format!("39998:{}:my-list", test_pubkey_hex());
        let tags = build_item_tags(&coord, "https://example.com", &[], None);
        let z = find_tag(&tags, "z").expect("z tag missing");
        assert_eq!(tag_values(z), vec!["z".to_string(), coord]);
    }

    #[test]
    fn build_item_tags_fields_with_equals_become_tags() {
        let fields = vec!["color=red".to_string(), "size=large".to_string()];
        let tags = build_item_tags("abc123", "https://example.com", &fields, None);
        let color = find_tag(&tags, "color").expect("color tag missing");
        assert_eq!(tag_values(color), vec!["color", "red"]);
        let size = find_tag(&tags, "size").expect("size tag missing");
        assert_eq!(tag_values(size), vec!["size", "large"]);
    }

    #[test]
    fn build_item_tags_fields_without_equals_skipped() {
        let fields = vec!["no-equals-here".to_string()];
        let tags = build_item_tags("abc123", "https://example.com", &fields, None);
        // Should only have z, r, client — no extra tag
        assert_eq!(tags.len(), 3);
    }

    #[test]
    fn build_item_tags_d_tag_present() {
        let tags = build_item_tags("abc123", "https://example.com", &[], Some("my-item"));
        let d = find_tag(&tags, "d").expect("d tag missing");
        assert_eq!(tag_values(d), vec!["d", "my-item"]);
    }

    #[test]
    fn build_item_tags_d_tag_absent() {
        let tags = build_item_tags("abc123", "https://example.com", &[], None);
        assert!(find_tag(&tags, "d").is_none());
    }
}
