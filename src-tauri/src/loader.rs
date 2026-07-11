

use serde::Serialize;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize)]
pub struct LoaderStatus {
    pub installed: bool,
    pub plugins_dir: String,
    pub plugins: Vec<String>,

    pub run_script: Option<String>,


    pub windows_proxy: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
    pub tag: String,
    pub prerelease: bool,
}

fn repo(loader: &str) -> &'static str {
    match loader {
        "BepInEx" => "BepInEx/BepInEx",
        _ => "LavaGang/MelonLoader",
    }
}


fn layout(game_dir: &str, loader: &str) -> (PathBuf, PathBuf) {
    let g = Path::new(game_dir);
    match loader {
        "BepInEx" => (g.join("BepInEx"), g.join("BepInEx").join("plugins")),
        _ => (g.join("MelonLoader"), g.join("Mods")),
    }
}

pub fn status(game_dir: &str, loader: &str) -> LoaderStatus {
    let (install_dir, plugins_dir) = layout(game_dir, loader);
    let installed = install_dir.is_dir();

    let mut plugins = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&plugins_dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()).map(|x| x.eq_ignore_ascii_case("dll"))
                == Some(true)
            {
                if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                    plugins.push(name.to_string());
                }
            }
        }
    }
    plugins.sort();

    let run_script = {
        let s = Path::new(game_dir).join("run_bepinex.sh");
        if s.is_file() {
            Some(s.to_string_lossy().into_owned())
        } else {
            None
        }
    };

    let g = Path::new(game_dir);
    let windows_proxy = g.join("winhttp.dll").is_file() || g.join("version.dll").is_file();

    LoaderStatus {
        installed,
        plugins_dir: plugins_dir.to_string_lossy().into_owned(),
        plugins,
        run_script,
        windows_proxy,
    }
}


pub fn add_plugin(plugins_dir: &str, dll: &str) -> Result<String, String> {
    let src = Path::new(dll);
    if src.extension().and_then(|x| x.to_str()).map(|x| x.eq_ignore_ascii_case("dll"))
        != Some(true)
    {
        return Err("not a .dll file".into());
    }
    let name = src.file_name().ok_or("could not read file name")?;
    let dst_dir = Path::new(plugins_dir);
    std::fs::create_dir_all(dst_dir).map_err(|e| format!("create {plugins_dir}: {e}"))?;
    let dst = dst_dir.join(name);
    std::fs::copy(src, &dst).map_err(|e| format!("copy failed: {e}"))?;
    Ok(name.to_string_lossy().into_owned())
}

pub fn remove_plugin(plugins_dir: &str, name: &str) -> Result<(), String> {
    let p = Path::new(plugins_dir).join(name);
    std::fs::remove_file(&p).map_err(|e| format!("remove {}: {e}", p.display()))
}


pub fn reveal(path: &str) -> Result<(), String> {
    Command::new("xdg-open")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("xdg-open failed: {e}"))
}


pub fn fetch_assets(loader: &str) -> Result<Vec<Asset>, String> {
    let url = format!("https://api.github.com/repos/{}/releases?per_page=10", repo(loader));
    let out = Command::new("curl")
        .args([
            "-sSL",
            "-H",
            "User-Agent: Zyringe",
            "-H",
            "Accept: application/vnd.github+json",
            &url,
        ])
        .output()
        .map_err(|e| format!("curl failed (is curl installed?): {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "GitHub request failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let json: serde_json::Value =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("bad GitHub response: {e}"))?;

    if let Some(msg) = json.get("message").and_then(|m| m.as_str()) {
        return Err(format!("GitHub: {msg}"));
    }
    let releases = json.as_array().ok_or("unexpected GitHub response")?;

    let mut assets = Vec::new();
    for rel in releases {
        let tag = rel["tag_name"].as_str().unwrap_or("").to_string();
        let prerelease = rel["prerelease"].as_bool().unwrap_or(false);
        if let Some(arr) = rel["assets"].as_array() {
            for a in arr {
                let name = a["name"].as_str().unwrap_or("").to_string();
                let durl = a["browser_download_url"].as_str().unwrap_or("").to_string();
                let size = a["size"].as_u64().unwrap_or(0);


                if name.is_empty() || durl.is_empty() || !name.to_lowercase().ends_with(".zip") {
                    continue;
                }
                assets.push(Asset {
                    name,
                    url: durl,
                    size,
                    tag: tag.clone(),
                    prerelease,
                });
            }
        }
    }
    Ok(assets)
}


fn looks_like_zip(path: &Path) -> Result<(), String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).map_err(|e| format!("open: {e}"))?;
    let mut magic = [0u8; 2];
    f.read_exact(&mut magic)
        .map_err(|_| "file is empty or unreadable".to_string())?;
    if &magic != b"PK" {
        return Err("that isn't a .zip (got a non-archive — likely a wrong asset or a failed download)".into());
    }
    Ok(())
}


fn extract_into<R: std::io::Read + std::io::Seek>(
    game_dir: &str,
    reader: R,
) -> Result<String, String> {
    let base = Path::new(game_dir);
    let mut archive = zip::ZipArchive::new(reader).map_err(|e| format!("open archive: {e}"))?;
    let mut count = 0;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| format!("archive entry: {e}"))?;
        let rel = match entry.enclosed_name() {
            Some(p) => p.to_owned(),
            None => continue,
        };
        let outpath = base.join(&rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&outpath).map_err(|e| format!("mkdir: {e}"))?;
        } else {
            if let Some(parent) = outpath.parent() {
                std::fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;
            }
            let mut out = std::fs::File::create(&outpath)
                .map_err(|e| format!("create {}: {e}", outpath.display()))?;
            std::io::copy(&mut entry, &mut out).map_err(|e| format!("write: {e}"))?;
            if let Some(mode) = entry.unix_mode() {
                let _ = std::fs::set_permissions(&outpath, std::fs::Permissions::from_mode(mode));
            }
            count += 1;
        }
    }
    Ok(format!("extracted {count} files into {game_dir}"))
}


fn verify_install(game_dir: &str, loader: &str, extracted: usize) -> Result<String, String> {
    let g = Path::new(game_dir);
    match loader {
        "BepInEx" => {
            let core = g.join("BepInEx").join("core");
            let proxy = g.join("winhttp.dll").is_file() || g.join("run_bepinex.sh").is_file();
            if !core.is_dir() || !proxy {
                return Err(format!(
                    "That archive didn't contain a complete BepInEx (extracted only {extracted} \
                     files; BepInEx/core + winhttp.dll are missing). The download was likely \
                     incomplete or the wrong asset — re-try, or pick a full \
                     'BepInEx-Unity.IL2CPP-win-x64' build."
                ));
            }


            let is_il2cpp = core.join("BepInEx.Unity.IL2CPP.dll").is_file()
                || core.join("Il2CppInterop.Runtime.dll").is_file();
            if !is_il2cpp {
                return Err(
                    "That's a Mono BepInEx build (BepInEx 5) — it will NOT load in an IL2CPP game \
                     (it loads but finds no Mono runtime and silently does nothing). Install a \
                     BepInEx 6 IL2CPP build instead: the asset name contains 'Unity.IL2CPP' \
                     (e.g. BepInEx-Unity.IL2CPP-win-x64-…). Remove this one first."
                        .into(),
                );
            }
            Ok(format!("BepInEx (IL2CPP) installed — extracted {extracted} files"))
        }
        _ => {
            let ok = g.join("MelonLoader").is_dir()
                && (g.join("version.dll").is_file() || g.join("MelonLoader.Bootstrap.so").is_file());
            if ok {
                Ok(format!("MelonLoader installed — extracted {extracted} files"))
            } else {
                Err(format!(
                    "That archive didn't contain a complete MelonLoader (extracted only \
                     {extracted} files; MelonLoader/ + version.dll are missing). The download was \
                     likely incomplete or the wrong asset — re-try, or pick a '…-win-x64' build."
                ))
            }
        }
    }
}


fn extracted_count(msg: &str) -> usize {
    msg.split_whitespace()
        .nth(1)
        .and_then(|n| n.parse().ok())
        .unwrap_or(0)
}

pub fn install_from_zip(game_dir: &str, zip_path: &str, loader: &str) -> Result<String, String> {
    looks_like_zip(Path::new(zip_path))?;
    let file = std::fs::File::open(zip_path).map_err(|e| format!("open zip: {e}"))?;
    let msg = extract_into(game_dir, file)?;
    verify_install(game_dir, loader, extracted_count(&msg))
}

pub fn install_from_url(game_dir: &str, url: &str, loader: &str) -> Result<String, String> {
    let tmp = std::env::temp_dir().join(format!("zyringe-loader-{}.zip", std::process::id()));

    let out = Command::new("curl")
        .args([
            "-fSL",
            "--retry",
            "2",
            "--retry-delay",
            "1",
            "-H",
            "User-Agent: Zyringe",
            "-o",
        ])
        .arg(&tmp)
        .arg(url)
        .output()
        .map_err(|e| format!("curl failed (is curl installed?): {e}"))?;
    if !out.status.success() {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!(
            "download failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    if let Err(e) = looks_like_zip(&tmp) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    let file = std::fs::File::open(&tmp).map_err(|e| format!("open downloaded zip: {e}"))?;
    let res = extract_into(game_dir, file);
    let _ = std::fs::remove_file(&tmp);
    let msg = res?;
    verify_install(game_dir, loader, extracted_count(&msg))
}


fn proc_alive(pid: i32) -> bool {
    pid > 0 && Path::new(&format!("/proc/{pid}")).exists()
}


pub fn is_active(pid: i32) -> bool {
    let maps = match std::fs::read_to_string(format!("/proc/{pid}/maps")) {
        Ok(m) => m.to_lowercase(),
        Err(_) => return false,
    };
    ["doorstop", "bepinex", "melonloader", "il2cppinterop"]
        .iter()
        .any(|k| maps.contains(k))
}


fn read_environ(pid: i32) -> Option<Vec<(String, String)>> {
    let raw = std::fs::read(format!("/proc/{pid}/environ")).ok()?;
    let mut env = Vec::new();
    for part in raw.split(|&b| b == 0) {
        if part.is_empty() {
            continue;
        }
        if let Ok(s) = std::str::from_utf8(part) {
            if let Some(eq) = s.find('=') {
                env.push((s[..eq].to_string(), s[eq + 1..].to_string()));
            }
        }
    }
    if env.is_empty() {
        None
    } else {
        Some(env)
    }
}


fn read_args(pid: i32) -> Vec<String> {
    match std::fs::read(format!("/proc/{pid}/cmdline")) {
        Ok(raw) => raw
            .split(|&b| b == 0)
            .filter(|s| !s.is_empty())
            .map(|s| String::from_utf8_lossy(s).into_owned())
            .collect(),
        Err(_) => Vec::new(),
    }
}

pub fn launch(
    game_dir: &str,
    exe_path: &str,
    loader: &str,
    app_id: Option<&str>,
    close_pid: Option<i32>,
) -> Result<String, String> {
    let g = Path::new(game_dir);


    let mut env: Option<Vec<(String, String)>> = None;
    let mut cwd: Option<PathBuf> = None;
    let mut game_args: Vec<String> = Vec::new();
    if let Some(pid) = close_pid {
        if proc_alive(pid) {
            env = read_environ(pid);
            cwd = std::fs::read_link(format!("/proc/{pid}/cwd")).ok();
            game_args = read_args(pid).into_iter().skip(1).collect();
        }
    }


    if let Some(pid) = close_pid {
        if proc_alive(pid) {
            let _ = Command::new("kill").arg("-TERM").arg(pid.to_string()).status();
            let mut waited = 0;
            while proc_alive(pid) && waited < 150 {
                std::thread::sleep(std::time::Duration::from_millis(100));
                waited += 1;
            }
            if proc_alive(pid) {
                return Err("couldn't close the running game (it may be showing a save/quit prompt). Close it manually, then Launch with mods again.".into());
            }
        }
    }

    if let Some(id) = app_id {
        if !id.is_empty() {
            let _ = std::fs::write(g.join("steam_appid.txt"), id);
        }
    }


    let mut cmd;
    let label;
    match loader {
        "BepInEx" => {
            let script = g.join("run_bepinex.sh");
            if !script.is_file() {
                return Err("run_bepinex.sh not found — install a Linux BepInEx (IL2CPP) build first".into());
            }
            let _ = std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755));
            cmd = Command::new("bash");
            cmd.arg(&script).arg(exe_path);
            label = "BepInEx";
        }
        _ => {
            let script = g.join("run_melonloader.sh");
            if script.is_file() {
                let _ = std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755));
                cmd = Command::new("bash");
                cmd.arg(&script).arg(exe_path);
            } else {
                cmd = Command::new(exe_path);
            }
            label = "MelonLoader";
        }
    }
    for a in &game_args {
        cmd.arg(a);
    }


    if let Some(env) = env {
        cmd.env_clear();
        for (k, v) in env {
            cmd.env(k, v);
        }
    }
    cmd.current_dir(cwd.unwrap_or_else(|| PathBuf::from(game_dir)));

    cmd.spawn()
        .map(|_| format!("launched with {label}"))
        .map_err(|e| format!("launch failed: {e}"))
}


pub fn remove(game_dir: &str, loader: &str) -> Result<String, String> {
    let g = Path::new(game_dir);
    let targets: &[&str] = match loader {
        "BepInEx" => &[
            "BepInEx",
            "run_bepinex.sh",
            "doorstop_config.ini",
            ".doorstop_version",
            "libdoorstop_x64.so",
            "libdoorstop_x86.so",
            "libdoorstop.so",
            "doorstop_libs",
            "winhttp.dll",
        ],
        _ => &["MelonLoader", "version.dll", "dobby.dll", "NOTICE.txt"],
    };
    let mut removed = 0;
    for t in targets {
        let p = g.join(t);
        if p.is_dir() {
            if std::fs::remove_dir_all(&p).is_ok() {
                removed += 1;
            }
        } else if p.exists() && std::fs::remove_file(&p).is_ok() {
            removed += 1;
        }
    }
    Ok(format!("removed {removed} loader item(s) from {game_dir}"))
}

