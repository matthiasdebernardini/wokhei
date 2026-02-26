#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: agcli::Jemalloc = agcli::Jemalloc;

mod delete;
mod dtag;
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

fn normalize_import_source(
    import_flag: Option<&str>,
    first_arg: Option<&str>,
) -> Result<Option<String>, CommandError> {
    match import_flag {
        None => Ok(None),
        Some("true") => {
            let source = first_arg.ok_or_else(|| {
                CommandError::new(
                    "--import requires a source",
                    "INVALID_ARGS",
                    "Use --import=<file>, --import=-, or --import <file-or->",
                )
            })?;
            Ok(Some(source.to_string()))
        }
        Some(source) => Ok(Some(source.to_string())),
    }
}

fn resolve_import_source(req: &CommandRequest<'_>) -> Result<Option<String>, CommandError> {
    normalize_import_source(req.flag("import"), req.arg(0))
}

/// Resolve relay URL from --relay flag, `WOKHEI_RELAY` env var, or default.
fn resolve_relay(req: &CommandRequest<'_>) -> String {
    req.flag("relay")
        .map(String::from)
        .or_else(|| std::env::var("WOKHEI_RELAY").ok())
        .unwrap_or_else(|| "ws://localhost:7777".to_string())
}

// ---------------------------------------------------------------------------
// Command builders
// ---------------------------------------------------------------------------

fn init_command() -> Command {
    Command::new(
        "init",
        "Initialize keypair (generate new or import existing)",
    )
    .usage("wokhei init --generate | --import=<file-or-stdin>")
    .handler(|req: &CommandRequest<'_>, _ctx: &mut ExecutionContext| {
        let generate = parse_bool_flag(req, "generate")?;
        let import = resolve_import_source(req)?;

        if generate && import.is_some() {
            return Err(CommandError::new(
                "--generate and --import are mutually exclusive",
                "INVALID_ARGS",
                "Use either --generate or --import, not both",
            ));
        }

        keys::init(generate, import.as_deref())
    })
}

fn whoami_command() -> Command {
    Command::new("whoami", "Show current identity (pubkey, npub, keys path)")
        .usage("wokhei whoami")
        .handler(|_req: &CommandRequest<'_>, _ctx: &mut ExecutionContext| keys::whoami())
}

fn create_header_command(rt: Arc<tokio::runtime::Runtime>) -> Command {
    Command::new("create-header", "Create a list header event (kind 9998 or 39998)")
        .usage("wokhei create-header --name=<singular> --plural=<plural> [--titles=<singular,plural>] [--relay=<url>] [--description=<desc>] [--required=f1,f2] [--recommended=f1,f2] [--tags=t1,t2] [--alt=<text>] [--addressable [--d-tag=<id>]]")
        .handler(move |req: &CommandRequest<'_>, _ctx: &mut ExecutionContext| {
            if req.flag("title").is_some() || req.flag("aliases").is_some() {
                return Err(CommandError::new(
                    "--title/--aliases are no longer supported",
                    "INVALID_ARGS",
                    "Use --name=<singular> --plural=<plural> and optional --titles=<singular,plural>",
                ));
            }

            let name = req.flag("name").ok_or_else(|| {
                CommandError::new("--name is required", "MISSING_ARG", "Provide --name=<singular>")
            })?;
            let plural = req.flag("plural").ok_or_else(|| {
                CommandError::new(
                    "--plural is required",
                    "MISSING_ARG",
                    "Provide --plural=<plural>",
                )
            })?;
            let titles = parse_csv(req.flag("titles"));
            if !titles.is_empty() && titles.len() != 2 {
                return Err(CommandError::new(
                    "--titles requires exactly two comma-separated values",
                    "INVALID_ARGS",
                    "Use --titles=<singular,plural>",
                ));
            }

            let relay = resolve_relay(req);
            let addressable = parse_bool_flag(req, "addressable")?;

            let params = header::HeaderParams {
                relay,
                name: name.to_string(),
                plural_name: plural.to_string(),
                titles,
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
        .usage("wokhei add-item --header=<event-id> | --header-coordinate=<kind:pubkey:d-tag> --resource=<url> [--relay=<url>] [--content=<json>] [--fields=k=v,...] [--addressable [--d-tag=<id>]]")
        .handler(move |req: &CommandRequest<'_>, _ctx: &mut ExecutionContext| {
            if req.flag("z-tag").is_some() {
                return Err(CommandError::new(
                    "--z-tag is no longer supported",
                    "INVALID_ARGS",
                    "The z tag is now derived automatically from --header or --header-coordinate",
                ));
            }

            let resource = req.flag("resource").ok_or_else(|| {
                CommandError::new("--resource is required", "MISSING_ARG", "Provide --resource=<url>")
            })?;
            let relay = resolve_relay(req);
            let addressable = parse_bool_flag(req, "addressable")?;

            let params = item::ItemParams {
                relay,
                header: req.flag("header").map(String::from),
                header_coordinate: req.flag("header-coordinate").map(String::from),
                resource: resource.to_string(),
                content: req.flag("content").map(String::from),
                fields: parse_csv(req.flag("fields")),
                addressable,
                d_tag: req.flag("d-tag").map(String::from),
            };

            rt.block_on(item::add_item(params))
        })
}

fn list_headers_command(rt: Arc<tokio::runtime::Runtime>) -> Command {
    Command::new("list-headers", "List header events from a relay")
        .usage("wokhei list-headers [--relay=<url>] [--author=<pubkey>] [--tag=<topic>] [--name=<substring>] [--offset=<n>] [--limit=<n>]")
        .handler(
            move |req: &CommandRequest<'_>, _ctx: &mut ExecutionContext| {
                let relay = resolve_relay(req);
                let author = req.flag("author").map(String::from);
                let tag = req.flag("tag").map(String::from);
                let name = req.flag("name").map(String::from);
                let offset = parse_usize_flag(req, "offset", 0)?;
                let limit = parse_usize_flag(req, "limit", 50)?;

                rt.block_on(query::list_headers(relay, author, tag, name, offset, limit))
            },
        )
}

fn list_items_command(rt: Arc<tokio::runtime::Runtime>) -> Command {
    Command::new("list-items", "List items belonging to a header")
        .usage("wokhei list-items [<header-id>] [--header-coordinate=<kind:pubkey:d-tag>] [--relay=<url>] [--limit=<n>]")
        .handler(
            move |req: &CommandRequest<'_>, _ctx: &mut ExecutionContext| {
                let header_id = req.arg(0).map(String::from);
                let header_coordinate = req.flag("header-coordinate").map(String::from);

                if header_id.is_none() && header_coordinate.is_none() {
                    return Err(CommandError::new(
                        "header ID or --header-coordinate is required",
                        "MISSING_ARG",
                        "Provide a header event ID as a positional argument, or use --header-coordinate=<kind:pubkey:d-tag>",
                    ));
                }

                let relay = resolve_relay(req);
                let limit = parse_usize_flag(req, "limit", 100)?;

                rt.block_on(query::list_items(relay, header_id, header_coordinate, limit))
            },
        )
}

fn inspect_command(rt: Arc<tokio::runtime::Runtime>) -> Command {
    Command::new("inspect", "Inspect a single event in full detail")
        .usage("wokhei inspect <event-id> [--relay=<url>]")
        .handler(
            move |req: &CommandRequest<'_>, _ctx: &mut ExecutionContext| {
                let event_id = req.arg(0).ok_or_else(|| {
                    CommandError::new(
                        "event ID is required",
                        "MISSING_ARG",
                        "Provide an event ID as a positional argument",
                    )
                })?;
                let relay = resolve_relay(req);

                rt.block_on(query::inspect(relay, event_id.to_string()))
            },
        )
}

fn delete_command(rt: Arc<tokio::runtime::Runtime>) -> Command {
    Command::new("delete", "Delete events (NIP-09 deletion request)")
        .usage("wokhei delete <event-id>... [--relay=<url>]")
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
                let relay = resolve_relay(req);
                let event_ids: Vec<String> = positionals.to_vec();

                rt.block_on(delete::delete(relay, event_ids))
            },
        )
}

fn count_command(rt: Arc<tokio::runtime::Runtime>) -> Command {
    Command::new("count", "Count header and item events on a relay")
        .usage("wokhei count [--relay=<url>]")
        .handler(
            move |req: &CommandRequest<'_>, _ctx: &mut ExecutionContext| {
                let relay = resolve_relay(req);
                rt.block_on(query::count(relay))
            },
        )
}

fn export_command(rt: Arc<tokio::runtime::Runtime>) -> Command {
    Command::new("export", "Export all headers and items as JSON backup")
        .usage("wokhei export [--relay=<url>]")
        .handler(
            move |req: &CommandRequest<'_>, _ctx: &mut ExecutionContext| {
                let relay = resolve_relay(req);
                rt.block_on(query::export(relay))
            },
        )
}

fn publish_command(rt: Arc<tokio::runtime::Runtime>) -> Command {
    Command::new(
        "publish",
        "Sign and publish raw event JSON from file or stdin",
    )
    .usage("wokhei publish <json-file-or-stdin> [--relay=<url>]")
    .handler(
        move |req: &CommandRequest<'_>, _ctx: &mut ExecutionContext| {
            let input = req.arg(0).ok_or_else(|| {
                CommandError::new(
                    "JSON input source is required",
                    "MISSING_ARG",
                    "Provide a JSON file path, or use - for stdin",
                )
            })?;
            let relay = resolve_relay(req);

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
    .schema_version("wokhei.v1")
    .root_field("keys_configured", json!(keys::keys_exist()))
    .command(init_command())
    .command(whoami_command())
    .command(create_header_command(rt.clone()))
    .command(add_item_command(rt.clone()))
    .command(list_headers_command(rt.clone()))
    .command(list_items_command(rt.clone()))
    .command(inspect_command(rt.clone()))
    .command(delete_command(rt.clone()))
    .command(count_command(rt.clone()))
    .command(export_command(rt.clone()))
    .command(publish_command(rt));

    let execution = cli.run_env();

    println!("{}", execution.to_json_pretty());
    process::exit(execution.exit_code());
}

#[cfg(test)]
mod tests {
    use super::*;
    use agcli::{Command, CommandOutput};

    // -----------------------------------------------------------------------
    // normalize_import_source
    // -----------------------------------------------------------------------

    #[test]
    fn import_equals_form_is_preserved() {
        let out = normalize_import_source(Some("-"), None).expect("valid import source");
        assert_eq!(out.as_deref(), Some("-"));
    }

    #[test]
    fn import_space_form_uses_positional_source() {
        let out =
            normalize_import_source(Some("true"), Some("/dev/stdin")).expect("valid import source");
        assert_eq!(out.as_deref(), Some("/dev/stdin"));
    }

    #[test]
    fn import_missing_source_errors() {
        assert!(normalize_import_source(Some("true"), None).is_err());
    }

    // -----------------------------------------------------------------------
    // parse_csv — direct unit tests
    // -----------------------------------------------------------------------

    #[test]
    fn parse_csv_none_returns_empty() {
        assert!(parse_csv(None).is_empty());
    }

    #[test]
    fn parse_csv_empty_string_returns_empty() {
        assert!(parse_csv(Some("")).is_empty());
    }

    #[test]
    fn parse_csv_single_value() {
        assert_eq!(parse_csv(Some("a")), vec!["a"]);
    }

    #[test]
    fn parse_csv_multiple_values() {
        assert_eq!(parse_csv(Some("a,b,c")), vec!["a", "b", "c"]);
    }

    #[test]
    fn parse_csv_trims_whitespace() {
        assert_eq!(parse_csv(Some(" a , b ")), vec!["a", "b"]);
    }

    // -----------------------------------------------------------------------
    // parse_bool_flag — tested via AgentCli::run_argv
    // -----------------------------------------------------------------------

    fn bool_flag_cli() -> AgentCli {
        AgentCli::new("test", "t").command(Command::new("c", "c").handler(
            |req: &CommandRequest<'_>, _ctx: &mut ExecutionContext| {
                let v = parse_bool_flag(req, "flag")?;
                Ok(CommandOutput::new(json!({ "v": v })))
            },
        ))
    }

    #[test]
    fn bool_flag_absent_is_false() {
        let exec = bool_flag_cli().run_argv(["test", "c"]);
        assert!(exec.envelope().ok());
        let j: serde_json::Value = serde_json::from_str(&exec.to_json()).unwrap();
        assert_eq!(j["result"]["v"], false);
    }

    #[test]
    fn bool_flag_bare_is_true() {
        let exec = bool_flag_cli().run_argv(["test", "c", "--flag"]);
        assert!(exec.envelope().ok());
        let j: serde_json::Value = serde_json::from_str(&exec.to_json()).unwrap();
        assert_eq!(j["result"]["v"], true);
    }

    #[test]
    fn bool_flag_equals_true_works() {
        let exec = bool_flag_cli().run_argv(["test", "c", "--flag=true"]);
        assert!(exec.envelope().ok());
        let j: serde_json::Value = serde_json::from_str(&exec.to_json()).unwrap();
        assert_eq!(j["result"]["v"], true);
    }

    #[test]
    fn bool_flag_invalid_value_errors() {
        let exec = bool_flag_cli().run_argv(["test", "c", "--flag=nonsense"]);
        assert!(!exec.envelope().ok());
        let j: serde_json::Value = serde_json::from_str(&exec.to_json()).unwrap();
        assert_eq!(j["error"]["code"], "INVALID_ARGS");
    }

    // -----------------------------------------------------------------------
    // parse_usize_flag — tested via AgentCli::run_argv
    // -----------------------------------------------------------------------

    fn usize_flag_cli() -> AgentCli {
        AgentCli::new("test", "t").command(Command::new("c", "c").handler(
            |req: &CommandRequest<'_>, _ctx: &mut ExecutionContext| {
                let v = parse_usize_flag(req, "limit", 42)?;
                Ok(CommandOutput::new(json!({ "v": v })))
            },
        ))
    }

    #[test]
    fn usize_flag_absent_returns_default() {
        let exec = usize_flag_cli().run_argv(["test", "c"]);
        assert!(exec.envelope().ok());
        let j: serde_json::Value = serde_json::from_str(&exec.to_json()).unwrap();
        assert_eq!(j["result"]["v"], 42);
    }

    #[test]
    fn usize_flag_valid_number() {
        let exec = usize_flag_cli().run_argv(["test", "c", "--limit=10"]);
        assert!(exec.envelope().ok());
        let j: serde_json::Value = serde_json::from_str(&exec.to_json()).unwrap();
        assert_eq!(j["result"]["v"], 10);
    }

    #[test]
    fn usize_flag_zero_works() {
        let exec = usize_flag_cli().run_argv(["test", "c", "--limit=0"]);
        assert!(exec.envelope().ok());
        let j: serde_json::Value = serde_json::from_str(&exec.to_json()).unwrap();
        assert_eq!(j["result"]["v"], 0);
    }

    #[test]
    fn usize_flag_invalid_errors() {
        let exec = usize_flag_cli().run_argv(["test", "c", "--limit=abc"]);
        assert!(!exec.envelope().ok());
        let j: serde_json::Value = serde_json::from_str(&exec.to_json()).unwrap();
        assert_eq!(j["error"]["code"], "INVALID_ARGS");
    }

    // -----------------------------------------------------------------------
    // resolve_relay — tested via AgentCli::run_argv
    // These tests mutate WOKHEI_RELAY env var — run serially via nextest config.
    // -----------------------------------------------------------------------

    fn relay_cli() -> AgentCli {
        AgentCli::new("test", "t").command(Command::new("c", "c").handler(
            |req: &CommandRequest<'_>, _ctx: &mut ExecutionContext| {
                let v = resolve_relay(req);
                Ok(CommandOutput::new(json!({ "v": v })))
            },
        ))
    }

    fn relay_result(exec: &agcli::Execution) -> String {
        let j: serde_json::Value = serde_json::from_str(&exec.to_json()).unwrap();
        j["result"]["v"].as_str().unwrap().to_string()
    }

    #[test]
    fn resolve_relay_default_fallback() {
        std::env::remove_var("WOKHEI_RELAY");
        let exec = relay_cli().run_argv(["test", "c"]);
        assert!(exec.envelope().ok());
        assert_eq!(relay_result(&exec), "ws://localhost:7777");
    }

    #[test]
    fn resolve_relay_flag_override() {
        std::env::remove_var("WOKHEI_RELAY");
        let exec = relay_cli().run_argv(["test", "c", "--relay=ws://custom:1234"]);
        assert!(exec.envelope().ok());
        assert_eq!(relay_result(&exec), "ws://custom:1234");
    }

    #[test]
    fn resolve_relay_env_var() {
        std::env::set_var("WOKHEI_RELAY", "ws://envrelay:5555");
        let exec = relay_cli().run_argv(["test", "c"]);
        assert_eq!(relay_result(&exec), "ws://envrelay:5555");
        std::env::remove_var("WOKHEI_RELAY");
    }

    #[test]
    fn resolve_relay_flag_beats_env() {
        std::env::set_var("WOKHEI_RELAY", "ws://envrelay:5555");
        let exec = relay_cli().run_argv(["test", "c", "--relay=ws://flagrelay:9999"]);
        assert_eq!(relay_result(&exec), "ws://flagrelay:9999");
        std::env::remove_var("WOKHEI_RELAY");
    }
}
