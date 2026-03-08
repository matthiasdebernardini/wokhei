use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;

struct TestContext {
    home: TempDir,
    relay: String,
    bin: PathBuf,
}

impl TestContext {
    fn new() -> Self {
        let home = TempDir::new().expect("failed to create temp HOME");
        let relay = std::env::var("WOKHEI_RELAY").unwrap_or_else(|_| "ws://localhost:7777".into());
        let bin = PathBuf::from(env!("CARGO_BIN_EXE_wokhei"));
        Self { home, relay, bin }
    }

    fn run(&self, args: &[&str]) -> Value {
        let output = Command::new(&self.bin)
            .args(args)
            .env("HOME", self.home.path())
            .env("WOKHEI_RELAY", &self.relay)
            .output()
            .expect("failed to execute wokhei");

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        serde_json::from_str(&stdout).unwrap_or_else(|e| {
            panic!(
                "failed to parse JSON from wokhei {args:?}\n\
                 error: {e}\n\
                 stdout: {stdout}\n\
                 stderr: {stderr}"
            )
        })
    }

    fn run_ok(&self, args: &[&str]) -> Value {
        let json = self.run(args);
        assert!(
            json["ok"].as_bool().unwrap_or(false),
            "expected ok=true for {args:?}, got: {json}"
        );
        json
    }

    fn init(&self) -> Value {
        self.run_ok(&["init", "--generate"])
    }

    fn pubkey(&self) -> String {
        let json = self.run_ok(&["whoami"]);
        json["result"]["pubkey"]
            .as_str()
            .expect("missing pubkey")
            .to_string()
    }
}

#[test]
#[ignore = "requires strfry relay"]
fn init_and_whoami() {
    let ctx = TestContext::new();

    let init = ctx.init();
    let init_pubkey = init["result"]["pubkey"]
        .as_str()
        .expect("init should return pubkey");
    assert_eq!(init_pubkey.len(), 64, "pubkey should be 64 hex chars");

    let whoami = ctx.run_ok(&["whoami"]);
    let whoami_pubkey = whoami["result"]["pubkey"]
        .as_str()
        .expect("whoami should return pubkey");

    assert_eq!(init_pubkey, whoami_pubkey, "pubkey should roundtrip");

    let npub = whoami["result"]["npub"]
        .as_str()
        .expect("whoami should return npub");
    assert!(npub.starts_with("npub1"), "npub should start with npub1");
}

#[test]
#[ignore = "requires strfry relay"]
fn create_header_and_list() {
    let ctx = TestContext::new();
    ctx.init();
    let pk = ctx.pubkey();

    let header = ctx.run_ok(&["create-header", "--name=inttest", "--plural=inttests"]);
    let event_id = header["result"]["event_id"]
        .as_str()
        .expect("should return event_id");
    assert_eq!(event_id.len(), 64);
    assert_eq!(header["result"]["kind"], 9998);

    let list = ctx.run_ok(&["list-headers", &format!("--author={pk}")]);
    let headers = list["result"]["headers"]
        .as_array()
        .expect("headers should be an array");
    assert!(
        headers.iter().any(|h| h["event_id"] == event_id),
        "created header should appear in list-headers"
    );
}

#[test]
#[ignore = "requires strfry relay"]
fn create_addressable_header() {
    let ctx = TestContext::new();
    ctx.init();

    let header = ctx.run_ok(&[
        "create-header",
        "--name=addr",
        "--plural=addrs",
        "--addressable",
    ]);
    let result = &header["result"];

    assert_eq!(result["kind"], 39998);
    assert!(
        result["d_tag"].as_str().is_some_and(|s| !s.is_empty()),
        "addressable header should have d_tag"
    );
    assert!(
        result["coordinate"]
            .as_str()
            .is_some_and(|s| s.starts_with("39998:")),
        "addressable header should have coordinate"
    );
}

#[test]
#[ignore = "requires strfry relay"]
fn add_item_and_list() {
    let ctx = TestContext::new();
    ctx.init();

    let header = ctx.run_ok(&["create-header", "--name=itemtest", "--plural=itemtests"]);
    let header_id = header["result"]["event_id"]
        .as_str()
        .expect("header event_id");

    let item = ctx.run_ok(&[
        "add-item",
        &format!("--header={header_id}"),
        "--resource=https://example.com/test-item",
    ]);
    let item_id = item["result"]["event_id"].as_str().expect("item event_id");
    assert_eq!(item_id.len(), 64);

    let items = ctx.run_ok(&["list-items", header_id]);
    let item_list = items["result"]["items"]
        .as_array()
        .expect("items should be an array");
    assert!(
        item_list.iter().any(|i| i["event_id"] == item_id),
        "created item should appear in list-items"
    );
    assert!(
        item_list.iter().any(|i| {
            i["tags"].as_array().is_some_and(|tags| {
                tags.iter()
                    .any(|t| t[0] == "r" && t[1] == "https://example.com/test-item")
            })
        }),
        "item resource should be in tags as [\"r\", url]"
    );
}

#[test]
#[ignore = "requires strfry relay"]
fn inspect_event() {
    let ctx = TestContext::new();
    ctx.init();

    let header = ctx.run_ok(&[
        "create-header",
        "--name=insptest",
        "--plural=insptests",
        "--description=A test header for inspect",
    ]);
    let event_id = header["result"]["event_id"].as_str().expect("event_id");

    let inspected = ctx.run_ok(&["inspect", event_id]);
    let result = &inspected["result"];

    assert_eq!(result["event_id"], event_id);
    assert!(result["kind"] == 9998 || result["kind"] == 39998);
    assert!(result["pubkey"].as_str().is_some());
    assert!(result["created_at"].as_i64().is_some());
    assert!(result["tags"].as_array().is_some());
    assert_eq!(result["name"], "insptest");
}

#[test]
#[ignore = "requires strfry relay"]
fn delete_event() {
    let ctx = TestContext::new();
    ctx.init();
    let pk = ctx.pubkey();

    let header = ctx.run_ok(&["create-header", "--name=deltest", "--plural=deltests"]);
    let event_id = header["result"]["event_id"].as_str().expect("event_id");

    let del = ctx.run_ok(&["delete", event_id]);
    assert!(
        del["result"]["deletion_event_id"].as_str().is_some(),
        "delete should return deletion_event_id"
    );
    assert!(
        del["result"]["deleted_ids"]
            .as_array()
            .is_some_and(|a| a.iter().any(|id| id == event_id)),
        "deleted_ids should contain the event"
    );

    // After deletion, list-headers may return NO_RESULTS (ok=false) if no
    // headers remain, or ok=true with a list that excludes the deleted event.
    let list = ctx.run(&["list-headers", &format!("--author={pk}")]);
    if list["ok"].as_bool().unwrap_or(false) {
        let empty = vec![];
        let headers = list["result"]["headers"].as_array().unwrap_or(&empty);
        assert!(
            !headers.iter().any(|h| h["event_id"] == event_id),
            "deleted header should not appear in list-headers"
        );
    }
}
