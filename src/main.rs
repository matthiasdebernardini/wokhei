mod delete;
mod error;
mod header;
mod item;
mod keys;
mod publish;
mod query;

use std::process;
use std::sync::Arc;

use agcli::{AgentCli, Command, CommandError, CommandRequest, ErrorEnvelope, ExecutionContext};
use serde_json::json;

// ---------------------------------------------------------------------------
// Validation helpers (agcli is schema-less — we enforce constraints ourselves)
// ---------------------------------------------------------------------------

/// Parse a boolean flag strictly. Absent = false, "true" = true, anything else = error.
fn parse_bool_flag(req: &CommandRequest<'_>, name: &str) -> Result<bool, CommandError> {
    match req.flag(name) {
        None => Ok(false),
        Some("true") => Ok(true),
        Some(other) => Err(CommandError::new(
            format!("--{name} is a boolean flag, got unexpected value: {other}"),
            "INVALID_ARGS",
            format!("Use --{name} without a value, or remove it"),
        )),
    }
}

/// Parse comma-separated flag value into Vec<String>. Absent = empty vec.
fn parse_csv(flag_value: Option<&str>) -> Vec<String> {
    match flag_value {
        Some(s) if !s.is_empty() => s.split(',').map(|v| v.trim().to_string()).collect(),
        _ => Vec::new(),
    }
}

/// Parse a usize flag with a default. Invalid values return error.
fn parse_usize_flag(
    req: &CommandRequest<'_>,
    name: &str,
    default: usize,
) -> Result<usize, CommandError> {
    match req.flag(name) {
        None => Ok(default),
        Some(v) => v.parse().map_err(|_| {
            CommandError::new(
                format!("--{name} must be a positive integer, got: {v}"),
                "INVALID_ARGS",
                format!("Provide a valid number for --{name}"),
            )
        }),
    }
}

// ---------------------------------------------------------------------------
// Command builders
// ---------------------------------------------------------------------------

fn init_command() -> Command {
    Command::new(
        "init",
        "Initialize keypair (generate new or import existing)",
    )
    .usage("wokhei init --generate | --import <file-or-stdin>")
    .handler(|req: &CommandRequest<'_>, _ctx: &mut ExecutionContext| {
        let generate = parse_bool_flag(req, "generate")?;
        let import = req.flag("import");

        if generate && import.is_some() {
            return Err(CommandError::new(
                "--generate and --import are mutually exclusive",
                "INVALID_ARGS",
                "Use either --generate or --import, not both",
            ));
        }

        keys::init(generate, import)
    })
}

fn whoami_command() -> Command {
    Command::new("whoami", "Show current identity (pubkey, npub, keys path)")
        .usage("wokhei whoami")
        .handler(|_req: &CommandRequest<'_>, _ctx: &mut ExecutionContext| keys::whoami())
}

fn create_header_command(rt: Arc<tokio::runtime::Runtime>) -> Command {
    Command::new("create-header", "Create a list header event (kind 9998 or 39998)")
        .usage("wokhei create-header --name <name> --title <title> [--relay <url>] [--aliases a,b] [--description <desc>] [--required f1,f2] [--recommended f1,f2] [--tags t1,t2] [--alt <text>] [--addressable --d-tag <id>]")
        .handler(move |req: &CommandRequest<'_>, _ctx: &mut ExecutionContext| {
            let name = req.flag("name").ok_or_else(|| {
                CommandError::new("--name is required", "MISSING_ARG", "Provide --name <list-name>")
            })?;
            let title = req.flag("title").ok_or_else(|| {
                CommandError::new("--title is required", "MISSING_ARG", "Provide --title <list-title>")
            })?;
            let relay = req.flag("relay").unwrap_or("ws://localhost:7777");
            let addressable = parse_bool_flag(req, "addressable")?;

            let params = header::HeaderParams {
                relay: relay.to_string(),
                name: name.to_string(),
                aliases: parse_csv(req.flag("aliases")),
                title: title.to_string(),
                description: req.flag("description").map(String::from),
                required: parse_csv(req.flag("required")),
                recommended: parse_csv(req.flag("recommended")),
                tags_list: parse_csv(req.flag("tags")),
                alt: req.flag("alt").map(String::from),
                addressable,
                d_tag: req.flag("d-tag").map(String::from),
            };

            rt.block_on(header::create_header(params))
        })
}

fn add_item_command(rt: Arc<tokio::runtime::Runtime>) -> Command {
    Command::new("add-item", "Add an item to a list (kind 9999 or 39999)")
        .usage("wokhei add-item --header <event-id> | --header-coordinate <kind:pubkey:d-tag> --resource <url> [--relay <url>] [--content <json>] [--fields k=v,...] [--z-tag <type>] [--addressable --d-tag <id>]")
        .handler(move |req: &CommandRequest<'_>, _ctx: &mut ExecutionContext| {
            let resource = req.flag("resource").ok_or_else(|| {
                CommandError::new("--resource is required", "MISSING_ARG", "Provide --resource <url>")
            })?;
            let relay = req.flag("relay").unwrap_or("ws://localhost:7777");
            let addressable = parse_bool_flag(req, "addressable")?;

            let params = item::ItemParams {
                relay: relay.to_string(),
                header: req.flag("header").map(String::from),
                header_coordinate: req.flag("header-coordinate").map(String::from),
                resource: resource.to_string(),
                content: req.flag("content").map(String::from),
                fields: parse_csv(req.flag("fields")),
                z_tag: req.flag("z-tag").unwrap_or("listItem").to_string(),
                addressable,
                d_tag: req.flag("d-tag").map(String::from),
            };

            rt.block_on(item::add_item(params))
        })
}

fn list_headers_command(rt: Arc<tokio::runtime::Runtime>) -> Command {
    Command::new("list-headers", "List header events from a relay")
        .usage(
            "wokhei list-headers [--relay <url>] [--author <pubkey>] [--tag <topic>] [--limit <n>]",
        )
        .handler(
            move |req: &CommandRequest<'_>, _ctx: &mut ExecutionContext| {
                let relay = req
                    .flag("relay")
                    .unwrap_or("ws://localhost:7777")
                    .to_string();
                let author = req.flag("author").map(String::from);
                let tag = req.flag("tag").map(String::from);
                let limit = parse_usize_flag(req, "limit", 50)?;

                rt.block_on(query::list_headers(relay, author, tag, limit))
            },
        )
}

fn list_items_command(rt: Arc<tokio::runtime::Runtime>) -> Command {
    Command::new("list-items", "List items belonging to a header")
        .usage("wokhei list-items <header-id> [--relay <url>] [--limit <n>]")
        .handler(
            move |req: &CommandRequest<'_>, _ctx: &mut ExecutionContext| {
                let header_id = req.arg(0).ok_or_else(|| {
                    CommandError::new(
                        "header ID is required",
                        "MISSING_ARG",
                        "Provide a header event ID as a positional argument",
                    )
                })?;
                let relay = req
                    .flag("relay")
                    .unwrap_or("ws://localhost:7777")
                    .to_string();
                let limit = parse_usize_flag(req, "limit", 100)?;

                rt.block_on(query::list_items(relay, header_id.to_string(), limit))
            },
        )
}

fn inspect_command(rt: Arc<tokio::runtime::Runtime>) -> Command {
    Command::new("inspect", "Inspect a single event in full detail")
        .usage("wokhei inspect <event-id> [--relay <url>]")
        .handler(
            move |req: &CommandRequest<'_>, _ctx: &mut ExecutionContext| {
                let event_id = req.arg(0).ok_or_else(|| {
                    CommandError::new(
                        "event ID is required",
                        "MISSING_ARG",
                        "Provide an event ID as a positional argument",
                    )
                })?;
                let relay = req
                    .flag("relay")
                    .unwrap_or("ws://localhost:7777")
                    .to_string();

                rt.block_on(query::inspect(relay, event_id.to_string()))
            },
        )
}

fn delete_command(rt: Arc<tokio::runtime::Runtime>) -> Command {
    Command::new("delete", "Delete events (NIP-09 deletion request)")
        .usage("wokhei delete <event-id>... [--relay <url>]")
        .handler(
            move |req: &CommandRequest<'_>, _ctx: &mut ExecutionContext| {
                let positionals = req.positionals();
                if positionals.is_empty() {
                    return Err(CommandError::new(
                        "at least one event ID is required",
                        "MISSING_ARG",
                        "Provide one or more event IDs as positional arguments",
                    ));
                }
                let relay = req
                    .flag("relay")
                    .unwrap_or("ws://localhost:7777")
                    .to_string();
                let event_ids: Vec<String> = positionals.to_vec();

                rt.block_on(delete::delete(relay, event_ids))
            },
        )
}

fn publish_command(rt: Arc<tokio::runtime::Runtime>) -> Command {
    Command::new(
        "publish",
        "Sign and publish raw event JSON from file or stdin",
    )
    .usage("wokhei publish <json-file-or-stdin> [--relay <url>]")
    .handler(
        move |req: &CommandRequest<'_>, _ctx: &mut ExecutionContext| {
            let input = req.arg(0).ok_or_else(|| {
                CommandError::new(
                    "JSON input source is required",
                    "MISSING_ARG",
                    "Provide a JSON file path, or use - for stdin",
                )
            })?;
            let relay = req
                .flag("relay")
                .unwrap_or("ws://localhost:7777")
                .to_string();

            rt.block_on(publish::publish(relay, input.to_string()))
        },
    )
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    // Install panic hook that outputs JSON error envelope
    std::panic::set_hook(Box::new(|info| {
        let message = if let Some(msg) = info.payload().downcast_ref::<&str>() {
            (*msg).to_string()
        } else if let Some(msg) = info.payload().downcast_ref::<String>() {
            msg.clone()
        } else {
            "Unknown panic".to_string()
        };
        let envelope = ErrorEnvelope::new(
            "unknown",
            message,
            "INTERNAL_ERROR",
            "This is a bug — please report it",
            vec![],
        );
        let json = serde_json::to_string_pretty(&envelope).unwrap_or_else(|_| {
            r#"{"ok":false,"error":{"message":"panic","code":"INTERNAL_ERROR"}}"#.to_string()
        });
        println!("{json}");
    }));

    // Build tokio runtime
    let rt = Arc::new(tokio::runtime::Runtime::new().expect("Failed to create tokio runtime"));

    let cli = AgentCli::new(
        "wokhei",
        "Agent-first CLI for Decentralized Lists on Nostr (DCoSL protocol)",
    )
    .version(env!("CARGO_PKG_VERSION"))
    .root_field("keys_configured", json!(keys::keys_exist()))
    .command(init_command())
    .command(whoami_command())
    .command(create_header_command(rt.clone()))
    .command(add_item_command(rt.clone()))
    .command(list_headers_command(rt.clone()))
    .command(list_items_command(rt.clone()))
    .command(inspect_command(rt.clone()))
    .command(delete_command(rt.clone()))
    .command(publish_command(rt));

    let execution = cli.run_env();

    println!("{}", execution.to_json_pretty());
    process::exit(execution.exit_code());
}
