//! SQLite stores, schema-compatible with Alfred's `.alfdb` (from live schema dumps).
//! Keeps the same table names/columns so migration/import is trivial.

use rusqlite::{params, Connection};
use std::path::PathBuf;
use std::sync::Mutex;

pub struct Store {
    conn: Mutex<Connection>,
}

fn open(name: &str) -> rusqlite::Result<Connection> {
    let dir = crate::config::data_dir();
    std::fs::create_dir_all(&dir).ok();
    let conn = Connection::open(dir.join(name))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    Ok(conn)
}

impl Store {
    pub fn knowledge() -> Self {
        let conn = open("knowledge.alfdb").unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS knowledge(item TEXT, keyword TEXT, ts DECIMAL, hidden INTEGER DEFAULT 0);
             CREATE INDEX IF NOT EXISTS knowledge_item ON knowledge(item);
             CREATE INDEX IF NOT EXISTS knowledge_keyword ON knowledge(keyword);
             CREATE INDEX IF NOT EXISTS knowledge_ts ON knowledge(ts);
             CREATE TABLE IF NOT EXISTS latching(item TEXT, keyword TEXT, strong DECIMAL, ts DECIMAL);",
        ).unwrap();
        Self { conn: Mutex::new(conn) }
    }

    /// (keyword, snippet) for rows with autoexpand=1.
    pub fn autoexpand_snippets(&self) -> Vec<(String, String)> {
        let c = self.conn.lock().unwrap();
        let mut st = match c.prepare("SELECT keyword, snippet FROM snippets WHERE autoexpand=1") {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map(|x| x.filter_map(Result::ok).collect())
            .unwrap_or_default()
    }

    pub fn snippets() -> Self {
        let conn = open("snippets.alfdb").unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS snippets(uid TEXT PRIMARY KEY, name TEXT, keyword TEXT, snippet TEXT, snippetrtf BLOB, collection TEXT, autoexpand BOOLEAN, ignoredynamicplaceholders BOOLEAN);
             CREATE INDEX IF NOT EXISTS snippets_keyword ON snippets(keyword);
             CREATE INDEX IF NOT EXISTS snippets_autoexpand ON snippets(autoexpand);
             CREATE TABLE IF NOT EXISTS triggers(keyword TEXT, triggeruid TEXT, workflowuid TEXT);",
        ).unwrap();
        Self { conn: Mutex::new(conn) }
    }

    pub fn clipboard() -> Self {
        let conn = open("clipboard.alfdb").unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS clipboard(item TEXT, ts DECIMAL, app TEXT, apppath TEXT, dataType INTEGER, dataHash TEXT);
             CREATE INDEX IF NOT EXISTS clipboard_ts ON clipboard(ts);",
        ).unwrap();
        Self { conn: Mutex::new(conn) }
    }

    pub fn urlhistory() -> Self {
        let conn = open("urlhistory.alfdb").unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS history(item TEXT, flatItem TEXT, ts DECIMAL);",
        ).unwrap();
        Self { conn: Mutex::new(conn) }
    }

    pub fn filecache() -> Self {
        let conn = open("filecache.alfdb").unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS files(path TEXT, fileName TEXT, name TEXT, nameSearch TEXT, nameSplit TEXT, nameChars TEXT, lastUsed INTEGER, keywords TEXT, altnames TEXT, filetype INTEGER);
             CREATE INDEX IF NOT EXISTS files_path ON files(path);
             CREATE INDEX IF NOT EXISTS files_nameSearch ON files(nameSearch);",
        ).unwrap();
        Self { conn: Mutex::new(conn) }
    }

    /// Record a result being selected / used (drives ranking).
    pub fn record_use(&self, item: &str, keyword: &str) {
        let c = self.conn.lock().unwrap();
        let now = now_ts();
        c.execute(
            "INSERT INTO knowledge(item, keyword, ts) VALUES(?1, ?2, ?3)",
            params![item, keyword, now],
        ).ok();
    }

    /// Latest usage timestamp per item for keywords starting with prefix.
    pub fn last_used_prefix(&self, prefix: &str) -> Vec<(String, f64)> {
        let c = self.conn.lock().unwrap();
        let mut st = c
            .prepare("SELECT item, MAX(ts) FROM knowledge WHERE keyword LIKE ?1 GROUP BY item ORDER BY 2 DESC")
            .unwrap();
        st.query_map(params![format!("{prefix}%")], |r| Ok((r.get(0)?, r.get(1)?)))
            .map(|x| x.filter_map(Result::ok).collect())
            .unwrap_or_default()
    }

    /// Latest usage timestamp per item for a keyword (recency signal).
    pub fn last_used(&self, keyword: &str) -> Vec<(String, f64)> {
        let c = self.conn.lock().unwrap();
        let mut st = c
            .prepare("SELECT item, MAX(ts) FROM knowledge WHERE keyword=?1 GROUP BY item")
            .unwrap();
        st.query_map(params![keyword], |r| Ok((r.get(0)?, r.get(1)?)))
            .map(|x| x.filter_map(Result::ok).collect())
            .unwrap_or_default()
    }

    pub fn hide_item(&self, item: &str, keyword: &str) {
        let c = self.conn.lock().unwrap();
        c.execute("INSERT INTO knowledge(item,keyword,ts,hidden) VALUES(?1,?2,?3,1)", params![item, keyword, now_ts()]).ok();
    }
}

pub struct Snippet {
    pub uid: String,
    pub name: String,
    pub keyword: String,
    pub text: String,
    pub collection: String,
    pub autoexpand: bool,
}

impl Store {
    pub fn add_snippet(&self, keyword: &str, text: &str, name: &str) -> String {
        let uid = uuid_v4();
        let c = self.conn.lock().unwrap();
        c.execute("INSERT INTO snippets(uid,name,keyword,snippet,collection,autoexpand) VALUES(?1,?2,?3,?4,'default',1)",
            params![uid, name, keyword, text]).unwrap();
        uid
    }

    pub fn find_snippets(&self, prefix: &str) -> Vec<Snippet> {
        let c = self.conn.lock().unwrap();
        let mut st = c.prepare("SELECT uid,name,keyword,snippet,collection,autoexpand FROM snippets WHERE keyword LIKE ?1").unwrap();
        st.query_map(params![format!("{prefix}%")], |r| Ok(Snippet {
            uid: r.get(0)?, name: r.get(1)?, keyword: r.get(2)?, text: r.get(3)?,
            collection: r.get(4)?, autoexpand: r.get::<_, i64>(5)? != 0,
        })).map(|x| x.filter_map(Result::ok).collect()).unwrap_or_default()
    }

    pub fn all_snippets(&self) -> Vec<Snippet> {
        let c = self.conn.lock().unwrap();
        let mut st = c.prepare("SELECT uid,name,keyword,snippet,collection,autoexpand FROM snippets").unwrap();
        st.query_map([], |r| Ok(Snippet {
            uid: r.get(0)?, name: r.get(1)?, keyword: r.get(2)?, text: r.get(3)?,
            collection: r.get(4)?, autoexpand: r.get::<_, i64>(5)? != 0,
        })).map(|x| x.filter_map(Result::ok).collect()).unwrap_or_default()
    }

    pub fn clipboard_push(&self, item: &str, app: &str) {
        let c = self.conn.lock().unwrap();
        let hash = simple_hash(item);
        c.execute("INSERT INTO clipboard(item,ts,app,apppath,dataType,dataHash) VALUES(?1,?2,?3,'',0,?4)", params![item, now_ts(), app, hash]).unwrap();
        c.execute("DELETE FROM clipboard WHERE rowid NOT IN (SELECT rowid FROM clipboard ORDER BY ts DESC LIMIT ?1)", params![500i64]).ok();
    }

    /// Insert immediately, return rowid (0 on failure) — for async attribution.
    pub fn clipboard_push_now(&self, item: &str) -> i64 {
        let c = self.conn.lock().unwrap();
        let hash = simple_hash(item);
        match c.execute("INSERT INTO clipboard(item,ts,app,apppath,dataType,dataHash) VALUES(?1,?2,'',NULL,0,?3)", params![item, now_ts(), hash]) {
            Ok(_) => {
                c.execute("DELETE FROM clipboard WHERE rowid NOT IN (SELECT rowid FROM clipboard ORDER BY ts DESC LIMIT ?1)", params![500i64]).ok();
                c.last_insert_rowid()
            }
            Err(_) => 0,
        }
    }

    pub fn clipboard_set_app(&self, rowid: i64, app: &str) {
        let c = self.conn.lock().unwrap();
        c.execute("UPDATE clipboard SET app=?2 WHERE rowid=?1", params![rowid, app]).ok();
    }

    pub fn clipboard_recent(&self, n: usize) -> Vec<(String, f64)> {
        let c = self.conn.lock().unwrap();
        let mut st = c.prepare("SELECT item, ts FROM clipboard ORDER BY ts DESC LIMIT ?1").unwrap();
        st.query_map(params![n as i64], |r| Ok((r.get(0)?, r.get(1)?))).map(|x| x.filter_map(Result::ok).collect()).unwrap_or_default()
    }

    pub fn clipboard_search(&self, q: &str, n: usize) -> Vec<(String, f64)> {
        let c = self.conn.lock().unwrap();
        let mut st = c.prepare("SELECT item, ts FROM clipboard WHERE item LIKE ?1 ORDER BY ts DESC LIMIT ?2").unwrap();
        st.query_map(params![format!("%{q}%"), n as i64], |r| Ok((r.get(0)?, r.get(1)?))).map(|x| x.filter_map(Result::ok).collect()).unwrap_or_default()
    }

    pub fn url_record(&self, url: &str) {
        let c = self.conn.lock().unwrap();
        c.execute("INSERT INTO history(item,flatItem,ts) VALUES(?1,?2,?3)", params![url, url, now_ts()]).ok();
    }

    pub fn url_history(&self, q: &str, n: usize) -> Vec<(String, f64)> {
        let c = self.conn.lock().unwrap();
        let mut st = c.prepare("SELECT item, ts FROM history WHERE item LIKE ?1 ORDER BY ts DESC LIMIT ?2").unwrap();
        st.query_map(params![format!("%{q}%"), n as i64], |r| Ok((r.get(0)?, r.get(1)?))).map(|x| x.filter_map(Result::ok).collect()).unwrap_or_default()
    }
}

pub fn now_ts() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

fn simple_hash(s: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

pub fn uuid_v4() -> String {
    let mut b = [0u8; 16];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        use std::io::Read;
        let _ = f.read_exact(&mut b);
    } else {
        for x in b.iter_mut() {
            *x = rand_u8();
        }
    }
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7], b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]
    )
}

fn rand_u8() -> u8 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let mut s = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64;
    s ^= s >> 13;
    s = s.wrapping_mul(6364136223846793005);
    ((s >> 32) & 0xff) as u8
}

pub fn filecache_upsert(path: &str, name_search: &str, filetype: i32) {
    let conn = Connection::open(crate::config::data_dir().join("filecache.alfdb")).unwrap_or_else(|_| Connection::open_in_memory().unwrap());
    conn.execute(
        "INSERT INTO files(path,fileName,name,nameSearch,lastUsed,filetype) VALUES(?1,?2,?2,?2,?3,?4)
         ON CONFLICT(path) DO UPDATE SET lastUsed=excluded.lastUsed",
        params![path, name_search, now_ts() as i64, filetype],
    ).ok();
}
