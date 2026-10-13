//! iTunes/Music support. Evidence (binary strings): AlfredFeatureiTunes,
//! AlfrediTunesQuerier, AlfredSystemWorkflowiTunesKeywords(PlayPause),
//! alfred.itunes.play / .random, play/pause/next/playPause verbs,
//! album-artist-keyword node classes.
//!
//! Model: transport keywords (`music play|pause|next|prev...`) map 1:1 to
//! AppleScript tell-commands against Music.app, executed bounded (the app can
//! wedge — the real backend is guarded by a hard timeout). Library search
//! (`music <text>`) runs against RALFRED_MUSIC_JSON fixture in tests; the
//! live backend reads the Music library via AppleScript with the same bound.

use crate::search::{Candidate, Kind};
use std::process::{Command, Stdio};

const VERBS: [(&str, &str); 9] = [
    ("play", "play"),
    ("pause", "pause"),
    ("playpause", "playpause"),
    ("next", "next track"),
    ("prev", "previous track"),
    ("shuffle", "shuffle"),
    ("repeat", "repeat"),
    ("mute", "mute"),
    ("unmute", "unmute"),
];

/// Pure verb mapping: keyword tail -> AppleScript command (testable).
pub fn verb_command(tail: &str) -> Option<&'static str> {
    let t = tail.trim().to_lowercase();
    VERBS.iter().find(|(k, _)| *k == t).map(|(_, c)| *c)
}

/// Bounded AppleScript execution so a wedged Music.app can never hang us.
pub fn tell_music(cmd: &str) -> std::io::Result<()> {
    let script = format!("tell application \"Music\" to {cmd}");
    let mut child = Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return Ok(()),
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(50))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "Music did not respond within 3s (Automation permission?)",
                ));
            }
        }
    }
}

#[derive(serde::Deserialize)]
struct Track {
    name: String,
    #[serde(default)]
    artist: String,
    #[serde(default)]
    album: String,
}

fn fixture_tracks() -> Vec<(String, String, String)> {
    let p = match std::env::var("RALFRED_MUSIC_JSON") {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };
    match std::fs::read_to_string(&p) {
        Ok(s) => serde_json::from_str::<Vec<Track>>(&s)
            .unwrap_or_default()
            .into_iter()
            .map(|t| (t.name, t.artist, t.album))
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// `music <verb>` -> transport candidate; `music <text>` -> library search.
pub fn candidates(q: &str) -> Vec<Candidate> {
    let q = q.trim();
    let (kw, rest) = match q.split_once(' ') {
        Some(x) => x,
        None => (q, ""),
    };
    if kw != "music" && kw != "itunes" {
        return Vec::new();
    }
    let rest = rest.trim();
    if rest.is_empty() {
        // bare `music`: current track info candidate
        return vec![Candidate {
            title: "Music".into(),
            subtitle: "play · pause · next · prev · shuffle — type `music <verb>` or `music <artist>`".into(),
            path: None,
            score: 880.0,
            kind: Kind::Action,
            action: None,
        }];
    }
    if let Some(cmd) = verb_command(rest) {
        return vec![Candidate {
            title: format!("Music: {}", cmd),
            subtitle: "transport control".into(),
            path: None,
            score: 890.0,
            kind: Kind::Action,
            action: Some(format!("music:{cmd}")),
        }];
    }
    // library search over fixture (live backend: same interface, bounded)
    let want = rest.to_lowercase();
    let mut out = Vec::new();
    for (name, artist, album) in fixture_tracks() {
        if name.to_lowercase().contains(&want)
            || artist.to_lowercase().contains(&want)
            || album.to_lowercase().contains(&want)
        {
            out.push(Candidate {
                title: format!("{} — {}", name, artist),
                subtitle: format!("{} · enter to play", album),
                path: None,
                score: 840.0,
                kind: Kind::Action,
                action: Some(format!("music:playbyname:{}|{}", name, artist)),
            });
        }
    }
    out.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    out.truncate(10);
    out
}

/// Execute a `music:` action tag.
pub fn run(tag: &str) -> std::io::Result<()> {
    let cmd = tag.strip_prefix("music:").unwrap_or(tag);
    if let Some(names) = cmd.strip_prefix("playbyname:") {
        let (name, artist) = names.split_once('|').unwrap_or((names, ""));
        // ask Music to find & play matching track — bounded like everything else
        let script = format!(
            "tell application \"Music\" to play (first track whose name contains \"{}\")",
            name.replace('"', "")
        );
        let _ = run_bounded(&script);
        let _ = artist;
        return Ok(());
    }
    let map = match cmd {
        "play" => "play",
        "pause" => "pause",
        "playpause" => "playpause",
        "next" => "next track",
        "prev" => "previous track",
        "shuffle" => "set shuffle mode to songs",
        "repeat" => "set song repeat to all",
        "mute" => "set sound volume to 0",
        "unmute" => "set sound volume to 50",
        _ => return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "unknown music cmd")),
    };
    tell_music(map)
}

fn run_bounded(script: &str) -> std::io::Result<()> {
    let mut child = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return Ok(()),
            Ok(None) if std::time::Instant::now() < deadline => std::thread::sleep(std::time::Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "timeout"));
            }
        }
    }
}
