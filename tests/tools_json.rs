//! The web app's copy of the tool list (web/src/lib/api/tools.json), from which it generates its TypeScript types.
//! Fails when the copy is stale; `UPDATE_TOOLS=1 cargo test --test tools_json` rewrites it.

use health_mcp::tools::Health;

#[test]
fn web_tools_json_is_current() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/web/src/lib/api/tools.json");
    let tools: Vec<_> = Health::tool_list()
        .into_iter()
        .map(|t| serde_json::json!({ "name": t.name, "description": t.description, "inputSchema": t.input_schema, "annotations": t.annotations }))
        .collect();
    let want = serde_json::to_string_pretty(&tools).unwrap() + "\n";
    if std::env::var_os("UPDATE_TOOLS").is_some() {
        std::fs::write(path, &want).unwrap();
        return;
    }
    let have = std::fs::read_to_string(path).unwrap_or_default();
    assert!(
        have == want,
        "{path} is stale: run `UPDATE_TOOLS=1 cargo test --test tools_json`, then `pnpm gen:tools` in web/"
    );
}
