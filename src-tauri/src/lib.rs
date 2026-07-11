

mod diag;
mod injector;
mod loader;
mod metadata;
mod mono;
mod scanner;
mod steam;

use injector::{InjectRequest, LaunchRequest};
use scanner::MonoProcess;
use tauri::AppHandle;

#[tauri::command]
async fn list_mono_processes() -> Vec<MonoProcess> {


    tauri::async_runtime::spawn_blocking(scanner::scan)
        .await
        .unwrap_or_default()
}

#[tauri::command]
fn cancel_injection(pid: i32) -> Result<(), String> {
    injector::cancel(pid)
}

#[tauri::command]
fn open_player_log() -> Result<String, String> {
    diag::open_player_log()
}

#[tauri::command]
fn open_helper_log(pid: i32) -> Result<String, String> {
    diag::open_helper_log(pid)
}

#[tauri::command]
fn list_entry_points(dll_path: String) -> Result<Vec<metadata::EntryPoint>, String> {
    metadata::list_entry_points(&dll_path)
}

#[tauri::command]
fn save_text(path: String, content: String) -> Result<(), String> {
    std::fs::write(&path, content).map_err(|e| format!("write {path}: {e}"))
}

#[tauri::command]
fn loader_status(game_dir: String, loader: String) -> loader::LoaderStatus {
    loader::status(&game_dir, &loader)
}

#[tauri::command]
fn loader_add_plugin(plugins_dir: String, dll_path: String) -> Result<String, String> {
    loader::add_plugin(&plugins_dir, &dll_path)
}

#[tauri::command]
fn loader_remove_plugin(plugins_dir: String, name: String) -> Result<(), String> {
    loader::remove_plugin(&plugins_dir, &name)
}

#[tauri::command]
fn reveal_path(path: String) -> Result<(), String> {
    loader::reveal(&path)
}

#[tauri::command]
async fn loader_fetch_assets(loader: String) -> Result<Vec<loader::Asset>, String> {
    tauri::async_runtime::spawn_blocking(move || loader::fetch_assets(&loader))
        .await
        .map_err(|e| format!("task join error: {e}"))?
}

#[tauri::command]
async fn loader_install_zip(
    game_dir: String,
    zip_path: String,
    loader: String,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        loader::install_from_zip(&game_dir, &zip_path, &loader)
    })
    .await
    .map_err(|e| format!("task join error: {e}"))?
}

#[tauri::command]
async fn loader_install_url(
    game_dir: String,
    url: String,
    loader: String,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || loader::install_from_url(&game_dir, &url, &loader))
        .await
        .map_err(|e| format!("task join error: {e}"))?
}

#[tauri::command]
fn loader_remove(game_dir: String, loader: String) -> Result<String, String> {
    loader::remove(&game_dir, &loader)
}

#[tauri::command]
fn loader_active(pid: i32) -> bool {
    loader::is_active(pid)
}

#[tauri::command]
async fn loader_launch(
    game_dir: String,
    exe_path: String,
    loader: String,
    app_id: Option<String>,
    close_pid: Option<i32>,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        loader::launch(&game_dir, &exe_path, &loader, app_id.as_deref(), close_pid)
    })
    .await
    .map_err(|e| format!("task join error: {e}"))?
}

#[tauri::command]
fn steam_running() -> bool {
    steam::is_running()
}

#[tauri::command]
async fn steam_set_launch_option(app_id: String, option: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || steam::set_launch_option(&app_id, &option))
        .await
        .map_err(|e| format!("task join error: {e}"))?
}

#[tauri::command]
fn steam_run(app_id: String) -> Result<String, String> {
    steam::run_game(&app_id)
}

#[tauri::command]
async fn inject(app: AppHandle, request: InjectRequest) -> Result<(), String> {


    tauri::async_runtime::spawn_blocking(move || injector::inject(&app, request))
        .await
        .map_err(|e| format!("task join error: {e}"))?
}

#[tauri::command]
async fn launch_with_preload(app: AppHandle, request: LaunchRequest) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || injector::launch_with_preload(&app, request))
        .await
        .map_err(|e| format!("task join error: {e}"))?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            list_mono_processes,
            inject,
            launch_with_preload,
            cancel_injection,
            open_player_log,
            open_helper_log,
            list_entry_points,
            save_text,
            loader_status,
            loader_add_plugin,
            loader_remove_plugin,
            reveal_path,
            loader_fetch_assets,
            loader_install_zip,
            loader_install_url,
            loader_remove,
            loader_launch,
            loader_active,
            steam_running,
            steam_set_launch_option,
            steam_run
        ])
        .run(tauri::generate_context!())
        .expect("error while running Zyringe");
}
