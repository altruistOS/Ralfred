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
