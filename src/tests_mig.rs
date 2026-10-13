use std::path::PathBuf;

#[test]
fn migrate_from_fake_home() {
    let root = std::env::temp_dir().join(format!("ralfred_mig_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("Databases")).unwrap();
    let wfdir = root.join("Alfred.alfredpreferences/workflows/AAAA1111-2222-4333-8444-555566667777");
    std::fs::create_dir_all(&wfdir).unwrap();
    let info = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>name</key><string>MigWF</string><key>connections</key><dict/>
<key>objects</key><array><dict>
<key>config</key><dict><key>keyword</key><string>mk</string><key>script</key><string>echo x</string><key>type</key><integer>1</integer></dict>
<key>type</key><string>alfred.scriptfilter</string><key>uid</key><string>S1</string>
</dict></array>
</dict></plist>"#;
    std::fs::write(wfdir.join("info.plist"), info).unwrap();
    let db = rusqlite::Connection::open(root.join("Databases/snippets.alfdb")).unwrap();
    db.execute_batch("CREATE TABLE snippets(uid TEXT PRIMARY KEY, name TEXT, keyword TEXT, snippet TEXT, snippetrtf BLOB, collection TEXT, autoexpand BOOLEAN, ignoredynamicplaceholders BOOLEAN); INSERT INTO snippets VALUES('x1','n','mkq','hello',NULL,'default',1,0);").unwrap();
    drop(db);
    std::env::set_var("RALFRED_ALFRED_HOME", &root);
    let rep = crate::migration::migrate(false);
    assert_eq!(rep.workflows_copied, 1, "wf report {:?}", rep.errors);
    assert!(rep.errors.is_empty(), "errors {:?}", rep.errors);
    assert_eq!(rep.db_rows_imported, 1);
    // dry run is idempotent-safe
    let rep2 = crate::migration::migrate(true);
    assert_eq!(rep2.workflows_skipped, 1);
    // cleanup side effects on the real store (test hygiene)
    let store_db = crate::config::data_dir().join("snippets.alfdb");
    if let Ok(c) = rusqlite::Connection::open(&store_db) {
        c.execute("DELETE FROM snippets WHERE keyword='mkq'", []).ok();
    }
    let _ = std::fs::remove_dir_all(crate::config::workflows_dir().join("migrated-AAAA1111-2222-4333-8444-555566667777"));
    std::env::remove_var("RALFRED_ALFRED_HOME");
    let _ = std::fs::remove_dir_all(&root);
}
