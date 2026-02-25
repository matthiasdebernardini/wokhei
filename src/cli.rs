use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "wokhei",
    about = "Agent-first CLI for Decentralized Lists on Nostr",
    version,
    color = clap::ColorChoice::Never
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Initialize keypair (generate new or import existing)
    Init {
        /// Generate a new random keypair
        #[arg(long, group = "key_source")]
        generate: bool,

        /// Import an existing nsec key (bech32 format)
        #[arg(long, group = "key_source")]
        import: Option<String>,
    },

    /// Show current identity (pubkey, npub, keys path)
    Whoami,

    /// Create a new list header event (kind 9998 or 39998)
    CreateHeader {
        /// Relay URL
        #[arg(long, default_value = "ws://localhost:7777")]
        relay: String,

        /// Primary list name
        #[arg(long)]
        name: String,

        /// Extra name aliases (comma-separated)
        #[arg(long, value_delimiter = ',')]
        aliases: Vec<String>,

        /// List title
        #[arg(long)]
        title: String,

        /// List description
        #[arg(long)]
        description: Option<String>,

        /// Required fields for items (comma-separated)
        #[arg(long, value_delimiter = ',')]
        required: Vec<String>,

        /// Recommended fields for items (comma-separated)
        #[arg(long, value_delimiter = ',')]
        recommended: Vec<String>,

        /// Topic tags (comma-separated)
        #[arg(long, value_delimiter = ',')]
        tags: Vec<String>,

        /// Alt text (auto-generated if omitted)
        #[arg(long)]
        alt: Option<String>,

        /// Use addressable kind 39998 instead of 9998
        #[arg(long)]
        addressable: bool,

        /// Identifier for addressable events (required with --addressable)
        #[arg(long)]
        d_tag: Option<String>,
    },

    /// Add an item to a list (kind 9999 or 39999)
    AddItem {
        /// Relay URL
        #[arg(long, default_value = "ws://localhost:7777")]
        relay: String,

        /// Header event ID (fetches header from relay to auto-detect kind)
        #[arg(long, group = "header_ref")]
        header: Option<String>,

        /// Header coordinate as kind:pubkey:d-tag (no relay lookup)
        #[arg(long, group = "header_ref")]
        header_coordinate: Option<String>,

        /// Resource URL or identifier (r tag)
        #[arg(long)]
        resource: String,

        /// Structured JSON content
        #[arg(long)]
        content: Option<String>,

        /// Additional tag key=value pairs (comma-separated)
        #[arg(long, value_delimiter = ',')]
        fields: Vec<String>,

        /// Item type classification
        #[arg(long, default_value = "listItem")]
        z_tag: String,

        /// Use addressable kind 39999 instead of 9999
        #[arg(long)]
        addressable: bool,

        /// Identifier for addressable events (required with --addressable)
        #[arg(long)]
        d_tag: Option<String>,
    },

    /// List header events from a relay
    ListHeaders {
        /// Relay URL
        #[arg(long, default_value = "ws://localhost:7777")]
        relay: String,

        /// Filter by author pubkey (hex)
        #[arg(long)]
        author: Option<String>,

        /// Filter by topic tag
        #[arg(long)]
        tag: Option<String>,

        /// Maximum number of results
        #[arg(long, default_value = "50")]
        limit: usize,
    },

    /// List items belonging to a header
    ListItems {
        /// Relay URL
        #[arg(long, default_value = "ws://localhost:7777")]
        relay: String,

        /// Header event ID
        header_id: String,

        /// Maximum number of results
        #[arg(long, default_value = "100")]
        limit: usize,
    },

    /// Inspect a single event in full detail
    Inspect {
        /// Relay URL
        #[arg(long, default_value = "ws://localhost:7777")]
        relay: String,

        /// Event ID to inspect
        event_id: String,
    },

    /// Delete events (publishes kind 5 NIP-09 deletion request)
    Delete {
        /// Relay URL
        #[arg(long, default_value = "ws://localhost:7777")]
        relay: String,

        /// Event IDs to delete
        #[arg(required = true)]
        event_ids: Vec<String>,
    },

    /// Sign and publish raw event JSON from file or stdin
    Publish {
        /// Relay URL
        #[arg(long, default_value = "ws://localhost:7777")]
        relay: String,

        /// JSON file path (use - for stdin)
        input: String,
    },
}
