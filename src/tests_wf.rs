//! Script-filter protocol and workflow-graph tests.
//! Each test names the SPEC.md requirement it pins down.

use crate::workflow::{
    parse_script_filter, parse_script_filter_output, parse_script_filter_output_checked, Workflow,
    RERUN_MAX, RERUN_MIN,
};

/// Build a throwaway unpacked workflow directory containing `info.plist`.
fn workflow_dir(tag: &str, xml: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ralfred_wf_{}_{}", tag, std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("info.plist"), xml).unwrap();
    dir
}

fn minimal_plist(name: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
  <key>name</key><string>{name}</string>
  <key>objects</key><array></array>
  <key>connections</key><dict></dict>
</dict>
</plist>"#
    )
}

// --- JSON form (SPEC §6.6) -------------------------------------------------

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
fn script_filter_empty() {
    assert!(parse_script_filter_output("").is_empty());
    assert!(parse_script_filter_output("  ").is_empty());
    // Empty output is an empty result, not a diagnostic.
    assert!(parse_script_filter("").is_ok());
    assert!(parse_script_filter("  \n ").is_ok());
}

#[test]
fn script_filter_full_item_fields() {
    let raw = r#"{"items":[{
        "uid":"u1","type":"file","title":"T","subtitle":"S","arg":"A",
        "valid":false,"autocomplete":"ac","match":"mt","quicklookurl":"ql",
        "icon":{"type":"filetype","path":"/tmp/i.png"},
        "text":{"copy":"cp","largetype":"lt"},
        "mods":{"cmd":{"valid":true,"arg":"carg","subtitle":"csub"}}
    }]}"#;
    let out = parse_script_filter(raw).expect("valid payload");
    assert_eq!(out.items.len(), 1);
    let it = &out.items[0];
    assert_eq!(it.uid, "u1");
    assert_eq!(it.item_type, "file");
    assert_eq!(it.title, "T");
    assert_eq!(it.subtitle, "S");
    assert_eq!(it.arg, "A");
    assert!(!it.valid, "explicit valid:false must be preserved");
    assert_eq!(it.autocomplete, "ac");
    assert_eq!(it.match_text, "mt");
    assert_eq!(it.quicklookurl, "ql");
    assert_eq!(it.icon.as_ref().unwrap().kind, "filetype");
    assert_eq!(it.icon.as_ref().unwrap().path, "/tmp/i.png");
    assert_eq!(it.text.as_ref().unwrap().copy, "cp");
    assert_eq!(it.text.as_ref().unwrap().largetype, "lt");
    assert_eq!(it.mods.get("cmd").unwrap().arg, "carg");
    assert!(it.mods.get("cmd").unwrap().valid);
}

#[test]
fn script_filter_valid_defaults_to_true() {
    let out = parse_script_filter(r#"{"items":[{"title":"x"}]}"#).unwrap();
    assert!(out.items[0].valid);
}

#[test]
fn script_filter_invalid_item_has_no_action_payload() {
    // SPEC §6.6: valid:false means the row is shown but not actionable.
    let items = parse_script_filter_output(r#"{"items":[{"title":"x","arg":"a","valid":false}]}"#);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "x");
    assert!(items[0].path.is_none(), "an invalid row must carry no payload");
}

#[test]
fn script_filter_variables_and_rerun() {
    let out = parse_script_filter(
        r#"{"rerun":1.0,"variables":{"counter":3,"flag":true,"f":1.5},"items":[{"title":"T"}]}"#,
    )
    .unwrap();
    assert_eq!(out.rerun, Some(1.0));
    assert_eq!(out.variables.get("counter").unwrap(), "3");
    assert_eq!(out.variables.get("flag").unwrap(), "true");
    assert_eq!(out.variables.get("f").unwrap(), "1.5");
}

#[test]
fn script_filter_variables_only_is_valid() {
    // A script filter may emit session state alone, with no items.
    let out = parse_script_filter(r#"{"variables":{"a":"b"}}"#).unwrap();
    assert!(out.items.is_empty());
    assert_eq!(out.variables.get("a").unwrap(), "b");
}

#[test]
fn script_filter_rerun_bounds() {
    assert_eq!(parse_script_filter(r#"{"rerun":0.1}"#).unwrap().rerun, Some(0.1));
    assert_eq!(parse_script_filter(r#"{"rerun":5.0}"#).unwrap().rerun, Some(5.0));
    // SPEC §8.5 — outside the interval it is ignored, not an error, and the
    // rest of the payload still parses.
    let out = parse_script_filter(r#"{"rerun":10,"items":[{"title":"T"}]}"#).unwrap();
    assert_eq!(out.rerun, None);
    assert_eq!(out.items.len(), 1);
    let out = parse_script_filter(r#"{"rerun":0.01,"items":[{"title":"T"}]}"#).unwrap();
    assert_eq!(out.rerun, None);
}

#[test]
fn script_filter_rerunafter_alias() {
    assert_eq!(parse_script_filter(r#"{"rerunafter":2}"#).unwrap().rerun, Some(2.0));
}

#[test]
fn script_filter_rerun_not_a_number_is_an_error() {
    let e = parse_script_filter(r#"{"rerun":"soon"}"#).unwrap_err();
    assert!(e.contains("rerun"), "diagnostic should name the key: {e}");
}

#[test]
fn script_filter_alfredworkflow_root() {
    // SPEC §6.6 — a single `alfredworkflow` root wrapper is accepted.
    let out = parse_script_filter(r#"{"alfredworkflow":{"items":[{"title":"W"}]}}"#).unwrap();
    assert_eq!(out.items.len(), 1);
    assert_eq!(out.items[0].title, "W");
}

// --- JSON error cases (SPEC §8.4) -----------------------------------------

#[test]
fn script_filter_missing_keys_is_reported() {
    let e = parse_script_filter(r#"{"nope":1}"#).unwrap_err();
    assert!(
        e.contains("items") && e.contains("variables") && e.contains("rerunafter"),
        "diagnostic must name the expected keys: {e}"
    );
}

#[test]
fn script_filter_plain_text_is_not_a_payload() {
    // Without items/variables/rerun there is nothing to parse; the previous
    // newline fallback was not an observed Alfred behaviour (SPEC §8.4).
    assert!(parse_script_filter("alpha\nbeta\n").is_err());
    assert!(parse_script_filter_output("alpha\nbeta\n").is_empty());
    // …but the checked variant hands the caller the reason.
    let e = parse_script_filter_output_checked("alpha\nbeta\n").unwrap_err();
    assert!(e.contains("JSON"), "diagnostic should name the problem: {e}");
}

#[test]
fn script_filter_items_not_an_array_is_an_error() {
    assert!(parse_script_filter(r#"{"items":{}}"#).is_err());
}

#[test]
fn script_filter_item_not_an_object_is_an_error() {
    let e = parse_script_filter(r#"{"items":[1,2]}"#).unwrap_err();
    assert!(e.contains("items[0]"), "diagnostic should locate the row: {e}");
}

#[test]
fn script_filter_malformed_json_reports_the_error() {
    let e = parse_script_filter(r#"{"items":[}"#).unwrap_err();
    assert!(e.contains("JSON error"), "diagnostic: {e}");
}

#[test]
fn script_filter_non_object_json_is_an_error() {
    assert!(parse_script_filter("[1,2,3]").is_err());
}

// --- XML form (SPEC §6.6) --------------------------------------------------

const XML_SAMPLE: &str = r#"<?xml version="1.0"?>
<output>
  <rerun>1</rerun>
  <variables>
    <variable name="counter">7</variable>
  </variables>
  <items>
    <item arg="Script run 7 times">
      <title>Variable Counter</title>
      <subtitle>This script has run 7 times this session</subtitle>
    </item>
  </items>
</output>"#;

#[test]
fn script_filter_xml_form() {
    let out = parse_script_filter(XML_SAMPLE).expect("shipped XML shape must parse");
    assert_eq!(out.rerun, Some(1.0));
    assert_eq!(out.variables.get("counter").unwrap(), "7");
    assert_eq!(out.items.len(), 1);
    let it = &out.items[0];
    assert_eq!(it.title, "Variable Counter");
    assert_eq!(it.subtitle, "This script has run 7 times this session");
    assert_eq!(it.arg, "Script run 7 times");
}

#[test]
fn script_filter_xml_text_and_icon_and_entities() {
    let xml = r#"<?xml version="1.0"?>
<output>
  <items>
    <item arg="a&amp;b">
      <title>x &lt;y&gt;</title>
      <icon type="fileicon">/tmp/i.png</icon>
      <text type="copy">CP</text>
      <text type="largetype">LT</text>
      <quicklookurl>http://q</quicklookurl>
      <valid>false</valid>
      <autocomplete>ac</autocomplete>
      <match>mt</match>
    </item>
  </items>
</output>"#;
    let out = parse_script_filter(xml).unwrap();
    let it = &out.items[0];
    assert_eq!(it.arg, "a&b");
    assert_eq!(it.title, "x <y>");
    assert_eq!(it.icon.as_ref().unwrap().kind, "fileicon");
    assert_eq!(it.icon.as_ref().unwrap().path, "/tmp/i.png");
    assert_eq!(it.text.as_ref().unwrap().copy, "CP");
    assert_eq!(it.text.as_ref().unwrap().largetype, "LT");
    assert_eq!(it.quicklookurl, "http://q");
    assert!(!it.valid);
    assert_eq!(it.autocomplete, "ac");
    assert_eq!(it.match_text, "mt");
}

#[test]
fn script_filter_xml_bad_root_is_reported() {
    let e = parse_script_filter(r#"<?xml version="1.0"?><plist><dict/></plist>"#).unwrap_err();
    assert!(e.contains("output"), "diagnostic must name the expected root: {e}");
}

#[test]
fn script_filter_xml_malformed_reports_position() {
    let e =
        parse_script_filter("<output><items><item><title>x</title></items></output>").unwrap_err();
    assert!(e.contains("XML Parse Error"), "diagnostic: {e}");
    assert!(e.contains("Row"), "diagnostic should carry a position: {e}");
}

#[test]
fn script_filter_xml_unclosed_element_is_reported() {
    let e = parse_script_filter("<output><items>").unwrap_err();
    assert!(e.contains("XML Parse Error"), "diagnostic: {e}");
}

#[test]
fn script_filter_xml_empty_items() {
    let out = parse_script_filter("<output><items></items></output>").unwrap();
    assert!(out.items.is_empty());
}

#[test]
fn script_filter_xml_rerun_out_of_range_ignored() {
    let out = parse_script_filter("<output><rerun>42</rerun><items></items></output>").unwrap();
    assert_eq!(out.rerun, None);
}

// --- workflow graph (SPEC §8.2) -------------------------------------------

#[test]
fn defunct_connection_target_is_dropped_and_reported() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
  <key>bundleid</key><string>com.example.test</string>
  <key>name</key><string>Graph Test</string>
  <key>description</key><string>d</string>
  <key>version</key><string>2.1</string>
  <key>objects</key><array>
    <dict>
      <key>uid</key><string>A</string>
      <key>type</key><string>alfred.workflow.input.scriptfilter</string>
      <key>config</key><dict>
        <key>keyword</key><string>gt</string>
      </dict>
    </dict>
  </array>
  <key>connections</key><dict>
    <key>A</key><array>
      <dict>
        <key>destinationuid</key><string>MISSING</string>
        <key>modifiers</key><integer>0</integer>
      </dict>
    </array>
  </dict>
</dict>
</plist>"#;
    let dir = workflow_dir("graph", xml);
    let wf = Workflow::load_dir(&dir).expect("workflow loads");
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(wf.bundleid, "com.example.test");
    assert_eq!(wf.name, "Graph Test");
    assert_eq!(wf.description, "d");
    assert_eq!(wf.version, "2.1");
    assert!(
        wf.connections.values().all(|v| v.is_empty()),
        "the dangling edge must be dropped"
    );
    assert!(
        wf.warnings.iter().any(|w| w.contains("MISSING")),
        "the dropped edge must be reported: {:?}",
        wf.warnings
    );
}

#[test]
fn valid_connection_survives() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
  <key>name</key><string>Keep</string>
  <key>objects</key><array>
    <dict><key>uid</key><string>A</string><key>type</key><string>t.a</string><key>config</key><dict/></dict>
    <dict><key>uid</key><string>B</string><key>type</key><string>t.b</string><key>config</key><dict/></dict>
  </array>
  <key>connections</key><dict>
    <key>A</key><array>
      <dict><key>destinationuid</key><string>B</string><key>modifiers</key><integer>0</integer></dict>
    </array>
  </dict>
</dict>
</plist>"#;
    let dir = workflow_dir("keep", xml);
    let wf = Workflow::load_dir(&dir).expect("workflow loads");
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(wf.connections.get("A").unwrap().len(), 1);
    assert!(wf.warnings.is_empty(), "no warnings expected: {:?}", wf.warnings);
}

// --- environment contract (SPEC §6.7) -------------------------------------

#[test]
fn script_env_exports_the_alfred_contract() {
    let dir = workflow_dir("env", &minimal_plist("Env Test"));
    let wf = Workflow::load_dir(&dir).expect("workflow loads");
    std::fs::remove_dir_all(&dir).ok();
    let env = wf.script_env("query");
    let get =
        |k: &str| -> Option<String> { env.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone()) };
    for key in [
        "alfred_version",
        "alfred_version_build",
        "alfred_workflow_bundleid",
        "alfred_workflow_uid",
        "alfred_workflow_name",
        "alfred_workflow_description",
        "alfred_workflow_version",
        "alfred_workflow_keyword",
        "alfred_workflow_data",
        "alfred_workflow_cache",
        "alfred_preferences",
        "alfred_preferences_localhash",
        "alfred_debug",
        "alfred_theme",
        "alfred_theme_background",
        "alfred_theme_selection_background",
        "alfred_theme_subtext",
        "alfred_theme_window_width",
        "ralfred_version",
    ] {
        assert!(get(key).is_some(), "missing env var {key}");
    }
    assert_eq!(get("alfred_workflow_name").unwrap(), "Env Test");
    assert_eq!(
        get("alfred_version").unwrap(),
        crate::workflow::ALFRED_COMPAT_VERSION
    );
    // The workflow data/cache paths must be per-workflow, not shared.
    assert!(get("alfred_workflow_data").unwrap().contains("workflow-data"));
    assert!(get("alfred_workflow_cache").unwrap().contains("workflow-cache"));
}

#[test]
fn script_env_includes_workflow_variables() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
  <key>name</key><string>Vars</string>
  <key>variables</key><dict>
    <key>counter</key><string>5</string>
  </dict>
  <key>objects</key><array></array>
  <key>connections</key><dict></dict>
</dict>
</plist>"#;
    let dir = workflow_dir("vars", xml);
    let wf = Workflow::load_dir(&dir).expect("workflow loads");
    std::fs::remove_dir_all(&dir).ok();
    let env = wf.script_env("");
    assert!(
        env.iter()
            .any(|(k, v)| k == "ALFRED_WORKFLOW_VARIABLE_COUNTER" && v == "5"),
        "workflow variables must reach the script: {env:?}"
    );
}

// --- end-to-end script filter execution -----------------------------------

/// A script filter that interpolates a workflow variable must receive it in
/// the environment, and its JSON must reach the result list (SPEC §6.5–§6.7).
#[test]
fn script_filter_sees_workflow_variables_end_to_end() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
  <key>name</key><string>Vars E2E</string>
  <key>variables</key><dict>
    <key>WHO</key><string>probe-value</string>
  </dict>
  <key>objects</key><array>
    <dict>
      <key>uid</key><string>SF</string>
      <key>type</key><string>alfred.workflow.input.scriptfilter</string>
      <key>config</key><dict>
        <key>keyword</key><string>vve2e</string>
        <key>type</key><integer>2</integer>
        <key>script</key>
        <string>echo "{\"items\":[{\"title\":\"t-$ALFRED_WORKFLOW_VARIABLE_WHO\",\"arg\":\"x\"}]}"</string>
      </dict>
    </dict>
  </array>
  <key>connections</key><dict></dict>
</dict>
</plist>"#;
    let dir = workflow_dir("varenv", xml);
    let wf = Workflow::load_dir(&dir).expect("workflow loads");
    let (items, warnings) = wf.run_keyword_report("");
    std::fs::remove_dir_all(&dir).ok();
    assert!(
        warnings.is_empty(),
        "the script filter reported problems: {warnings:?}"
    );
    assert_eq!(items.len(), 1, "no rows: {items:?} / {warnings:?}");
    assert_eq!(
        items[0].title, "t-probe-value",
        "workflow variable did not reach the script: {items:?}"
    );
}

/// A script filter emitting the legacy XML form must produce rows end to end.
#[test]
fn script_filter_xml_runs_end_to_end() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
  <key>name</key><string>XML E2E</string>
  <key>objects</key><array>
    <dict>
      <key>uid</key><string>SF</string>
      <key>type</key><string>alfred.workflow.input.scriptfilter</string>
      <key>config</key><dict>
        <key>keyword</key><string>xlee2e</string>
        <key>type</key><integer>2</integer>
        <key>script</key>
        <string>printf '&lt;output&gt;&lt;items&gt;&lt;item arg="a"&gt;&lt;title&gt;xml-hit&lt;/title&gt;&lt;/item&gt;&lt;/items&gt;&lt;/output&gt;'</string>
      </dict>
    </dict>
  </array>
  <key>connections</key><dict></dict>
</dict>
</plist>"#;
    let dir = workflow_dir("xmle2e", xml);
    let wf = Workflow::load_dir(&dir).expect("workflow loads");
    let (items, warnings) = wf.run_keyword_report("");
    std::fs::remove_dir_all(&dir).ok();
    assert!(warnings.is_empty(), "warnings: {warnings:?}");
    assert_eq!(items.len(), 1, "no rows: {warnings:?}");
    assert_eq!(items[0].title, "xml-hit");
}

/// A script filter that emits garbage must surface a diagnostic rather than
/// silently returning rows or falling back to a newline split (SPEC §8.4).
#[test]
fn script_filter_bad_output_is_reported_end_to_end() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
  <key>name</key><string>Bad E2E</string>
  <key>objects</key><array>
    <dict>
      <key>uid</key><string>SF</string>
      <key>type</key><string>alfred.workflow.input.scriptfilter</string>
      <key>config</key><dict>
        <key>keyword</key><string>bade2e</string>
        <key>type</key><integer>2</integer>
        <key>script</key><string>echo not-json-at-all</string>
      </dict>
    </dict>
  </array>
  <key>connections</key><dict></dict>
</dict>
</plist>"#;
    let dir = workflow_dir("bade2e", xml);
    let wf = Workflow::load_dir(&dir).expect("workflow loads");
    let (items, warnings) = wf.run_keyword_report("");
    std::fs::remove_dir_all(&dir).ok();
    assert!(items.is_empty(), "garbage output must not yield rows: {items:?}");
    assert!(
        warnings.iter().any(|w| w.contains("JSON")),
        "the parse failure must be reported: {warnings:?}"
    );
}

// --- archive/dir de-duplication (SPEC §8.12) ------------------------------

#[test]
fn workflows_dedupe_after_unpack() {
    // a `.alfredworkflow` beside its own `.wfunpack` must yield ONE entry
    // (the user saw "two Snippet Triggers - Getting Started")
    let dir = std::env::temp_dir().join(format!("ralfred_wf_dedupe_{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    let unpack = dir.join("Snippet Triggers.wfunpack");
    std::fs::create_dir_all(&unpack).unwrap();
    std::fs::write(
        unpack.join("info.plist"),
        minimal_plist("Snippet Triggers - Getting Started"),
    )
    .unwrap();
    // an invalid zip file is fine: unpacking is skipped because the folder
    // already exists — which is exactly the user's on-disk state
    std::fs::write(dir.join("Snippet Triggers.alfredworkflow"), b"not a zip").unwrap();
    let wfs = crate::workflow::load_workflows(&dir);
    let dupes = wfs.iter().filter(|w| w.name.contains("Snippet")).count();
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(dupes, 1, "snippet workflow listed {} times", dupes);
}

// --- malformed bundles (SPEC §8.3) ----------------------------------------

/// A corrupted workflow bundle must be rejected with a diagnostic, never
/// partially loaded.
#[test]
fn malformed_workflow_bundle_is_rejected() {
    let bad = workflow_dir("badbundle", "this is not a property list at all");
    let err = Workflow::load_dir(&bad).unwrap_err();
    std::fs::remove_dir_all(&bad).ok();
    assert!(!err.is_empty(), "a diagnostic is required, got an empty string");

    // A directory with no info.plist at all is also rejected, and says why.
    let empty = std::env::temp_dir().join(format!("ralfred_wf_none_{}", std::process::id()));
    std::fs::remove_dir_all(&empty).ok();
    std::fs::create_dir_all(&empty).unwrap();
    let err2 = Workflow::load_dir(&empty).unwrap_err();
    std::fs::remove_dir_all(&empty).ok();
    assert!(err2.contains("info.plist"), "diagnostic: {err2}");
}

// --- multi-keyword dispatch (real workflow bug) ----------------------------

/// A workflow may carry several keyword inputs; Alfred dispatches on any of
/// them. Registering only the first leaves the others permanently unreachable —
/// the reported "workflow doesn't work" symptom.
#[test]
fn every_keyword_input_is_dispatchable() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
  <key>bundleid</key><string>com.example.multikw</string>
  <key>name</key><string>Multi Keyword</string>
  <key>connections</key><dict></dict>
  <key>objects</key><array>
    <dict><key>uid</key><string>K1</string>
      <key>type</key><string>alfred.workflow.input.keyword</string>
      <key>config</key><dict><key>keyword</key><string>2show</string></dict></dict>
    <dict><key>uid</key><string>K2</string>
      <key>type</key><string>alfred.workflow.input.keyword</string>
      <key>config</key><dict><key>keyword</key><string>show</string></dict></dict>
  </array>
</dict>
</plist>"#;
    let dir = workflow_dir("multikw", xml);
    let wf = Workflow::load_dir(&dir).expect("workflow loads");
    std::fs::remove_dir_all(&dir).ok();

    assert_eq!(
        wf.keywords,
        vec!["2show".to_string(), "show".to_string()],
        "every keyword must be registered"
    );
    assert_eq!(wf.keyword.as_deref(), Some("2show"), "first keyword kept");

    // Both keywords must produce a row; an unknown one must not.
    assert_eq!(wf.run_keyword_for("2show", "abc").0.len(), 1);
    assert_eq!(wf.run_keyword_for("show", "abc").0.len(), 1);
    assert!(wf.run_keyword_for("nope", "abc").0.is_empty());
    // The back-compat entry point still uses the first keyword.
    assert_eq!(wf.run_keyword("abc").len(), 1);
    // A keyword input with no visible title must not yield a blank row.
    assert!(
        !wf.run_keyword_for("show", "").0[0].title.is_empty(),
        "a keyword input must produce a non-empty title"
    );
}
