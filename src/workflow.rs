//! Workflow engine — `.alfredworkflow` / unpacked workflow dirs, reading
//! Alfred's `info.plist` format (verified against live sample workflows).
//! Connection graph: {uid: [{destinationuid, modifiers}]}  object types we run:
//!   scriptfilter (keyword+script), filter, conditional, output, openfile,
//!   jabascript omitted. Script filter protocol = env vars + JSON items out.

use crate::search::{Candidate, Kind};
use plist::Value;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

/// One decoded object (node) of a workflow graph: its UID, the `type` string
/// from the `connections` map, and that object's configuration dictionary.
#[derive(Debug, Clone)]
pub struct WfObj {
    pub uid: String,
    pub typ: String,
    pub config: HashMap<String, plist::Value>,
}

#[derive(Debug, Clone)]
pub struct Workflow {
    pub uid: String, // folder name
    pub bundleid: String, // info.plist `bundleid`, falling back to the folder name
    pub name: String,
    pub description: String,
    pub version: String,
    pub keyword: Option<String>, // first keyword (kept for listing/back-compat)
    /// Every keyword this workflow answers to, in object order. A workflow may
    /// carry several keyword inputs (real example: "2show" *and* "show");
    /// Alfred dispatches on any of them, so registering only the first leaves
    /// the remaining keyword inputs permanently unreachable.
    pub keywords: Vec<String>,
    pub objects: Vec<WfObj>,
    pub connections: HashMap<String, Vec<(String, i64)>>, // from -> [(to, modifiers)]
    pub dir: PathBuf,
    /// info.plist `variables` — exposed to scripts as ALFRED_WORKFLOW_VARIABLE_<K>
    /// and substituted as {var:K} (evidence: alfred_workflow_variable_* strings).
    pub variables: HashMap<String, String>,
    /// Load-time problems: connections naming an object that does not exist in
    /// this workflow (SPEC §8.2). Those edges are dropped; the graph still runs.
    pub warnings: Vec<String>,
}

/// Interface version emulated for third-party workflows, which commonly gate on
/// `alfred_version`. This is the studied target's version, not ralfred's own.
pub const ALFRED_COMPAT_VERSION: &str = "5.8";
pub const ALFRED_COMPAT_BUILD: &str = "2348";
/// Panel width reported to scripts as `alfred_theme_window_width`.
pub const PANEL_WIDTH: u32 = 700;

pub fn load_workflows(dir: &Path) -> Vec<Workflow> {
    let mut out = Vec::new();
    if !dir.exists() {
        return out;
    }
    let entries: Vec<_> = std::fs::read_dir(dir)
        .map(|rd| rd.filter_map(Result::ok).collect())
        .unwrap_or_default();
    let mut dirs: Vec<PathBuf> = entries
        .iter()
        .filter_map(|e| {
            let p = e.path();
            if p.is_dir() {
                Some(p)
            } else if p.extension().map(|x| x == "alfredworkflow").unwrap_or(false) {
                // unpack zipped workflow next to it once
                let unpack = p.with_extension("wfunpack");
                if !unpack.exists() {
                    let _ = unzip_to(&p, &unpack);
                }
                Some(unpack)
            } else {
                None
            }
        })
        .collect();
    // a `.alfredworkflow` unpacks next to itself into `<name>.wfunpack`; if
    // that folder also sits in the listing we would show the same workflow
    // twice (the user's "two Snippet Triggers") — dedupe by path.
    dirs.sort();
    dirs.dedup();
    for d in dirs {
        if let Ok(wf) = Workflow::load_dir(&d) {
            out.push(wf);
        }
    }
    out
}

pub fn unzip_to(zip_path: &Path, dest: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dest)?;
    let file = std::fs::File::open(zip_path)?;
    let mut z = zip::ZipArchive::new(file).map_err(|e| std::io::Error::other(e))?;
    for i in 0..z.len() {
        let mut f = z.by_index(i).map_err(|e| std::io::Error::other(e))?;
        let name = f.name().to_string();
        if name.contains("..") {
            continue;
        }
        let outpath = dest.join(name);
        if f.is_dir() {
            std::fs::create_dir_all(&outpath)?;
        } else {
            if let Some(p) = outpath.parent() {
                std::fs::create_dir_all(p)?;
            }
            let mut buf = Vec::new();
            f.read_to_end(&mut buf)?;
            std::fs::write(&outpath, buf)?;
        }
    }
    Ok(())
}

/// Machine-local identifier, mirroring the `localhash` value Alfred keeps in its
/// `prefs.json` pointer file. A legacy Alfred pointer file wins when present;
/// otherwise ralfred's own data root; otherwise empty.
fn localhash() -> String {
    let pointer = crate::config::app_support_dir().join("prefs.json");
    if let Ok(s) = std::fs::read_to_string(&pointer) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
            if let Some(h) = v.get("localhash").and_then(|x| x.as_str()) {
                if !h.is_empty() {
                    return h.to_string();
                }
            }
        }
    }
    std::fs::read_to_string(crate::config::data_dir().join("localhash"))
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

impl Workflow {
    pub fn load_dir(dir: &Path) -> Result<Self, String> {
        let plist_path = dir.join("info.plist");
        if !plist_path.exists() {
            return Err("no info.plist".into());
        }
        let v = Value::from_file(&plist_path).map_err(|e| e.to_string())?;
        let d = v.as_dictionary().ok_or("not a dict")?;

        let mut objects = Vec::new();
        if let Some(arr) = d.get("objects").and_then(|v| v.as_array()) {
            for o in arr {
                if let Some(od) = o.as_dictionary() {
                    let uid = od
                        .get("uid")
                        .and_then(|x| x.as_string())
                        .unwrap_or("")
                        .to_string();
                    let typ = od
                        .get("type")
                        .and_then(|x| x.as_string())
                        .unwrap_or("unknown")
                        .to_string();
                    let mut config = HashMap::new();
                    if let Some(c) = od.get("config").and_then(|x| x.as_dictionary()) {
                        for (k, val) in c.iter() {
                            config.insert(k.clone(), val.clone());
                        }
                    }
                    objects.push(WfObj { uid, typ, config });
                }
            }
        }
        let mut connections: HashMap<String, Vec<(String, i64)>> = HashMap::new();
        if let Some(c) = d.get("connections").and_then(|x| x.as_dictionary()) {
            for (from, conns) in c.iter() {
                if let Some(arr) = conns.as_array() {
                    for e in arr {
                        if let Some(ed) = e.as_dictionary() {
                            let dst = ed
                                .get("destinationuid")
                                .and_then(|x| x.as_string())
                                .unwrap_or("")
                                .to_string();
                            let mods = ed
                                .get("modifiers")
                                .and_then(|x| x.as_signed_integer())
                                .unwrap_or(0) as i64;
                            connections
                                .entry(from.clone())
                                .or_default()
                                .push((dst, mods));
                        }
                    }
                }
            }
        }
        // SPEC §8.2 — an edge naming an object that does not exist is dropped
        // and reported; the rest of the graph still runs.
        let known: std::collections::HashSet<&str> =
            objects.iter().map(|o| o.uid.as_str()).collect();
        let mut warnings = Vec::new();
        for (from, edges) in connections.iter_mut() {
            let before = edges.len();
            edges.retain(|(dst, _)| {
                let ok = known.contains(dst.as_str());
                if !ok {
                    warnings.push(format!(
                        "Tidying defunct connection from '{from}' as output uid '{dst}' is missing"
                    ));
                }
                ok
            });
            if edges.len() != before && !known.contains(from.as_str()) {
                warnings.push(format!("connection source uid '{from}' is not an object"));
            }
        }
        let str_of = |key: &str| -> String {
            d.get(key)
                .and_then(|x| x.as_string())
                .unwrap_or("")
                .to_string()
        };
        let name = str_of("name");
        let bundleid = str_of("bundleid");
        let description = str_of("description");
        let version = str_of("version");
        // Collect every keyword-bearing object, in object order, de-duplicated.
        let mut keywords: Vec<String> = Vec::new();
        for o in &objects {
            if let Some(k) = o.config.get("keyword").and_then(|x| x.as_string()) {
                if !k.is_empty() && !keywords.iter().any(|e| e == k) {
                    keywords.push(k.to_string());
                }
            }
        }
        let keyword = keywords.first().cloned();
        let mut variables = HashMap::new();
        if let Some(v) = d.get("variables").and_then(|x| x.as_dictionary()) {
            for (k, val) in v.iter() {
                if let Some(s) = val.as_string() {
                    variables.insert(k.clone(), s.to_string());
                }
            }
        }
        let uid = dir
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let bundleid = if bundleid.is_empty() { uid.clone() } else { bundleid };
        Ok(Workflow {
            uid,
            bundleid,
            name,
            description,
            version,
            keyword,
            keywords,
            objects,
            connections,
            variables,
            warnings,
            dir: dir.to_path_buf(),
        })
    }

    /// Run a keyword-matched workflow with `query`: locate the scriptfilter
    /// carrying the keyword, execute it, then walk the connection graph to
    /// action nodes (openfile/openurl/terminal) and attach the resulting action.
    /// Convenience wrapper that discards diagnostics.
    pub fn run_keyword(&self, query: &str) -> Vec<Candidate> {
        self.run_keyword_report(query).0
    }

    /// Run a keyword-matched workflow, returning the rows plus any diagnostics
    /// (script-filter parse/exec failures per SPEC §6.6 / §8.4). Uses the
    /// workflow's first keyword; use [`Workflow::run_keyword_for`] when the
    /// caller knows which of the workflow's keywords the user typed.
    pub fn run_keyword_report(&self, query: &str) -> (Vec<Candidate>, Vec<String>) {
        match self.keywords.first() {
            Some(k) => self.run_keyword_for(k, query),
            None => (Vec::new(), self.warnings.clone()),
        }
    }

    /// Run the entry point carrying `keyword`: execute it, then walk the
    /// connection graph to action nodes (openfile/openurl/terminal) and attach
    /// the resulting action.
    pub fn run_keyword_for(&self, keyword: &str, query: &str) -> (Vec<Candidate>, Vec<String>) {
        let mut results = Vec::new();
        let mut warnings: Vec<String> = self.warnings.clone();
        let mut entry: Option<&WfObj> = None;
        for o in &self.objects {
            if let Some(k) = o.config.get("keyword").and_then(|x| x.as_string()) {
                if k == keyword {
                    entry = Some(o);
                    break;
                }
            }
        }
        let Some(entry) = entry else { return (results, warnings) };
        // A keyword input often has no visible title of its own; falling back to
        // an empty query produced blank rows for list-filter/keyword inputs.
        let input_title = entry
            .config
            .get("title")
            .and_then(|x| x.as_string())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                if query.is_empty() {
                    self.name.clone()
                } else {
                    query.to_string()
                }
            });
        match entry.typ.as_str() {
            t if t.contains("scriptfilter") => {
                let script = entry
                    .config
                    .get("script")
                    .and_then(|x| x.as_string())
                    .unwrap_or("")
                    .to_string();
                let stype = entry.config.get("type").and_then(|x| x.as_signed_integer()).unwrap_or(2) as i32;
                let (items, w) = self.run_script_filter(&script, stype, query);
                results = items;
                warnings.extend(w);
            }
            t if t.contains("input.keyword") => {
                // keyword entry: arg passes straight through the graph as a single item
                results.push(Candidate {
                    title: input_title.clone(),
                    subtitle: String::new(),
                    path: Some(query.to_string()),
                    score: 100.0,
                    kind: Kind::Action,
                    action: None,
                });
            }
            _ => {
                // trigger.* with keyword: arg passthrough like Alfred's snippet triggers
                results.push(Candidate {
                    title: input_title.clone(),
                    subtitle: String::new(),
                    path: Some(query.to_string()),
                    score: 100.0,
                    kind: Kind::Action,
                    action: None,
                });
            }
        }
        // BFS the graph from entry collecting terminal action nodes (depth<=3)
        let actions = self.collect_actions(&entry.uid);
        for (dst_uid, _mods) in &actions {
            if let Some(node) = self.objects.iter().find(|o| o.uid == *dst_uid) {
                if let Some((prefix, target, config_based)) = self.action_of(node, query) {
                    let mk_tag = |arg: &str| -> String {
                        let t = if config_based {
                            target.clone()
                        } else {
                            arg.to_string()
                        };
                        format!("{prefix}:{t}")
                    };
                    if results.is_empty() {
                        results.push(Candidate {
                            title: format!("{} ({})", node.typ, self.name),
                            subtitle: query.to_string(),
                            path: Some(query.to_string()),
                            score: 100.0,
                            kind: Kind::Action,
                            action: Some(mk_tag(query)),
                        });
                    }
                    for c in &mut results {
                        let arg = c.path.clone().unwrap_or_else(|| query.to_string());
                        c.action = Some(mk_tag(&arg));
                    }
                    break;
                }
            }
        }
        (results, warnings)
    }

    /// Depth-limited BFS over connections from `from`, yielding (uid, modifiers).
    fn collect_actions(&self, from: &str) -> Vec<(String, i64)> {
        let mut seen = std::collections::HashSet::new();
        let mut frontier = vec![from.to_string()];
        let mut out = Vec::new();
        for _ in 0..3 {
            let mut next = Vec::new();
            for f in &frontier {
                for (dst, mods) in self.connections.get(f).cloned().unwrap_or_default() {
                    if seen.insert(dst.clone()) {
                        out.push((dst.clone(), mods));
                        next.push(dst);
                    }
                }
            }
            if next.is_empty() {
                break;
            }
            frontier = next;
        }
        out
    }

    /// Translate an action node into (type-prefix, target, config_based).
    /// config_based=true: target came from node config (Alfred Open-File style);
    /// false: node has no target of its own -> the item's *arg* fills it.
    fn action_of(&self, node: &WfObj, query: &str) -> Option<(String, String, bool)> {
        let t = node.typ.as_str();
        if t.contains("openfile") || t.contains("openurl") {
            let paths = node.config.get("paths").and_then(|x| x.as_array());
            let first = paths
                .and_then(|arr| arr.first())
                .and_then(|x| x.as_string())
                .unwrap_or("")
                .to_string();
            if !first.is_empty() {
                let target = self.subst(&first, query);
                return Some((t.to_string(), target, true));
            }
            return Some((t.to_string(), query.to_string(), false));
        }
        if t.contains("revealfile")
            || t.contains("terminalcommand")
            || t.contains("systemcommand")
            || t.contains("output.clipboard")
            || t.contains("action.script")
        {
            return Some((t.to_string(), query.to_string(), false));
        }
        None
    }

    /// Substitute {query}, {var:NAME} and {NAME} using workflow variables.
    fn subst(&self, s: &str, query: &str) -> String {
        let mut out = s.replace("{query}", query).replace("{q}", query);
        for (k, v) in &self.variables {
            out = out.replace(&format!("{{var:{k}}}", k = k), v);
            out = out.replace(&format!("{{{k}}}", k = k), v);
        }
        out
    }

    fn run_script_filter(&self, script: &str, stype: i32, query: &str) -> (Vec<Candidate>, Vec<String>) {
        // stype per Alfred docs mapping observed: 0=applescript 1=/bin/bash 2=/bin/zsh 3=/bin/sh 4=php 5=ruby 6=python 7=perl 8=python3 ...
        let (shell, interp): (&str, Option<&[&str]>) = match stype {
            1 => ("/bin/bash", Some(&["-c", script])),
            2 => ("/bin/zsh", Some(&["-c", script])),
            3 => ("/bin/sh", Some(&["-c", script])),
            4 => ("php", Some(&["-r", script])),
            5 => ("ruby", Some(["-e"].as_slice())),
            6 => ("python", Some(["-c"].as_slice())),
            7 => ("perl", Some(["-e"].as_slice())),
            8 => ("python3", Some(["-c"].as_slice())),
            _ => ("/bin/zsh", Some(&["-c", script])),
        };
        let mut cmd = Command::new(shell);
        if let Some(args) = interp {
            if stype == 5 {
                // ruby -e uses script via env fallback
                let s = script.to_string();
                cmd.args(["-e", &s]);
            } else if stype == 6 || stype == 8 || stype == 7 {
                let s = script.to_string();
                cmd.args(args).arg(&s);
            } else if stype == 4 {
                let s = script.to_string();
                cmd.args(args).arg(&s);
            } else {
                cmd.args(args);
            }
        }
        cmd.current_dir(&self.dir);
        cmd.env("query", query);
        cmd.env("q", query);
        for (k, v) in &self.variables {
            cmd.env(format!("ALFRED_WORKFLOW_VARIABLE_{k}", k = k.to_uppercase()), v);
        }
        for (k, v) in self.script_env(query) {
            cmd.env(k, v);
        }
        match cmd.output() {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                match parse_script_filter_output_checked(&stdout) {
                    Ok(items) => (items, Vec::new()),
                    Err(e) => (Vec::new(), vec![format!("script filter '{}': {e}", self.name)]),
                }
            }
            Err(e) => (
                Vec::new(),
                vec![format!("script filter '{}' could not run: {e}", self.name)],
            ),
        }
    }

    /// The environment contract exported to workflow scripts (SPEC §6.7).
    /// Returns the observed `alfred_*` names plus ralfred's own identity so a
    /// workflow written for Alfred interoperates unchanged.
    pub fn script_env(&self, _query: &str) -> Vec<(String, String)> {
        let cfg = crate::config::Config::load();
        let data_root = crate::config::data_dir();
        let wf_data = data_root.join("workflow-data").join(&self.bundleid);
        let wf_cache = data_root.join("workflow-cache").join(&self.bundleid);
        let theme = crate::theme::Theme::resolve(&cfg.theme);
        let hex = |s: &str| -> String {
            if s.starts_with('#') {
                s.to_string()
            } else {
                format!("#{s}")
            }
        };
        let mut e: Vec<(String, String)> = vec![
            ("ralfred_version".into(), env!("CARGO_PKG_VERSION").into()),
            // Interface version being emulated: third-party workflows gate on it.
            ("alfred_version".into(), ALFRED_COMPAT_VERSION.into()),
            ("alfred_version_build".into(), ALFRED_COMPAT_BUILD.into()),
            ("alfred_workflow_bundleid".into(), self.bundleid.clone()),
            ("alfred_workflow_uid".into(), self.uid.clone()),
            ("alfred_workflow_name".into(), self.name.clone()),
            ("alfred_workflow_description".into(), self.description.clone()),
            ("alfred_workflow_version".into(), self.version.clone()),
            (
                "alfred_workflow_keyword".into(),
                self.keyword.clone().unwrap_or_default(),
            ),
            ("alfred_workflow_data".into(), wf_data.to_string_lossy().to_string()),
            ("alfred_workflow_cache".into(), wf_cache.to_string_lossy().to_string()),
            (
                "alfred_preferences".into(),
                crate::config::prefs_dir().to_string_lossy().to_string(),
            ),
            ("alfred_preferences_localhash".into(), localhash()),
            (
                "alfred_debug".into(),
                if std::env::var("RALFRED_DEBUG").is_ok() {
                    "1".to_string()
                } else {
                    "0".to_string()
                },
            ),
            ("alfred_theme".into(), theme.name.clone()),
            ("alfred_theme_background".into(), hex(&theme.background)),
            (
                "alfred_theme_selection_background".into(),
                hex(&theme.selection_background),
            ),
            ("alfred_theme_subtext".into(), hex(&theme.subtext)),
            ("alfred_theme_window_width".into(), PANEL_WIDTH.to_string()),
        ];
        for (k, v) in &self.variables {
            e.push((
                format!("ALFRED_WORKFLOW_VARIABLE_{k}", k = k.to_uppercase()),
                v.clone(),
            ));
        }
        e
    }
}

// ---------------------------------------------------------------------------
// Script filter output protocol (SPEC §6.6, §8.4, §8.5)
// ---------------------------------------------------------------------------
/// Accepted `rerun` interval, in seconds (SPEC §6.6). Outside the range the
/// value is ignored rather than honoured.
pub const RERUN_MIN: f64 = 0.1;
pub const RERUN_MAX: f64 = 5.0;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SfIcon {
    pub kind: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SfText {
    pub copy: String,
    pub largetype: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SfMod {
    pub valid: bool,
    pub arg: String,
    pub subtitle: String,
}

/// One script-filter row, with the full field set scripts may emit.
#[derive(Debug, Clone, PartialEq)]
pub struct SfItem {
    pub uid: String,
    pub item_type: String,
    pub title: String,
    pub subtitle: String,
    pub arg: String,
    /// `false` marks a row that is displayed but not actionable.
    pub valid: bool,
    pub autocomplete: String,
    /// `match` — the text used for query filtering instead of the title.
    pub match_text: String,
    pub icon: Option<SfIcon>,
    pub quicklookurl: String,
    pub text: Option<SfText>,
    pub mods: HashMap<String, SfMod>,
}

impl Default for SfItem {
    fn default() -> Self {
        SfItem {
            uid: String::new(),
            item_type: String::new(),
            title: String::new(),
            subtitle: String::new(),
            arg: String::new(),
            valid: true,
            autocomplete: String::new(),
            match_text: String::new(),
            icon: None,
            quicklookurl: String::new(),
            text: None,
            mods: HashMap::new(),
        }
    }
}

/// A script filter's whole response: rows plus session state.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SfOutput {
    pub items: Vec<SfItem>,
    /// Exported back into the script on the next run in the same session.
    pub variables: HashMap<String, String>,
    pub rerun: Option<f64>,
}

/// Strict parse of a script filter's stdout. Accepts the JSON form and the
/// legacy XML form; every other shape produces an actionable diagnostic
/// (SPEC §8.4). Empty/whitespace output is an empty result, not an error.
pub fn parse_script_filter(stdout: &str) -> Result<SfOutput, String> {
    let t = stdout.trim();
    if t.is_empty() {
        return Ok(SfOutput::default());
    }
    if t.starts_with('<') {
        parse_xml_output(t)
    } else {
        parse_json_output(t)
    }
}

/// Lenient wrapper used by the UI paths: a malformed response yields no rows.
/// Use [`parse_script_filter`] when the diagnostic matters.
pub fn parse_script_filter_output(stdout: &str) -> Vec<Candidate> {
    match parse_script_filter(stdout) {
        Ok(o) => o.items.iter().map(sf_item_to_candidate).collect(),
        Err(_) => Vec::new(),
    }
}

/// Like [`parse_script_filter_output`] but surfaces the parse error.
pub fn parse_script_filter_output_checked(stdout: &str) -> Result<Vec<Candidate>, String> {
    parse_script_filter(stdout).map(|o| o.items.iter().map(sf_item_to_candidate).collect())
}

fn sf_item_to_candidate(it: &SfItem) -> Candidate {
    let arg = if it.arg.is_empty() { it.title.clone() } else { it.arg.clone() };
    Candidate {
        title: it.title.clone(),
        subtitle: if it.subtitle.is_empty() { arg.clone() } else { it.subtitle.clone() },
        // A row the script marked invalid has no payload to act on.
        path: if it.valid { Some(arg) } else { None },
        score: 100.0,
        kind: Kind::Action,
        action: None,
    }
}

fn snippet(s: &str) -> String {
    let mut out: String = s.chars().take(200).collect();
    if s.chars().count() > 200 {
        out.push('…');
    }
    out
}

fn json_str(v: Option<&serde_json::Value>) -> String {
    match v {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

fn json_scalar_to_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn parse_json_output(t: &str) -> Result<SfOutput, String> {
    let v: serde_json::Value =
        serde_json::from_str(t).map_err(|e| format!("JSON error: {e} in JSON: {}", snippet(t)))?;
    let obj = v
        .as_object()
        .ok_or_else(|| format!("script filter JSON must be an object, got: {}", snippet(t)))?;
    // Alfred also accepts a single `alfredworkflow` root wrapper.
    let obj = match obj.get("alfredworkflow") {
        Some(serde_json::Value::Object(inner)) if !obj.contains_key("items") => inner,
        _ => obj,
    };
    let has_items = obj.contains_key("items");
    let has_vars = obj.contains_key("variables");
    let has_rerun = obj.contains_key("rerun") || obj.contains_key("rerunafter");
    if !has_items && !has_vars && !has_rerun {
        return Err(format!(
            "JSON is missing expected keys; items, variables or rerunafter: {}",
            snippet(t)
        ));
    }
    let mut out = SfOutput::default();
    if let Some(vars) = obj.get("variables") {
        let m = vars
            .as_object()
            .ok_or_else(|| "'variables' must be an object".to_string())?;
        for (k, val) in m {
            out.variables.insert(k.clone(), json_scalar_to_string(val));
        }
    }
    if let Some(r) = obj.get("rerun").or_else(|| obj.get("rerunafter")) {
        match r.as_f64() {
            // In range: honoured. Out of range: ignored, never an error, so a
            // mis-set interval cannot break an otherwise valid filter (§8.5).
            Some(x) if (RERUN_MIN..=RERUN_MAX).contains(&x) => out.rerun = Some(x),
            Some(_) => {}
            None => {
                return Err(format!(
                    "'rerun' must be a number, got {}",
                    snippet(&r.to_string())
                ))
            }
        }
    }
    if has_items {
        let arr = obj
            .get("items")
            .and_then(|x| x.as_array())
            .ok_or_else(|| "'items' must be an array".to_string())?;
        for (i, it) in arr.iter().enumerate() {
            let o = it
                .as_object()
                .ok_or_else(|| format!("items[{i}] is not an object"))?;
            out.items.push(json_item(o));
        }
    }
    Ok(out)
}

fn json_item(o: &serde_json::Map<String, serde_json::Value>) -> SfItem {
    let mut it = SfItem {
        uid: json_str(o.get("uid")),
        item_type: json_str(o.get("type")),
        title: json_str(o.get("title")),
        subtitle: json_str(o.get("subtitle")),
        arg: json_str(o.get("arg")),
        autocomplete: json_str(o.get("autocomplete")),
        match_text: json_str(o.get("match")),
        quicklookurl: json_str(o.get("quicklookurl")),
        valid: match o.get("valid") {
            Some(serde_json::Value::Bool(b)) => *b,
            _ => true,
        },
        ..SfItem::default()
    };
    if let Some(ic) = o.get("icon").and_then(|x| x.as_object()) {
        it.icon = Some(SfIcon {
            kind: json_str(ic.get("type")),
            path: json_str(ic.get("path")),
        });
    }
    if let Some(tx) = o.get("text").and_then(|x| x.as_object()) {
        it.text = Some(SfText {
            copy: json_str(tx.get("copy")),
            largetype: json_str(tx.get("largetype")),
        });
    }
    if let Some(m) = o.get("mods").and_then(|x| x.as_object()) {
        for (k, v) in m {
            let Some(mo) = v.as_object() else { continue };
            let valid = match mo.get("valid") {
                Some(serde_json::Value::Bool(b)) => *b,
                _ => true,
            };
            it.mods.insert(
                k.to_lowercase(),
                SfMod {
                    valid,
                    arg: json_str(mo.get("arg")),
                    subtitle: json_str(mo.get("subtitle")),
                },
            );
        }
    }
    it
}

// --- legacy XML form -------------------------------------------------------

#[derive(Debug, Clone)]
enum XNode {
    Elem {
        name: String,
        attrs: Vec<(String, String)>,
        children: Vec<XNode>,
    },
    Text(String),
}

impl XNode {
    fn name(&self) -> Option<&str> {
        match self {
            XNode::Elem { name, .. } => Some(name.as_str()),
            XNode::Text(_) => None,
        }
    }
    fn children(&self) -> &[XNode] {
        match self {
            XNode::Elem { children, .. } => children,
            XNode::Text(_) => &[],
        }
    }
    fn children_named<'a>(&'a self, want: &'a str) -> impl Iterator<Item = &'a XNode> {
        self.children()
            .iter()
            .filter(move |c| c.name() == Some(want))
    }
    fn attr(&self, want: &str) -> Option<&str> {
        match self {
            XNode::Elem { attrs, .. } => attrs
                .iter()
                .find(|(k, _)| k == want)
                .map(|(_, v)| v.as_str()),
            XNode::Text(_) => None,
        }
    }
    fn text(&self) -> String {
        let mut s = String::new();
        for c in self.children() {
            if let XNode::Text(t) = c {
                s.push_str(t);
            }
        }
        s
    }
}

fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest.find(';') else {
            out.push_str(rest);
            return out;
        };
        let ent = &rest[1..end];
        let decoded = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ if ent.starts_with("#x") || ent.starts_with("#X") => {
                u32::from_str_radix(&ent[2..], 16).ok().and_then(char::from_u32)
            }
            _ if ent.starts_with('#') => {
                ent[1..].parse::<u32>().ok().and_then(char::from_u32)
            }
            _ => None,
        };
        match decoded {
            Some(c) => out.push(c),
            None => out.push_str(&rest[..=end]),
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

fn parse_bool(s: &str) -> bool {
    matches!(s.trim().to_ascii_lowercase().as_str(), "true" | "1" | "yes")
}

struct XmlParser<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> XmlParser<'a> {
    fn new(src: &'a str) -> Self {
        XmlParser { src, pos: 0 }
    }
    fn rest(&self) -> &'a str {
        &self.src[self.pos..]
    }
    fn line_col(&self) -> (usize, usize) {
        let upto = &self.src[..self.pos];
        let line = upto.matches('\n').count() + 1;
        let col = match upto.rfind('\n') {
            Some(i) => self.pos - i,
            None => self.pos + 1,
        };
        (line, col)
    }
    fn err(&self, what: &str) -> String {
        let (l, c) = self.line_col();
        format!(
            "XML Parse Error '{what}'. Row {l}, Col {c} in XML: {}",
            snippet(self.src)
        )
    }
    fn starts_with(&self, s: &str) -> bool {
        self.rest().starts_with(s)
    }
    fn eat(&mut self, s: &str) -> bool {
        if self.starts_with(s) {
            self.pos += s.len();
            true
        } else {
            false
        }
    }
    fn skip_ws(&mut self) {
        while let Some(c) = self.rest().chars().next() {
            if c.is_whitespace() {
                self.pos += c.len_utf8();
            } else {
                break;
            }
        }
    }
    /// Skip whitespace, the XML declaration, comments and doctypes.
    fn skip_misc(&mut self) -> Result<(), String> {
        loop {
            self.skip_ws();
            if self.starts_with("<?") {
                match self.rest().find("?>") {
                    Some(i) => self.pos += i + 2,
                    None => return Err(self.err("unterminated XML declaration")),
                }
            } else if self.starts_with("<!--") {
                match self.rest().find("-->") {
                    Some(i) => self.pos += i + 3,
                    None => return Err(self.err("unterminated comment")),
                }
            } else if self.starts_with("<!") {
                match self.rest().find('>') {
                    Some(i) => self.pos += i + 1,
                    None => return Err(self.err("unterminated declaration")),
                }
            } else {
                return Ok(());
            }
        }
    }
    fn parse_name(&mut self) -> Result<String, String> {
        let start = self.pos;
        while let Some(c) = self.rest().chars().next() {
            if c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ':') {
                self.pos += c.len_utf8();
            } else {
                break;
            }
        }
        if self.pos == start {
            return Err(self.err("expected an element name"));
        }
        Ok(self.src[start..self.pos].to_string())
    }
    fn parse_attrs(&mut self) -> Result<Vec<(String, String)>, String> {
        let mut attrs = Vec::new();
        loop {
            self.skip_ws();
            if self.starts_with("/>") || self.rest().starts_with('>') {
                return Ok(attrs);
            }
            if self.rest().is_empty() {
                return Err(self.err("unterminated start tag"));
            }
            let name = self.parse_name()?;
            self.skip_ws();
            if !self.eat("=") {
                return Err(self.err("expected '=' after an attribute name"));
            }
            self.skip_ws();
            let quote = self
                .rest()
                .chars()
                .next()
                .filter(|c| *c == '"' || *c == '\'')
                .ok_or_else(|| self.err("expected a quoted attribute value"))?;
            self.pos += quote.len_utf8();
            let start = self.pos;
            match self.rest().find(quote) {
                Some(i) => {
                    let raw = &self.src[start..start + i];
                    self.pos = start + i + quote.len_utf8();
                    attrs.push((name, decode_entities(raw)));
                }
                None => return Err(self.err("unterminated attribute value")),
            }
        }
    }
    fn parse_element(&mut self) -> Result<XNode, String> {
        if !self.eat("<") {
            return Err(self.err("expected '<'"));
        }
        let name = self.parse_name()?;
        let attrs = self.parse_attrs()?;
        if self.eat("/>") {
            return Ok(XNode::Elem { name, attrs, children: Vec::new() });
        }
        if !self.eat(">") {
            return Err(self.err("expected '>'"));
        }
        let mut children = Vec::new();
        loop {
            if self.rest().is_empty() {
                return Err(self.err(&format!("element <{name}> is not closed")));
            }
            if self.starts_with("</") {
                self.pos += 2;
                let close = self.parse_name()?;
                self.skip_ws();
                if !self.eat(">") {
                    return Err(self.err("expected '>' in end tag"));
                }
                if close != name {
                    return Err(self.err(&format!(
                        "mismatched end tag </{close}> for <{name}>"
                    )));
                }
                return Ok(XNode::Elem { name, attrs, children });
            }
            if self.starts_with("<![CDATA[") {
                let start = self.pos + "<![CDATA[".len();
                match self.src[start..].find("]]>") {
                    Some(i) => {
                        children.push(XNode::Text(self.src[start..start + i].to_string()));
                        self.pos = start + i + 3;
                    }
                    None => return Err(self.err("unterminated CDATA section")),
                }
                continue;
            }
            if self.starts_with("<") {
                children.push(self.parse_element()?);
                continue;
            }
            let start = self.pos;
            match self.rest().find('<') {
                Some(i) => {
                    children.push(XNode::Text(decode_entities(&self.src[start..start + i])));
                    self.pos = start + i;
                }
                None => return Err(self.err(&format!("element <{name}> is not closed"))),
            }
        }
    }
}

fn parse_xml_output(t: &str) -> Result<SfOutput, String> {
    let mut p = XmlParser::new(t);
    p.skip_misc()?;
    let root = p.parse_element()?;
    p.skip_misc()?;
    if !p.rest().is_empty() {
        return Err(p.err("trailing content after the root element"));
    }
    let name = root.name().unwrap_or("").to_string();
    if name != "output" {
        return Err(format!(
            "XML needs an 'output' root element, found <{name}>: {}",
            snippet(t)
        ));
    }
    let mut out = SfOutput::default();
    for ch in root.children() {
        match ch.name() {
            Some("rerun") => {
                if let Ok(v) = ch.text().trim().parse::<f64>() {
                    if (RERUN_MIN..=RERUN_MAX).contains(&v) {
                        out.rerun = Some(v);
                    }
                }
            }
            Some("variables") => {
                for v in ch.children_named("variable") {
                    if let Some(n) = v.attr("name") {
                        out.variables.insert(n.to_string(), v.text());
                    }
                }
            }
            Some("items") => {
                for it in ch.children_named("item") {
                    out.items.push(xml_item(it));
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

fn xml_item(it: &XNode) -> SfItem {
    let mut s = SfItem {
        arg: it.attr("arg").unwrap_or("").to_string(),
        uid: it.attr("uid").unwrap_or("").to_string(),
        ..SfItem::default()
    };
    if let Some(v) = it.attr("valid") {
        s.valid = parse_bool(v);
    }
    for ch in it.children() {
        match ch.name() {
            Some("title") => s.title = ch.text(),
            Some("subtitle") => s.subtitle = ch.text(),
            Some("uid") => s.uid = ch.text(),
            Some("autocomplete") => s.autocomplete = ch.text(),
            Some("match") => s.match_text = ch.text(),
            Some("quicklookurl") => s.quicklookurl = ch.text(),
            Some("valid") => s.valid = parse_bool(&ch.text()),
            Some("icon") => {
                s.icon = Some(SfIcon {
                    kind: ch.attr("type").unwrap_or("").to_string(),
                    path: ch.text(),
                })
            }
            Some("text") => {
                let kind = ch.attr("type").unwrap_or("copy").to_ascii_lowercase();
                let val = ch.text();
                let slot = s.text.get_or_insert_with(SfText::default);
                if kind == "largetype" {
                    slot.largetype = val;
                } else {
                    slot.copy = val;
                }
            }
            Some("mods") => {
                for m in ch.children_named("mod") {
                    let key = m.attr("key").unwrap_or("").to_ascii_lowercase();
                    if key.is_empty() {
                        continue;
                    }
                    s.mods.insert(
                        key,
                        SfMod {
                            valid: m.attr("valid").map(parse_bool).unwrap_or(true),
                            arg: m.attr("arg").unwrap_or("").to_string(),
                            subtitle: m.attr("subtitle").unwrap_or("").to_string(),
                        },
                    );
                }
            }
            _ => {}
        }
    }
    s
}
