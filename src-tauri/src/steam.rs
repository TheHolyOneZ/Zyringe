

use std::path::PathBuf;
use std::process::Command;

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}


pub fn is_running() -> bool {
    Command::new("pgrep")
        .args(["-x", "steam"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}


fn userdata_roots() -> Vec<PathBuf> {
    let h = match home() {
        Some(h) => h,
        None => return vec![],
    };
    [
        ".steam/steam/userdata",
        ".local/share/Steam/userdata",
        ".steam/root/userdata",
        ".var/app/com.valvesoftware.Steam/.local/share/Steam/userdata",
    ]
    .iter()
    .map(|s| h.join(s))
    .filter(|p| p.is_dir())
    .collect()
}


fn find_localconfigs(app_id: &str) -> Vec<PathBuf> {
    let mut all = Vec::new();
    for root in userdata_roots() {
        if let Ok(rd) = std::fs::read_dir(&root) {
            for e in rd.flatten() {
                let p = e.path().join("config/localconfig.vdf");
                if p.is_file() {
                    all.push(p);
                }
            }
        }
    }
    let needle = format!("\"{app_id}\"");
    let preferred: Vec<PathBuf> = all
        .iter()
        .filter(|p| std::fs::read_to_string(p).map(|c| c.contains(&needle)).unwrap_or(false))
        .cloned()
        .collect();
    if !preferred.is_empty() {
        return preferred;
    }
    all.sort_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
    all.reverse();
    all
}


fn find_quoted_key(content: &str, key: &str, from: usize) -> Option<usize> {
    let needle = format!("\"{key}\"");
    content.get(from..)?.find(&needle).map(|i| i + from)
}


fn matching_brace(content: &str, open: usize) -> Option<usize> {
    let b = content.as_bytes();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut esc = false;
    for i in open..b.len() {
        let c = b[i];
        if in_str {
            if esc {
                esc = false;
            } else if c == b'\\' {
                esc = true;
            } else if c == b'"' {
                in_str = false;
            }
        } else {
            match c {
                b'"' => in_str = true,
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            }
        }
    }
    None
}


fn find_string_end(content: &str, start: usize) -> Option<usize> {
    let b = content.as_bytes();
    let mut esc = false;
    for i in start..b.len() {
        let c = b[i];
        if esc {
            esc = false;
        } else if c == b'\\' {
            esc = true;
        } else if c == b'"' {
            return Some(i);
        }
    }
    None
}

fn escape_vdf(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}


fn set_launch_in_vdf(content: &str, app_id: &str, option: &str) -> Result<String, String> {
    let apps_key = find_quoted_key(content, "apps", 0)
        .or_else(|| find_quoted_key(content, "Apps", 0))
        .ok_or("no \"apps\" section in localconfig")?;
    let apps_open = content[apps_key..].find('{').map(|i| i + apps_key).ok_or("malformed apps")?;
    let apps_close = matching_brace(content, apps_open).ok_or("unbalanced apps section")?;
    let esc = escape_vdf(option);


    if let Some(app_key) = find_quoted_key(&content[apps_open..apps_close], app_id, 0).map(|i| i + apps_open) {
        let app_open = content[app_key..apps_close]
            .find('{')
            .map(|i| i + app_key)
            .ok_or("malformed app block")?;
        let app_close = matching_brace(content, app_open).ok_or("unbalanced app block")?;

        if let Some(lo_key) =
            find_quoted_key(&content[app_open..app_close], "LaunchOptions", 0).map(|i| i + app_open)
        {

            let after = lo_key + "\"LaunchOptions\"".len();
            let val_open = content[after..app_close]
                .find('"')
                .map(|i| i + after)
                .ok_or("malformed LaunchOptions")?;
            let val_close = find_string_end(content, val_open + 1).ok_or("malformed value")?;
            let mut out = String::with_capacity(content.len() + esc.len());
            out.push_str(&content[..val_open + 1]);
            out.push_str(&esc);
            out.push_str(&content[val_close..]);
            Ok(out)
        } else {

            let ins = format!("\n\t\t\t\t\t\t\"LaunchOptions\"\t\t\"{esc}\"");
            let mut out = String::with_capacity(content.len() + ins.len());
            out.push_str(&content[..app_open + 1]);
            out.push_str(&ins);
            out.push_str(&content[app_open + 1..]);
            Ok(out)
        }
    } else {

        let ins = format!(
            "\n\t\t\t\t\t\"{app_id}\"\n\t\t\t\t\t{{\n\t\t\t\t\t\t\"LaunchOptions\"\t\t\"{esc}\"\n\t\t\t\t\t}}"
        );
        let mut out = String::with_capacity(content.len() + ins.len());
        out.push_str(&content[..apps_open + 1]);
        out.push_str(&ins);
        out.push_str(&content[apps_open + 1..]);
        Ok(out)
    }
}

pub fn set_launch_option(app_id: &str, option: &str) -> Result<String, String> {
    if is_running() {
        return Err("Steam is running — fully close it first (Steam → Exit), then try again. Steam overwrites its config on exit.".into());
    }
    let configs = find_localconfigs(app_id);
    if configs.is_empty() {
        return Err("Couldn't find Steam's config (localconfig.vdf). Is Steam installed for this user?".into());
    }
    let mut last_err = None;
    for cfg in &configs {
        let content = match std::fs::read_to_string(cfg) {
            Ok(c) => c,
            Err(e) => {
                last_err = Some(format!("read {}: {e}", cfg.display()));
                continue;
            }
        };
        match set_launch_in_vdf(&content, app_id, option) {
            Ok(new) => {

                let _ = std::fs::write(cfg.with_extension("vdf.zyringe-bak"), &content);
                std::fs::write(cfg, new)
                    .map_err(|e| format!("write {}: {e}", cfg.display()))?;
                return Ok("launch option set in Steam".into());
            }
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| "couldn't set the launch option".into()))
}


pub fn run_game(app_id: &str) -> Result<String, String> {
    let url = format!("steam://run/{app_id}");
    if Command::new("steam").arg(&url).spawn().is_ok() {
        return Ok(format!("launching appid {app_id} via Steam"));
    }
    Command::new("xdg-open")
        .arg(&url)
        .spawn()
        .map(|_| format!("launching appid {app_id} via Steam"))
        .map_err(|e| format!("couldn't launch Steam: {e}"))
}
