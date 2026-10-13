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
        if typ.starts_with("largetype") {
            let _ = crate::largetype::run(a);
            return;
        }
        if typ.starts_with("onepassword") {
            let _ = crate::onepassword::run(a);
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
    #[serde(default)]
    i: String, // icon URL (protocol endpoint), empty when unresolvable
}

pub fn items_json(cands: &[Candidate]) -> String {
    let v: Vec<ItemMsg> = cands
        .iter()
        .map(|c| ItemMsg {
            t: c.title.clone(),
            s: c.subtitle.clone(),
            k: c.kind.label().to_string(),
            i: match c.path.as_deref() {
                Some(p) if std::path::Path::new(p).is_dir() || std::path::Path::new(p).is_file() => {
                    format!("ralfred://icon/{}", p_esc(p))
                }
                _ => String::new(),
            },
        })
        .collect();
    serde_json::to_string(&v).unwrap_or_else(|_| "[]".into())
}

pub const HTML: &str = r#"<!doctype html><meta charset=utf-8><style>
html,body{margin:0;height:100%;font-family:-apple-system,system-ui;background:transparent;overflow:hidden;-webkit-user-select:none}
#p{display:flex;flex-direction:column;position:absolute;inset:0;border-radius:12px;background:#{{BG}};box-shadow:0 8px 30px rgba(0,0,0,.55);overflow:hidden}
#fw{padding:5px 5px 0 5px;flex:0 0 auto}
#q{width:100%;box-sizing:border-box;border:0;background:#{{FIELD}};color:#{{TEXT}};outline:none;border-radius:8px;height:61px;padding:0 18px;font-size:30px;font-weight:400;caret-color:#{{TEXT}}}
#r{flex:1 1 auto;margin:0;padding:0 9px 9px 9px;overflow:hidden;display:none}
.i{display:flex;align-items:center;height:46px;border-radius:4px;padding:0 13px;cursor:default;position:relative}
.i.sel{background:#{{SELBG}}}
.ic{width:28px;height:28px;flex:0 0 28px;margin-right:11px;image-rendering:auto}
.tx{flex:1 1 auto;min-width:0}
.t{font-size:14px;color:#{{TEXT}};overflow:hidden;text-overflow:ellipsis;white-space:nowrap;line-height:17px}
.s{font-size:11px;color:#{{SUBTEXT}};overflow:hidden;text-overflow:ellipsis;white-space:nowrap;line-height:13px}
.h{color:#{{SUBTEXT}};font-size:14px;margin-left:10px;flex:0 0 auto}
.i.sel .h{color:#{{TEXT}}}
#lt{display:none;position:fixed;inset:0;background:#{{BG}};color:#{{TEXT}};z-index:99;align-items:center;justify-content:center;text-align:center;padding:6vw;font-weight:600;word-break:break-all;user-select:all}
#set{display:none;position:fixed;inset:0;background:rgba(0,0,0,.35);z-index:98;align-items:flex-start;justify-content:center;padding-top:10vh;-webkit-backdrop-filter:blur(6px);backdrop-filter:blur(6px)}
#setw{width:640px;max-height:76vh;background:#{{FIELD}};border:1px solid #{{SELBG}};border-radius:12px;color:#{{TEXT}};overflow:hidden;display:flex}
#setnav{width:170px;border-right:1px solid #{{SELBG}};flex:0 0 170px;padding:8px 0}
#setnav div{padding:10px 16px;font-size:13px;cursor:pointer;color:#{{SUBTEXT}}}
#setnav div.on{background:#{{SELBG}};color:#{{TEXT}}}
#setbody{flex:1;padding:16px 20px;overflow-y:auto}
.frow{display:flex;justify-content:space-between;align-items:center;margin:12px 0;font-size:13px}
.frow label{flex:1;padding-right:14px}
.frow input[type=text],.frow select{width:240px;background:#{{BG}};border:1px solid #{{SELBG}};color:#{{TEXT}};border-radius:6px;padding:6px 9px;font-size:13px}
.frow textarea{width:240px;height:64px;background:#{{BG}};border:1px solid #{{SELBG}};color:#{{TEXT}};border-radius:6px;padding:6px 9px;font-size:11px;font-family:ui-monospace,monospace}
.frow .hint{display:block;font-size:10px;color:#{{SUBTEXT}};margin-top:2px}
</style>
<div id="p"><div id="fw"><input id=q autofocus spellcheck=false></div><div id=r></div></div>
<div id="lt"></div>
<div id="set"><div id="setw"><div id="setnav"></div><div id="setbody"></div></div></div>
<script>
let sel=0, items=[];
const q=document.getElementById('q'), r=document.getElementById('r');
function esc(x){return x.replace(/&/g,'&amp;').replace(/</g,'&lt;')}
function hint(n){return n===0?'&#8617;':'&#8984;'+(n+1)}
function sizeSync(){const h=document.getElementById('p').scrollHeight;fetch('ralfred://api/resize?h='+Math.min(Math.max(h,71),972))}
function draw(v){if(window.RF_DIAG)fetch('ralfred://api/probe?js=draw'+v.length);items=v;sel=0;
  r.style.display=v.length?'block':'none';
  r.innerHTML=v.map((i,n)=>'<div class="i'+(n===sel?' sel':'')+'" onclick="go('+n+')" onmouseover="pick('+n+')">'+
    '<img class=ic src="'+(i.i||'')+'" onerror="this.style.display=\'none\'">'+
    '<div class=tx><div class=t>'+esc(i.t)+'</div><div class=s>'+esc(i.s)+'</div></div>'+
    '<div class=h>'+hint(n)+'</div></div>').join('');
  sizeSync();
  window.rfProbe(v.length);
}
function mark(){const c=[...r.children];c.forEach((c2,n)=>c2.className='i'+(n===sel?' sel':''));sizeSync()}
function pick(n){if(n!==sel){sel=n;mark()}}
function go(n){sel=n;fetch('ralfred://api/exec?id='+n)}
window.addEventListener('error',e=>{document.title='JSERR:'+e.message;});
let qid=0;
function rfSearch(s){if(!s){qid++;draw([]);sizeSync();return}
  const id=++qid;
  fetch('ralfred://api/search?q='+encodeURIComponent(s)).then(x=>x.text()).then(t=>{if(id!==qid)return;if(window.RF_DIAG)fetch('ralfred://api/probe?js=TXT'+t.length);try{draw(JSON.parse(t))}catch(e){if(window.RF_DIAG)fetch('ralfred://api/probe?js=DERR'+encodeURIComponent(e.message))}}).catch(e=>{if(window.RF_DIAG)fetch('ralfred://api/probe?js=E'+encodeURIComponent(e.message))})}
q.addEventListener('keydown',e=>{
  if(e.key==='Enter'){e.preventDefault();go(sel)}
  else if(e.key==='ArrowDown'){e.preventDefault();sel=Math.min(sel+1,items.length-1);mark()}
  else if(e.key==='ArrowUp'){e.preventDefault();sel=Math.max(sel-1,0);mark()}
  else if(e.key==='Escape'){if(document.getElementById('lt').style.display==='flex'){rfLargeTypeOff()}else if(document.getElementById('set').style.display==='flex'){rfSettings()}else{fetch('ralfred://api/hide')}}
  else if(e.metaKey&&e.key.toLowerCase()==='y'){e.preventDefault();if(items.length)fetch('ralfred://api/ql?id='+sel)}
  else if(e.metaKey&&e.key===','){e.preventDefault();rfSettings()}
  else if(e.metaKey&&e.key>='1'&&e.key<='9'){e.preventDefault();if(items.length)go(parseInt(e.key)-1)}
});
q.addEventListener('input',()=>rfSearch(q.value));
window.rfSearch=rfSearch;window.rfApply=d=>draw(d);document.title='BOOT';
const RF_BG='#{{BG}}';
window.rfProbe=n=>{fetch('ralfred://api/probe?rows='+n)};
window.rfTheme=()=>{fetch('ralfred://api/probe?bg='+RF_BG.replace('#',''))};
window.rfExec=n=>{fetch('ralfred://api/exec?id='+n)};
window.rfReset=()=>{q.value='';q.focus();draw([]);sizeSync()};
window.rfLargeType=(t,n)=>{const e=document.getElementById('lt');e.textContent=t;e.style.display='flex';e.style.fontSize=n+'px';};
window.rfLargeTypeOff=()=>{const e=document.getElementById('lt');e.style.display='none';};
let setDoc=null,setDirty=false;
window.rfSettings=async function(){const o=document.getElementById('set');
  if(o.style.display==='flex'){if(setDirty)rfSetSave();o.style.display='none';return}
  if(!setDoc){const r=await fetch('ralfred://api/settings');setDoc=JSON.parse(await r.text());rfSetRender(0)}
  o.style.display='flex';q.blur();};
function rfSetRender(sec){const nav=document.getElementById('setnav'),bd=document.getElementById('setbody');
  nav.innerHTML=setDoc.schema.sections.map((x,i)=>'<div class="'+(i===sec?'on':'')+'" onclick="rfSetRender('+i+')">'+x.title+'</div>').join('');
  const fs=setDoc.schema.sections[sec].fields;
  bd.innerHTML=fs.map(f=>{
   const v=setDoc.values[f.key];
   let ctl='';
   if(f.type==='bool')ctl='<input type=checkbox '+(v?'checked':'')+' onchange="rfSetEdit(\''+f.key+'\',this.checked)">';
   else if(f.type==='select')ctl='<select onchange="rfSetEdit(\''+f.key+'\',this.value)">'+f.options.map(o=>'<option '+(o===v?'selected':'')+'>'+o+'</option>').join('')+'</select>';
   else if(f.type==='int')ctl='<input type=text value="'+v+'" onchange="rfSetEdit(\''+f.key+'\',parseInt(this.value))">';
   else if(f.type==='list')ctl='<textarea onchange="rfSetEdit(\''+f.key+'\',this.value.split(String.fromCharCode(10)).filter(s=>s.trim()))">'+String(v)+'</textarea>';
   else if(f.type==='map')ctl='<textarea onchange="rfSetEdit(\''+f.key+'\',rfParseMap(this.value))">'+Object.entries(v||{}).map(([k,u])=>k+' = '+u).join(String.fromCharCode(10))+'</textarea>';
   else ctl='<input type=text value="'+v+'" onchange="rfSetEdit(\''+f.key+'\',this.value)">';
   return '<div class=frow><label>'+f.label+(f.hint?'<span class=hint>'+f.hint+'</span>':'')+'</label>'+ctl+'</div>';
  }).join('');}
window.rfParseMap=t=>{const o={};t.split(String.fromCharCode(10)).forEach(l=>{const i=l.indexOf('=');if(i>0)o[l.slice(0,i).trim()]=l.slice(i+1).trim()});return o};
window.rfSetEdit=(k,v)=>{setDoc.values[k]=v;setDirty=true;rfSetSave();};
async function rfSetSave(){const r=await fetch('ralfred://api/settings',{method:'PATCH',body:JSON.stringify(setDoc.values)});
  if(!r.ok){let j={error:'bad'};try{j=JSON.parse(await r.text())}catch(e){}alert('settings: '+(j.error||'save failed'))}
  else{setDirty=false;}}
</script>"#;

fn p_esc(s: &str) -> String {
    s.bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b == b'/' { (b as char).to_string() } else { format!("%{:02X}", b) })
        .collect()
}

/// HTML with the given theme's colors injected.
pub fn themed(theme: &crate::theme::Theme) -> String {
    theme.apply_html(HTML)
}
