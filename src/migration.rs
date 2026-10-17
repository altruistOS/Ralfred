//! Migration: import from a real Alfred 5 install — workflows and databases.
//! Data formats are schema-compatible by design, so this is copy + path fixup.
//! Never mutates the Alfred side (read-only import).

use std::path::{Path, PathBuf};

pub fn alfred_base() -> PathBuf {
    if let Ok(custom) = std::env::var("RALFRED_ALFRED_HOME") {
        return PathBuf::from(custom);
    }
    dirs::home_dir()
        .unwrap_or_default()
        .join("Library/Application Support/Alfred")
}

#[derive(Default, Debug)]
pub struct MigrationReport {
    pub workflows_copied: usize,
    pub workflows_skipped: usize,
    pub db_rows_imported: usize,
    pub errors: Vec<String>,
}

pub fn migrate(dry_run: bool) -> MigrationReport {
    let mut rep = MigrationReport::default();
    let src = alfred_base();
    if !src.exists() {
        rep.errors.push("Alfred 5 data dir not found".into());
        return rep;
    }

    // 1. Workflows. Alfred 5 keeps them as directories at
    // `<alfredpreferences>/preferences/workflows/<uid>/info.plist` (observed
    // layout string `workflows/%@/info.plist`), and also accepts them as
    // `.alfredworkflow` archives. Discovery is by content marker, not by path,
    // so a custom preferences root still works.
    let wf_dirs = find_workflow_dirs(&src);
    let wf_zips = find_workflow_zips(&src);
    let dst_root = crate::config::workflows_dir();
    if !dry_run {
        std::fs::create_dir_all(&dst_root).ok();
    }
    for s in &wf_dirs {
        let label = s
            .file_name()
            .map(|x| x.to_string_lossy().to_string())
            .unwrap_or_else(|| "wf".into());
        install_workflow(s, &label, &dst_root, dry_run, &mut rep);
    }
    // An archive is unpacked into OUR data dir, never next to the original:
    // the import must not write anything into the Alfred tree.
    let scratch = crate::config::data_dir().join("migration-scratch");
    for z in &wf_zips {
        let label = z
            .file_stem()
            .map(|x| x.to_string_lossy().to_string())
            .unwrap_or_else(|| "wf".into());
        let dst = dst_root.join(format!("migrated-{label}"));
        if dst.exists() {
            rep.workflows_skipped += 1;
            continue;
        }
        if dry_run {
            rep.workflows_copied += 1;
            continue;
        }
        let unpack = scratch.join(&label);
        if std::fs::create_dir_all(&scratch).is_err() {
            rep.errors.push(format!("workflow archive {label}: cannot create scratch dir"));
            continue;
        }
        std::fs::remove_dir_all(&unpack).ok();
        match crate::workflow::unzip_to(z, &unpack) {
            Ok(()) => install_workflow(&unpack, &label, &dst_root, false, &mut rep),
            Err(e) => rep
                .errors
                .push(format!("workflow archive {}: {e}", z.display())),
        }
    }

    // 2. Databases: copy rows from Alfred's alfdb files (identical schema).
    // (alfdb file, our db name, source table, INSERT ... SELECT across the
    // ATTACHed source — table refs must be `src_alias.<table>`, not the alias)
    let dbs = [
        ("snippets", "snippets", "snippets",
         "INSERT OR IGNORE INTO snippets(uid,name,keyword,snippet,snippetrtf,collection,autoexpand,ignoredynamicplaceholders)           SELECT uid,name,keyword,snippet,snippetrtf,collection,autoexpand,ignoredynamicplaceholders FROM src.snippets"),
        ("clipboard", "clipboard", "clipboard",
         "INSERT OR IGNORE INTO clipboard(item,ts,app,apppath,dataType,dataHash)           SELECT item,ts,app,apppath,dataType,dataHash FROM src.clipboard"),
        ("urlhistory", "urlhistory", "history",
         "INSERT OR IGNORE INTO history(item,flatItem,ts)           SELECT item,flatItem,ts FROM src.history"),
    ];
    if !dry_run {
        crate::db::Store::snippets();
        crate::db::Store::clipboard();
        crate::db::Store::urlhistory();
    }
    for (file, dst_name, table, sql) in dbs {
        let src_db = src.join("Databases").join(format!("{file}.alfdb"));
        if !src_db.exists() {
            continue;
        }
        if dry_run {
            rep.db_rows_imported += count_rows(&src_db, table);
            continue;
        }
        let dst_db = crate::config::data_dir().join(format!("{dst_name}.alfdb"));
        match import_db(&src_db, &dst_db, sql) {
            Ok(n) => rep.db_rows_imported += n,
            Err(e) => rep.errors.push(format!("db {file}: {e}")),
        }
    }
    rep
}

/// Copy one discovered workflow directory into the ralfred workflow store.
/// Idempotent: an already-present destination is counted as skipped.
fn install_workflow(
    src_dir: &Path,
    label: &str,
    dst_root: &Path,
    dry_run: bool,
    rep: &mut MigrationReport,
) {
    let dst = dst_root.join(format!("migrated-{label}"));
    if dst.exists() {
        rep.workflows_skipped += 1;
        return;
    }
    if dry_run {
        rep.workflows_copied += 1;
        return;
    }
    match copy_dir_all(src_dir, &dst) {
        Ok(_) => rep.workflows_copied += 1,
        Err(e) => rep.errors.push(format!("workflow {label}: {e}")),
    }
}

/// Workflow directories under `root`: any directory holding a top-level
/// `info.plist`. Alfred 5 stores these at
/// `<alfredpreferences>/preferences/workflows/<uid>/info.plist` (observed
/// layout string `workflows/%@/info.plist`), so discovery must not filter on
/// the `preferences/` path segment. A workflow directory is not descended into.
fn find_workflow_dirs(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if !p.is_dir() {
                continue;
            }
            if p.join("info.plist").exists() {
                out.push(p);
            } else {
                stack.push(p);
            }
        }
    }
    out.sort();
    out
}

/// `.alfredworkflow` archives under `root`. These are files, so they are found
/// by a file walk rather than by the directory branch above.
fn find_workflow_zips(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().map(|x| x == "alfredworkflow").unwrap_or(false) {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

fn count_rows(src_db: &Path, table: &str) -> usize {
    rusqlite::Connection::open_with_flags(src_db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .and_then(|c| c.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get::<_, i64>(0)))
        .map(|n| n as usize)
        .unwrap_or(0)
}

fn import_db(src_db: &Path, dst_db: &Path, insert_sql: &str) -> Result<usize, String> {
    let src = rusqlite::Connection::open_with_flags(
        src_db,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|e| e.to_string())?;
    let _ = src;
    let dst = rusqlite::Connection::open(dst_db).map_err(|e| e.to_string())?;
    dst.execute_batch(&format!(
        "ATTACH DATABASE '{}' AS src;",
        src_db.display().to_string().replace('\'', "''")
    ))
    .map_err(|e| format!("attach: {e}"))?;
    let n = dst.execute(insert_sql, []).map_err(|e| format!("insert: {e}"))?;
    dst.execute_batch("DETACH DATABASE src;").ok();
    let _ = src;
    Ok(n)
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for e in std::fs::read_dir(src)? {
        let e = e?;
        let p = e.path();
        let d = dst.join(e.file_name());
        if p.is_dir() {
            copy_dir_all(&p, &d)?;
        } else {
            std::fs::copy(&p, &d)?;
        }
    }
    Ok(())
}
