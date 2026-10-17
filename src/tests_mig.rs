use std::path::PathBuf;

#[test]
fn migrate_from_fake_home() {
    // env is process-global; serialize with the other env-touching tests
    let _lock = crate::tests::env_lock();
    let root = std::env::temp_dir().join(format!("ralfred_mig_{}", std::process::id()));
    std::env::remove_var("RALFRED_ALFRED_HOME");
    std::env::remove_var("RALFRED_HOME");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("Databases")).unwrap();
    // Alfred 5's real layout: <alfredpreferences>/preferences/workflows/<uid>/
    let wfdir = root
        .join("Alfred.alfredpreferences/preferences/workflows/AAAA1111-2222-4333-8444-555566667777");
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
    // destination must be isolated too — otherwise this test writes into the
    // real app-support dir and a leftover from a previous run makes it skip.
    let dst = std::env::temp_dir().join(format!("ralfred_migdstd_{}", std::process::id()));
    std::env::set_var("RALFRED_HOME", &dst);
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

/// A `.alfredworkflow` archive must be discovered (it is a file, not a
/// directory), imported, and unpacked somewhere other than the Alfred tree —
/// the import is read-only against the source.
#[test]
fn migrate_from_workflow_archive_leaves_source_untouched() {
    use std::io::Write;
    let _lock = crate::tests::env_lock();
    let root = std::env::temp_dir().join(format!("ralfred_migzip_{}", std::process::id()));
    let dst = std::env::temp_dir().join(format!("ralfred_migzipdst_{}", std::process::id()));
    std::env::remove_var("RALFRED_ALFRED_HOME");
    std::env::remove_var("RALFRED_HOME");
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&dst);
    std::fs::create_dir_all(root.join("Databases")).unwrap();

    let info = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>bundleid</key><string>com.example.zipwf</string>
<key>name</key><string>ZipWF</string>
<key>connections</key><dict/>
<key>objects</key><array/>
</dict></plist>"#;
    let zip_path = root.join("Zipped Thing.alfredworkflow");
    {
        let f = std::fs::File::create(&zip_path).unwrap();
        let mut zw = zip::ZipWriter::new(f);
        let opts = zip::write::SimpleFileOptions::default();
        zw.start_file("info.plist", opts).unwrap();
        zw.write_all(info.as_bytes()).unwrap();
        zw.finish().unwrap();
    }

    std::env::set_var("RALFRED_ALFRED_HOME", &root);
    std::env::set_var("RALFRED_HOME", &dst);
    let rep = crate::migration::migrate(false);
    assert_eq!(rep.workflows_copied, 1, "report {rep:?}");
    assert!(rep.errors.is_empty(), "errors {:?}", rep.errors);
    // The archive was imported…
    assert!(crate::config::workflows_dir()
        .join("migrated-Zipped Thing/info.plist")
        .exists());
    // …and the source tree gained nothing (no in-place unpack).
    let stray: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.contains("wfunpack"))
        .collect();
    assert!(stray.is_empty(), "import wrote into the source tree: {stray:?}");

    let _ = std::fs::remove_dir_all(crate::config::workflows_dir().join("migrated-Zipped Thing"));
    std::env::remove_var("RALFRED_ALFRED_HOME");
    std::env::remove_var("RALFRED_HOME");
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&dst);
}
