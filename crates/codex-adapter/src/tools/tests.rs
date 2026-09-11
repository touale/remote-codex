use super::*;
use serde_json::json;

#[test]
fn web_actions_keep_queries_links_and_unrecognized_results()
-> Result<(), Box<dyn std::error::Error>> {
    let search = project(&json!({"id":"search","type":"webSearch","query":"fallback",
        "action":{"type":"search","queries":["Rust async","Rust cancellation"]},
        "results":[{"title":"Rust book","url":"https://doc.rust-lang.org/book/","snippet":"Official guide"},
            {"type":"answer","text":"Additional search context"},
            {"title":"Not a web link","url":"file:///tmp/private"}]
    })).ok_or("search missing")?;
    assert_eq!(search.title, "Search the web · Rust async");
    assert_eq!(
        search.input.as_deref(),
        Some("Rust async\nRust cancellation")
    );
    assert_eq!(search.links.len(), 1);
    assert_eq!(
        search.links[0].description.as_deref(),
        Some("Official guide")
    );
    assert!(search.output.contains("Additional search context"));
    for (action, title) in [
        (
            json!({"type":"openPage","url":"https://example.com/"}),
            "Open webpage · https://example.com/",
        ),
        (
            json!({"type":"findInPage","url":"https://example.com/","pattern":"runtime"}),
            "Find in page · runtime",
        ),
    ] {
        let item = project(&json!({"id":"page","type":"webSearch","query":"","action":action}))
            .ok_or("page missing")?;
        assert_eq!(item.title, title);
        assert_eq!(item.links.len(), 1);
    }
    let empty = project(&json!({"id":"empty","type":"webSearch","query":"","action":null}))
        .ok_or("empty search missing")?;
    assert!(empty.input.is_none() && empty.output.is_empty() && empty.links.is_empty());
    assert!(empty.status.is_empty());
    Ok(())
}

#[test]
fn execution_records_have_readable_input_results_and_errors()
-> Result<(), Box<dyn std::error::Error>> {
    let command = project(
        &json!({"id":"cmd","type":"commandExecution","command":"touch hello.txt",
        "cwd":"/workspace/test","exitCode":0,"status":"completed","aggregatedOutput":null}),
    )
    .ok_or("command missing")?;
    assert_eq!(
        command.input.as_deref(),
        Some("touch hello.txt\n\nDirectory: /workspace/test\nExit code: 0")
    );
    assert!(command.output.is_empty());
    let dynamic = project(
        &json!({"id":"dynamic","type":"dynamicToolCall","namespace":"tools",
        "tool":"inspect","arguments":{"path":"src"},"contentItems":[
            {"type":"inputText","text":"Two files found"},
            {"type":"inputImage","imageUrl":"data:image/png;base64,private-payload"}
        ]}),
    )
    .ok_or("dynamic tool missing")?;
    assert_eq!(dynamic.title, "tools · inspect");
    assert_eq!(dynamic.input.as_deref(), Some("{\n  \"path\": \"src\"\n}"));
    assert!(dynamic.output.contains("Two files found") && dynamic.output.contains("Image result"));
    assert!(!dynamic.output.contains("private-payload"));
    let mcp = project(&json!({"id":"mcp","type":"mcpToolCall","server":"docs","tool":"search",
        "arguments":{"q":"rust"},"status":"failed","result":{"content":[{"type":"text","text":"Partial result"}]},
        "error":{"message":"Request timed out"}})).ok_or("mcp missing")?;
    assert_eq!(mcp.title, "docs · search");
    assert_eq!(mcp.output, "Partial result\n\nError: Request timed out");
    assert_eq!(mcp.status, "failed");
    let reasoning = project(&json!({"id":"thought","type":"reasoning","summary":["Check dependencies", "Compare APIs"]}))
        .ok_or("reasoning missing")?;
    assert_eq!(reasoning.output, "Check dependencies\n\nCompare APIs");
    let patch = project(
        &json!({"id":"patch","type":"fileChange","status":"completed",
        "changes":[{"path":"src/main.rs","diff":"+hello"}]}),
    )
    .ok_or("patch missing")?;
    assert_eq!(patch.title, "File changes · 1 file");
    assert_eq!(patch.changes[0].diff, "+hello");
    let old: ToolItem =
        serde_json::from_value(json!({"id":"old","kind":"webSearch","title":"webSearch",
        "status":"","output":"","changes":[]}))?;
    assert!(old.input.is_none() && old.links.is_empty());
    Ok(())
}
