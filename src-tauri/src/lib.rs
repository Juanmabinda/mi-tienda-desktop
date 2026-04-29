use std::path::PathBuf;
use std::sync::Mutex;
use serde::Deserialize;
use tauri::{AppHandle, Manager, RunEvent};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;
use tauri_plugin_updater::UpdaterExt;

// SERVER_URL es donde el Rust (no la WebView) postea el grant para canjearlo
// por el agent_token. Se setea en build via env var MI_TIENDA_SERVER_URL.
// Default: prod. Para staging: `MI_TIENDA_SERVER_URL=https://staging.mitiendapos.com.ar`.
const SERVER_URL: &str = match option_env!("MI_TIENDA_SERVER_URL") {
    Some(v) => v,
    None => "https://mitiendapos.com.ar",
};
const TOKEN_FILE: &str = "agent_token.txt";
const KIOSK_FILE: &str = "kiosk_mode";

struct AgentState {
    child: Mutex<Option<CommandChild>>,
}

#[derive(Deserialize)]
struct ExchangeResponse {
    token: String,
}

fn token_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app_data_dir: {e}"))?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("mkdir: {e}"))?;
    Ok(dir.join(TOKEN_FILE))
}

fn read_token(app: &AppHandle) -> Option<String> {
    let path = token_path(app).ok()?;
    let content = std::fs::read_to_string(path).ok()?;
    let trimmed = content.trim();
    if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
}

fn write_token(app: &AppHandle, token: &str) -> Result<(), String> {
    let path = token_path(app)?;
    std::fs::write(path, token).map_err(|e| format!("write: {e}"))
}

fn delete_token(app: &AppHandle) {
    if let Ok(path) = token_path(app) {
        let _ = std::fs::remove_file(path);
    }
}

// ─── Modo kiosko ────────────────────────────────────────────────
// Pref persistida en app_data_dir/kiosk_mode (contenido "1" = on).
// Útil para comercios que dedican una PC al POS y quieren la app en
// pantalla completa sin acceso a otras apps.

fn kiosk_path(app: &AppHandle) -> Option<PathBuf> {
    let dir = app.path().app_data_dir().ok()?;
    let _ = std::fs::create_dir_all(&dir);
    Some(dir.join(KIOSK_FILE))
}

fn read_kiosk_mode(app: &AppHandle) -> bool {
    kiosk_path(app)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|s| s.trim() == "1")
        .unwrap_or(false)
}

fn write_kiosk_mode(app: &AppHandle, enabled: bool) -> Result<(), String> {
    let path = kiosk_path(app).ok_or("no app_data_dir")?;
    std::fs::write(path, if enabled { "1" } else { "0" })
        .map_err(|e| format!("write kiosk: {e}"))
}

fn apply_kiosk_mode_if_set(app: &AppHandle) {
    if !read_kiosk_mode(app) { return; }
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_decorations(false);
        let _ = window.set_fullscreen(true);
    }
}

#[tauri::command]
fn kiosk_mode_enabled(app: AppHandle) -> bool {
    read_kiosk_mode(&app)
}

#[tauri::command]
fn set_kiosk_mode(app: AppHandle, enabled: bool) -> Result<(), String> {
    write_kiosk_mode(&app, enabled)
    // No aplicamos en runtime; pedimos restart del wrapper desde JS.
}

// ─── Sidecar Go (mi-tienda-print) ───────────────────────────────

fn spawn_agent_if_token(app: &AppHandle) -> Result<bool, String> {
    let state = app.state::<AgentState>();
    if state.child.lock().unwrap().is_some() {
        return Ok(true);
    }

    let Some(token) = read_token(app) else { return Ok(false); };

    let sidecar = app
        .shell()
        .sidecar("mi-tienda-print")
        .map_err(|e| format!("sidecar: {e}"))?
        .env("MI_TIENDA_AGENT_MANAGED", "1")
        .env("MI_TIENDA_AGENT_TOKEN", &token)
        // El sidecar usa MI_TIENDA_URL para WebSocket + config endpoint.
        // Lo matcheamos al SERVER_URL del wrapper para que prod y staging
        // queden coherentes.
        .env("MI_TIENDA_URL", SERVER_URL);

    let (mut rx, child) = sidecar.spawn().map_err(|e| format!("spawn: {e}"))?;

    {
        let mut slot = state.child.lock().unwrap();
        *slot = Some(child);
    }

    let app_for_drain = app.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(event) = rx.recv().await {
            match event {
                CommandEvent::Stdout(line) => {
                    println!("[agent] {}", String::from_utf8_lossy(&line).trim_end());
                }
                CommandEvent::Stderr(line) => {
                    eprintln!("[agent] {}", String::from_utf8_lossy(&line).trim_end());
                }
                CommandEvent::Terminated(payload) => {
                    eprintln!("[agent] terminated: code={:?}", payload.code);
                    if let Some(state) = app_for_drain.try_state::<AgentState>() {
                        *state.child.lock().unwrap() = None;
                    }
                    break;
                }
                _ => {}
            }
        }
    });

    Ok(true)
}

fn kill_agent(app: &AppHandle) {
    if let Some(state) = app.try_state::<AgentState>() {
        if let Some(child) = state.child.lock().unwrap().take() {
            let _ = child.kill();
        }
    }
}

// ─── Comandos invocables desde JS ───────────────────────────────

#[tauri::command]
async fn pair_agent(app: AppHandle, grant: String) -> Result<(), String> {
    let response = reqwest::Client::new()
        .post(format!("{SERVER_URL}/api/desktop_agent/exchange"))
        .json(&serde_json::json!({ "grant": grant }))
        .send()
        .await
        .map_err(|e| format!("exchange request: {e}"))?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!("exchange failed: HTTP {status}"));
    }

    let body: ExchangeResponse = response
        .json()
        .await
        .map_err(|e| format!("exchange parse: {e}"))?;

    write_token(&app, &body.token)?;

    kill_agent(&app);
    spawn_agent_if_token(&app)?;
    Ok(())
}

#[tauri::command]
fn agent_paired(app: AppHandle) -> bool {
    read_token(&app).is_some()
}

#[tauri::command]
fn agent_unpair(app: AppHandle) -> Result<(), String> {
    kill_agent(&app);
    delete_token(&app);
    Ok(())
}

// Bridge para descargas dentro del wrapper. WKWebView de Tauri no soporta
// <a download> ni Web Share API; acá abrimos un Save dialog nativo y
// escribimos los bytes que mandó JS. Útil para los reportes XLSX/CSV.
//
// Devuelve true si se guardó, false si el user canceló.
#[tauri::command]
async fn save_file_bytes(
    app: AppHandle,
    filename: String,
    bytes: Vec<u8>,
) -> Result<bool, String> {
    let dialog = app.dialog().clone();
    let path = tauri::async_runtime::spawn_blocking(move || {
        dialog.file().set_file_name(&filename).blocking_save_file()
    })
    .await
    .map_err(|e| format!("save dialog: {e}"))?;

    let Some(path) = path else { return Ok(false) };
    let path_buf = path.into_path().map_err(|e| format!("path resolve: {e}"))?;
    std::fs::write(&path_buf, bytes)
        .map_err(|e| format!("write {}: {e}", path_buf.display()))?;
    Ok(true)
}

// ─── Auto-update ────────────────────────────────────────────────

// Best-effort: chequea updates al boot y los aplica silenciosamente. Si no
// hay manifest, falla la red, o el endpoint no existe todavía, lo logueamos
// y seguimos. La app vieja sigue funcionando — peor caso, no se actualiza.
fn check_for_updates(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let updater = match app.updater() {
            Ok(u) => u,
            Err(e) => { eprintln!("[updater] init failed: {e}"); return; }
        };
        match updater.check().await {
            Ok(Some(update)) => {
                println!("[updater] update available: {}", update.version);
                if let Err(e) = update.download_and_install(|_, _| {}, || {}).await {
                    eprintln!("[updater] download/install failed: {e}");
                } else {
                    println!("[updater] installed; restart will apply it");
                }
            }
            Ok(None) => println!("[updater] up to date"),
            Err(e) => eprintln!("[updater] check failed: {e}"),
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .manage(AgentState { child: Mutex::new(None) })
        .invoke_handler(tauri::generate_handler![
            pair_agent,
            agent_paired,
            agent_unpair,
            save_file_bytes,
            kiosk_mode_enabled,
            set_kiosk_mode
        ])
        .setup(|app| {
            apply_kiosk_mode_if_set(app.handle());
            let _ = spawn_agent_if_token(app.handle());
            check_for_updates(app.handle().clone());
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let RunEvent::Exit = event {
                kill_agent(app);
            }
        });
}
