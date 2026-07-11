

use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

fn first_existing(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates.iter().find(|p| p.exists()).cloned()
}


fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
}


pub fn helper_so(app: &AppHandle) -> Result<PathBuf, String> {
    if let Ok(p) = std::env::var("ZYRINGE_HELPER_SO") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Ok(p);
        }
    }

    let mut candidates = Vec::new();
    if let Ok(res) = app.path().resolve("libzyringe.so", tauri::path::BaseDirectory::Resource) {
        candidates.push(res);
    }
    if let Some(dir) = exe_dir() {

        candidates.push(dir.join("../../helper/libzyringe.so"));
        candidates.push(dir.join("../../../helper/libzyringe.so"));
        candidates.push(dir.join("libzyringe.so"));
    }

    candidates.push(PathBuf::from("/usr/lib/zyringe/libzyringe.so"));
    candidates.push(PathBuf::from("helper/libzyringe.so"));

    first_existing(&candidates).ok_or_else(|| {
        "Could not locate libzyringe.so. Run `make` in helper/ or set ZYRINGE_HELPER_SO.".into()
    })
}


pub fn inject_bin(app: &AppHandle) -> Result<PathBuf, String> {
    if let Ok(p) = std::env::var("ZYRINGE_INJECT_BIN") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Ok(p);
        }
    }

    let mut candidates = Vec::new();
    if let Ok(res) = app.path().resolve("zyringe-inject", tauri::path::BaseDirectory::Resource) {
        candidates.push(res);
    }
    if let Some(dir) = exe_dir() {

        candidates.push(dir.join("zyringe-inject"));
    }


    candidates.push(PathBuf::from("/usr/lib/zyringe/zyringe-inject"));
    candidates.push(PathBuf::from("target/debug/zyringe-inject"));
    candidates.push(PathBuf::from("target/release/zyringe-inject"));

    first_existing(&candidates).ok_or_else(|| {
        "Could not locate zyringe-inject. Build the workspace or set ZYRINGE_INJECT_BIN.".into()
    })
}
