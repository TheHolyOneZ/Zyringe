

use std::path::{Path, PathBuf};

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn walk_for_player_log(dir: &Path, depth: usize, best: &mut Option<(std::time::SystemTime, PathBuf)>) {
    if depth > 4 {
        return;
    }
    let rd = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return,
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk_for_player_log(&p, depth + 1, best);
        } else if p.file_name().and_then(|n| n.to_str()) == Some("Player.log") {
            if let Ok(m) = p.metadata().and_then(|md| md.modified()) {
                if best.as_ref().map_or(true, |(bt, _)| m > *bt) {
                    *best = Some((m, p.clone()));
                }
            }
        }
    }
}


fn newest_player_log() -> Option<PathBuf> {
    let base = home()?.join(".config/unity3d");
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    walk_for_player_log(&base, 0, &mut best);
    best.map(|(_, p)| p)
}

fn open_path(p: &Path) -> Result<(), String> {
    std::process::Command::new("xdg-open")
        .arg(p)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("xdg-open failed: {e}"))
}

pub fn open_player_log() -> Result<String, String> {
    let p = newest_player_log()
        .ok_or("No Unity Player.log found under ~/.config/unity3d")?;
    open_path(&p)?;
    Ok(p.to_string_lossy().into_owned())
}

pub fn open_helper_log(pid: i32) -> Result<String, String> {
    let p = PathBuf::from(format!("/tmp/.zyringe/helper-{pid}.log"));
    if !p.exists() {
        return Err(format!("No helper log yet at {}", p.display()));
    }
    open_path(&p)?;
    Ok(p.to_string_lossy().into_owned())
}
