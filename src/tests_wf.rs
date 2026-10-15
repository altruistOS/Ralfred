use crate::workflow::parse_script_filter_output;

#[test]
fn script_filter_json() {
    let items = parse_script_filter_output(
        r#"{"items":[{"title":"Hello","subtitle":"world","arg":"http://x"}]}"#,
    );
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "Hello");
    assert_eq!(items[0].path.as_deref(), Some("http://x"));
}

#[test]
fn script_filter_plain_lines() {
    let items = parse_script_filter_output("alpha\nbeta\n");
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].title, "alpha");
}

#[test]
fn script_filter_empty() {
    assert!(parse_script_filter_output("").is_empty());
    assert!(parse_script_filter_output("  ").is_empty());
}

#[test]
fn workflows_dedupe_after_unpack() {
    // a `.alfredworkflow` beside its own `.wfunpack` must yield ONE entry
    // (the user saw "two Snippet Triggers - Getting Started")
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
  <key>name</key><string>Snippet Triggers - Getting Started</string>
  <key>objects</key><array></array>
  <key>connections</key><dict></dict>
</dict>
</plist>"#;
    let dir = std::env::temp_dir().join(format!("ralfred_wf_{}", std::process::id()));
    let unpack = dir.join("Snippet Triggers.wfunpack");
    std::fs::create_dir_all(&unpack).unwrap();
    std::fs::write(unpack.join("info.plist"), xml).unwrap();
    // an invalid zip file is fine: unpacking is skipped because the folder
    // already exists — which is exactly the user's on-disk state
    std::fs::write(dir.join("Snippet Triggers.alfredworkflow"), b"not a zip").unwrap();
    let wfs = crate::workflow::load_workflows(&dir);
    let dupes = wfs.iter().filter(|w| w.name.contains("Snippet")).count();
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(dupes, 1, "snippet workflow listed {} times", dupes);
}
