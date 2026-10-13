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

    // 1. Workflows: Alfred keeps them as dirs inside alfredpreferences (v5 layout:
    // preferences/workflows/<uuid>/info.plist) — discover by info.plist marker.
    let wf_srcs = find_workflows(&src);
    let dst_root = crate::config::workflows_dir();
    if !dry_run {
        std::fs::create_dir_all(&dst_root).ok();
    }
    for s in wf_srcs {
        let name = s.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_else(|| "wf".into());
        let dst = dst_root.join(format!("migrated-{name}"));
        if dst.exists() {
            rep.workflows_skipped += 1;
            continue;
        }
        if dry_run {
            rep.workflows_copied += 1;
            continue;
        }
        match copy_dir_all(&s, &dst) {
            Ok(_) => rep.workflows_copied += 1,
            Err(e) => rep.errors.push(format!("workflow {name}: {e}")),
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

fn find_workflows(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        if let Ok(rd) = std::fs::read_dir(&d) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    if p.join("info.plist").exists() && !p.to_string_lossy().contains("Alfred.alfredpreferences/preferences") {
                        // workflow dir candidate (info.plist at top level)
                        let n = p.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
                        if n.len() >= 8 || n.contains(".alfred") {
                            out.push(p);
                            continue;
                        }
                    }
                    if p.extension().map(|x| x == "alfredworkflow").unwrap_or(false) {
                        // zipped workflow: unpack in place for import list
                        let unpack = p.with_extension("wfunpack");
                        if !unpack.exists() && crate::workflow::unzip_to(&p, &unpack).is_ok() {
                            out.push(unpack);
                        } else if unpack.exists() {
                            out.push(unpack);
                        }
                    } else {
                        stack.push(p);
                    }
                }
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
