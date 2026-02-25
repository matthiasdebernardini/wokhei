mod cli;
mod delete;
mod error;
mod header;
mod item;
mod keys;
mod publish;
mod query;
mod response;

use std::process;

use clap::Parser;
use serde_json::json;

use cli::{Cli, Command};
use response::{NextAction, Response};

fn root_command() -> Response {
    let has_keys = keys::keys_exist();

    let mut actions = vec![];

    if has_keys {
        actions.push(NextAction::simple("wokhei whoami", "Show current identity"));
        actions.push(NextAction::simple(
            "wokhei create-header --relay ws://localhost:7777 --name <name> --title <title>",
            "Create a new list header (kind 9998)",
        ));
        actions.push(NextAction::simple(
            "wokhei create-header --relay ws://localhost:7777 --name <name> --title <title> --addressable --d-tag <id>",
            "Create an addressable list header (kind 39998)",
        ));
        actions.push(NextAction::simple(
            "wokhei list-headers --relay ws://localhost:7777",
            "List headers on a relay",
        ));
    } else {
        actions.push(NextAction::simple(
            "wokhei init --generate",
            "Generate a new keypair",
        ));
        actions.push(NextAction::simple(
            "wokhei init --import <nsec>",
            "Import an existing nsec key",
        ));
    }

    let commands = json!({
        "init": {
            "description": "Initialize keypair (generate new or import existing)",
            "flags": ["--generate", "--import <nsec>"]
        },
        "whoami": {
            "description": "Show current identity (pubkey, npub, keys path)"
        },
        "create-header": {
            "description": "Create a list header event (kind 9998 or 39998)",
            "flags": ["--relay <url>", "--name <name>", "--title <title>", "--aliases <a1,a2>", "--description <desc>", "--required <fields>", "--recommended <fields>", "--tags <t1,t2>", "--alt <text>", "--addressable", "--d-tag <id>"]
        },
        "add-item": {
            "description": "Add an item to a list (kind 9999 or 39999)",
            "flags": ["--relay <url>", "--header <event-id>", "--header-coordinate <kind:pubkey:d-tag>", "--resource <url>", "--content <json>", "--fields <k=v,...>", "--z-tag <type>", "--addressable", "--d-tag <id>"]
        },
        "list-headers": {
            "description": "List header events from a relay",
            "flags": ["--relay <url>", "--author <pubkey>", "--tag <topic>", "--limit <n>"]
        },
        "list-items": {
            "description": "List items belonging to a header",
            "flags": ["--relay <url>", "<header-id>", "--limit <n>"]
        },
        "inspect": {
            "description": "Inspect a single event in full detail",
            "flags": ["--relay <url>", "<event-id>"]
        },
        "delete": {
            "description": "Delete events (NIP-09 deletion request)",
            "flags": ["--relay <url>", "<event-ids...>"]
        },
        "publish": {
            "description": "Sign and publish raw event JSON",
            "flags": ["--relay <url>", "<json-file-or-stdin>"]
        }
    });

    Response::success(
        "root",
        json!({
            "name": "wokhei",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "Agent-first CLI for Decentralized Lists on Nostr (DCoSL protocol)",
            "keys_configured": has_keys,
            "commands": commands,
        }),
        actions,
    )
}

fn main() {
    // Install panic hook that outputs JSON
    std::panic::set_hook(Box::new(|info| {
        let message = if let Some(msg) = info.payload().downcast_ref::<&str>() {
            (*msg).to_string()
        } else if let Some(msg) = info.payload().downcast_ref::<String>() {
            msg.clone()
        } else {
            "Unknown panic".to_string()
        };
        let resp = Response::panic_error(message);
        println!("{}", resp.to_json());
    }));

    // Parse CLI — catch ALL clap errors including --help and --version
    let cli = match Cli::try_parse() {
        Ok(c) => c,
        Err(e) => {
            let resp = Response::clap_error(e.to_string());
            println!("{}", resp.to_json());
            process::exit(1);
        }
    };

    // Build tokio runtime
    let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");

    let response = match cli.command {
        None => root_command(),
        Some(Command::Init { generate, import }) => keys::init(generate, import),
        Some(Command::Whoami) => keys::whoami(),
        Some(Command::CreateHeader {
            relay,
            name,
            aliases,
            title,
            description,
            required,
            recommended,
            tags,
            alt,
            addressable,
            d_tag,
        }) => rt.block_on(header::create_header(header::HeaderParams {
            relay,
            name,
            aliases,
            title,
            description,
            required,
            recommended,
            tags_list: tags,
            alt,
            addressable,
            d_tag,
        })),
        Some(Command::AddItem {
            relay,
            header,
            header_coordinate,
            resource,
            content,
            fields,
            z_tag,
            addressable,
            d_tag,
        }) => rt.block_on(item::add_item(item::ItemParams {
            relay,
            header,
            header_coordinate,
            resource,
            content,
            fields,
            z_tag,
            addressable,
            d_tag,
        })),
        Some(Command::ListHeaders {
            relay,
            author,
            tag,
            limit,
        }) => rt.block_on(query::list_headers(relay, author, tag, limit)),
        Some(Command::ListItems {
            relay,
            header_id,
            limit,
        }) => rt.block_on(query::list_items(relay, header_id, limit)),
        Some(Command::Inspect { relay, event_id }) => rt.block_on(query::inspect(relay, event_id)),
        Some(Command::Delete { relay, event_ids }) => rt.block_on(delete::delete(relay, event_ids)),
        Some(Command::Publish { relay, input }) => rt.block_on(publish::publish(relay, input)),
    };

    println!("{}", response.to_json());

    if !response.ok {
        process::exit(1);
    }
}
