use std::fs;
use std::path::PathBuf;

fn plist_for(two_node: bool) -> String {
    let open = if two_node {
        r#"<dict>
            <key>config</key>
            <dict>
                <key>paths</key>
                <array><string>{query}</string></array>
                <key>withspace</key><true/>
            </dict>
            <key>type</key><string>alfred.workflow.action.openfile</string>
            <key>uid</key><string>OPEN1</string>
        </dict>"#
    } else {
        ""
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>name</key><string>GraphTest</string>
    <key>objects</key>
    <array>
        <dict>
            <key>config</key>
            <dict>
                <key>keyword</key><string>gtest</string>
                <key>script</key><string>echo '{{"items":[{{"title":"T","arg":"X"}}]}}'</string>
                <key>type</key><integer>1</integer>
            </dict>
            <key>type</key><string>alfred.scriptfilter</string>
            <key>uid</key><string>SF1</string>
        </dict>{open}
    </array>
    <key>connections</key>
    <dict>
        <key>SF1</key>
        <array>
            <dict>
                <key>destinationuid</key><string>OPEN1</string>
                <key>modifiers</key><integer>0</integer>
            </dict>
        </array>
    </dict>
</dict>
</plist>"#
    )
}

fn write_wf(dir: &PathBuf, two_node: bool) {
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join("info.plist"), plist_for(two_node)).unwrap();
}

#[test]
fn graph_without_action_node() {
    let d = std::env::temp_dir().join(format!("ralfred_g1_{}", std::process::id()));
    write_wf(&d, false);
    let wf = crate::workflow::Workflow::load_dir(&d).unwrap();
    let res = wf.run_keyword("q");
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].title, "T");
    assert_eq!(res[0].action, None); // OPEN1 missing -> no action attached
    let _ = fs::remove_dir_all(d);
}

#[test]
fn graph_with_openfile_action() {
    let d = std::env::temp_dir().join(format!("ralfred_g2_{}", std::process::id()));
    write_wf(&d, true);
    let wf = crate::workflow::Workflow::load_dir(&d).unwrap();
    let res = wf.run_keyword("q");
    assert_eq!(res.len(), 1);
    let a = res[0].action.clone().expect("action should be attached via graph");
    assert!(a.starts_with("alfred.workflow.action.openfile:"), "got {a}");
    let _ = fs::remove_dir_all(d);
}
