//! Action execution shared by CLI/TUI/GUI — the "Enter pressed" path.

use crate::db::Store;
use crate::router::{self, Action};
use crate::search::{Candidate, Kind};
use std::path::Path;

pub fn act(r: &Candidate, store: &Store) {
    if let Some(a) = &r.action {
        // workflow action tag: "<type>:<target>"
        let (typ, target) = a.split_once(':').unwrap_or((a.as_str(), ""));
        if typ.contains("openurl") {
            let _ = router::exec_action(Action::OpenUrl(target.to_string()));
            return;
        }
        if typ.contains("openfile") {
            if target.starts_with("http") {
                let _ = router::exec_action(Action::OpenUrl(target.to_string()));
            } else {
                let _ = router::exec_action(Action::Open(target.into()));
            }
            return;
        }
        if typ.contains("revealfile") {
            let _ = router::exec_action(Action::Reveal(target.into()));
            return;
        }
        if typ.contains("output.clipboard") {
            let _ = router::exec_action(Action::CopyText(target.to_string()));
            return;
        }
        if typ.contains("terminalcommand") || typ.contains("systemcommand") {
            if typ.contains("systemcommand") {
                let _ = std::process::Command::new("/bin/sh")
                    .arg("-c")
                    .arg(target)
                    .current_dir(std::env::temp_dir())
                    .spawn();
            } else {
                let _ = router::exec_action(Action::OpenTerminal(target.into()));
            }
            return;
        }
        if typ.starts_with("music") {
            let _ = crate::itunes::run(a);
            return;
        }
        if typ.contains("action.script") {
            let _ = std::process::Command::new("/bin/sh")
                .arg("-c")
                .arg(target)
                .spawn();
            return;
        }
    }
    match r.kind {
        Kind::App | Kind::Dir | Kind::File | Kind::PrefPane => {
            if let Some(p) = &r.path {
                let _ = router::exec_action(Action::Open(p.into()));
                store.record_use(p, "");
            }
        }
        Kind::WebSearch => {
            if let Some(u) = &r.path {
                if u.starts_with("http") {
                    let _ = router::exec_action(Action::OpenUrl(u.clone()));
                    store.record_use(u, "");
                } else {
                    let _ = router::exec_action(Action::CopyText(u.clone()));
                }
            }
        }
        Kind::Snippet => {
            if let Some(t) = &r.path {
                let _ = router::exec_action(Action::SnippetText(t.clone()));
            }
        }
        Kind::Dictionary => {
            if let Some(u) = &r.path {
                let _ = router::exec_action(Action::OpenUrl(u.clone()));
            }
        }
        Kind::Clipboard => {
            if let Some(t) = &r.path {
                let _ = router::exec_action(Action::Paste(t.clone()));
            }
        }
        Kind::Calc => {
            let _ = router::exec_action(Action::CopyText(r.title.clone()));
        }
        _ => {
            if let Some(p) = &r.path {
                if p.starts_with("http") {
                    let _ = router::exec_action(Action::OpenUrl(p.clone()));
                } else if Path::new(p).exists() {
                    let _ = router::exec_action(Action::Open(p.into()));
                } else {
                    let _ = router::exec_action(Action::CopyText(p.clone()));
                }
            }
        }
    }
}

use serde::Serialize;

#[derive(Serialize)]
struct ItemMsg {
    t: String,
    s: String,
    k: String,
}

pub fn items_json(cands: &[Candidate]) -> String {
    let v: Vec<ItemMsg> = cands
        .iter()
        .map(|c| ItemMsg {
            t: c.title.clone(),
            s: c.subtitle.clone(),
            k: c.kind.label().to_string(),
        })
        .collect();
    serde_json::to_string(&v).unwrap_or_else(|_| "[]".into())
}

pub const HTML: &str = r#"<!doctype html><meta charset=utf-8><style>
html,body{margin:0;height:100%;font-family:-apple-system,system-ui;background:transparent;overflow:hidden}
#q{width:100%;box-sizing:border-box;border:0;background:#{{BG}};color:#{{TEXT}};outline:none;
padding:16px 20px;font-size:24px;border-radius:16px}
#r{margin-top:6px;background:#{{BG}};border-radius:12px;overflow:hidden;max-height:280px;overflow-y:auto}
.i{display:flex;justify-content:space-between;align-items:center;padding:8px 18px;cursor:pointer}
.i:hover,.sel{background:#{{SELBG}}}
.t{font-size:14px;color:#{{TEXT}};overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.s{font-size:11px;color:#{{SUBTEXT}};margin-left:10px;max-width:340px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
</style><input id=q placeholder="Search ralfred…" autofocus><div id=r style="display:none"></div>
<script>
let sel=0, items=[];
const q=document.getElementById('q'), r=document.getElementById('r');
function esc(x){return x.replace(/&/g,'&amp;').replace(/</g,'&lt;')}
function draw(v){items=v;sel=0;
  r.style.display=v.length?'block':'none';
  r.innerHTML=v.map((i,n)=>'<div class="i'+(n===sel?' sel':'')+'" onclick="go('+n+')"><div class=t>'+esc(i.t)+
    '</div><div class=s>'+esc(i.s)+' '+esc(i.k)+'</div></div>').join('');
}
function mark(){[...r.children].forEach((c,n)=>c.className='i'+(n===sel?' sel':''))}
function go(n){sel=n;fetch('ralfred://api/exec?id='+n)}
function rfSearch(s){if(!s){draw([]);return}
  fetch('ralfred://api/search?q='+encodeURIComponent(s)).then(x=>x.json()).then(draw)}
q.addEventListener('keydown',e=>{
  if(e.key==='Enter'){e.preventDefault();go(sel)}
  else if(e.key==='ArrowDown'){e.preventDefault();sel=Math.min(sel+1,items.length-1);mark()}
  else if(e.key==='ArrowUp'){e.preventDefault();sel=Math.max(sel-1,0);mark()}
  else if(e.key==='Escape'){fetch('ralfred://api/hide')}
  else if(e.metaKey&&e.key.toLowerCase()==='y'){e.preventDefault();if(items.length)fetch('ralfred://api/ql?id='+sel)}
});
q.addEventListener('input',()=>rfSearch(q.value));
window.rfSearch=rfSearch;
const RF_BG='#{{BG}}';
window.rfProbe=n=>{fetch('ralfred://api/probe?rows='+n)};
window.rfTheme=()=>{fetch('ralfred://api/probe?bg='+RF_BG.replace('#',''))};
window.rfExec=n=>{fetch('ralfred://api/exec?id='+n)};
window.rfReset=()=>{q.value='';q.focus();draw([])};
</script>"#;

/// HTML with the given theme's colors injected.
pub fn themed(theme: &crate::theme::Theme) -> String {
    theme.apply_html(HTML)
}
