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
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct wfobj {
    pub uid: String,
    pub typ: String,
    pub config: HashMap<String, plist::Value>,
}

#[derive(Debug, Clone)]
pub struct Workflow {
    pub uid: String, // folder name / bundleid
    pub name: String,
    pub keyword: Option<String>, // keyword on the root input (keyword object)
    pub objects: Vec<wfobj>,
    pub connections: HashMap<String, Vec<(String, i64)>>, // from -> [(to, modifiers)]
    pub dir: PathBuf,
    /// info.plist `variables` — exposed to scripts as ALFRED_WORKFLOW_VARIABLE_<K>
    /// and substituted as {var:K} (evidence: alfred_workflow_variable_* strings).
    pub variables: HashMap<String, String>,
}

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
                    objects.push(wfobj { uid, typ, config });
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
        let name = d
            .get("name")
            .and_then(|x| x.as_string())
            .unwrap_or("")
            .to_string();
        // keyword: first scriptfilter/filter with keyword
        let keyword = objects.iter().find_map(|o| {
            o.config
                .get("keyword")
                .and_then(|x| x.as_string())
                .map(|s| s.to_string())
        });
        let mut variables = HashMap::new();
        if let Some(v) = d.get("variables").and_then(|x| x.as_dictionary()) {
            for (k, val) in v.iter() {
                if let Some(s) = val.as_string() {
                    variables.insert(k.clone(), s.to_string());
                }
            }
        }
        Ok(Workflow {
            uid: dir.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
            name,
            keyword,
            objects,
            connections,
            variables,
            dir: dir.to_path_buf(),
        })
    }

    /// Run a keyword-matched workflow with `query`: locate the scriptfilter
    /// carrying the keyword, execute it, then walk the connection graph to
    /// action nodes (openfile/openurl/terminal) and attach the resulting action.
    pub fn run_keyword(&self, query: &str) -> Vec<Candidate> {
        let mut results = Vec::new();
        let kw = self.keyword.clone().unwrap_or_default();
        let mut entry: Option<&wfobj> = None;
        for o in &self.objects {
            if let Some(k) = o.config.get("keyword").and_then(|x| x.as_string()) {
                if k == kw {
                    entry = Some(o);
                    break;
                }
            }
        }
        let Some(entry) = entry else { return results };
        match entry.typ.as_str() {
            t if t.contains("scriptfilter") => {
                let script = entry
                    .config
                    .get("script")
                    .and_then(|x| x.as_string())
                    .unwrap_or("")
                    .to_string();
                let stype = entry.config.get("type").and_then(|x| x.as_signed_integer()).unwrap_or(2) as i32;
                results = self.run_script_filter(&script, stype, query);
            }
            t if t.contains("input.keyword") => {
                // keyword entry: arg passes straight through the graph as a single item
                results.push(Candidate {
                    title: query.to_string(),
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
                    title: query.to_string(),
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
        results
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
    fn action_of(&self, node: &wfobj, query: &str) -> Option<(String, String, bool)> {
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

    fn run_script_filter(&self, script: &str, stype: i32, query: &str) -> Vec<Candidate> {
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
        cmd.env("alfred_workflow_version", "1.0");
        cmd.env("alfred_workflow_bundleid", &self.uid);
        cmd.env("alfred_workflow_name", &self.name);
        for (k, v) in &self.variables {
            cmd.env(format!("ALFRED_WORKFLOW_VARIABLE_{k}", k = k.to_uppercase()), v);
        }
        match cmd.output() {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                parse_script_filter_output(&stdout)
            }
            Err(_) => Vec::new(),
        }
    }
}

/// Parse Alfred script-filter JSON: {"items":[{"title","subtitle","arg","uid","autocomplete","valid","text":{"copy","queue"},"icon":{"path"},"mods":{"cmd|alt|ctrl|shift":{...}}}]}
pub fn parse_script_filter_output(stdout: &str) -> Vec<Candidate> {
    let mut out = Vec::new();
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return out;
    }
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if let Some(items) = v.get("items").and_then(|x| x.as_array()) {
            for it in items {
                let title = it.get("title").and_then(|x| x.as_str()).unwrap_or("").to_string();
                let subtitle = it
                    .get("subtitle")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string();
                let arg = it.get("arg").and_then(|x| x.as_str()).unwrap_or(&title).to_string();
                out.push(Candidate {
                    title,
                    subtitle: if subtitle.is_empty() { arg.clone() } else { subtitle },
                    path: Some(arg),
                    score: 100.0,
                    kind: Kind::Action,
                    action: None,
                });
            }
        }
        return out;
    }
    // legacy newline-separated
    for line in trimmed.lines().filter(|l| !l.trim().is_empty()) {
        out.push(Candidate {
            title: line.trim().to_string(),
            subtitle: String::new(),
            path: Some(line.trim().to_string()),
            score: 50.0,
            kind: Kind::Action,
            action: None,
        });
    }
    out
}
