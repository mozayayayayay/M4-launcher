
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
};
use tauri::{AppHandle, Manager};
use uuid::Uuid;

fn data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherSettings {
    pub memory_mb: u32,
    pub java_path: Option<String>,
    pub resolution_width: u32,
    pub resolution_height: u32,
    pub fullscreen: bool,
    pub theme: String,
    pub accent_color: String,
    pub close_launcher_on_game_start: bool,
}

impl Default for LauncherSettings {
    fn default() -> Self {
        Self {
            memory_mb: 2048,
            java_path: None,
            resolution_width: 1280,
            resolution_height: 720,
            fullscreen: false,
            theme: "galaxy".into(),
            accent_color: "#9b8cff".into(),
            close_launcher_on_game_start: false,
        }
    }
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> Result<LauncherSettings, String> {
    let path = data_dir(&app)?.join("settings.json");

    if !path.exists() {
        return Ok(LauncherSettings::default());
    }

    let contents = fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&contents).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_settings(
    app: AppHandle,
    settings: LauncherSettings,
) -> Result<(), String> {
    if !(512..=65536).contains(&settings.memory_mb) {
        return Err("RAM allocation must be between 512 and 65536 MB.".into());
    }

    if !(640..=7680).contains(&settings.resolution_width)
        || !(480..=4320).contains(&settings.resolution_height)
    {
        return Err("The selected resolution is outside the supported range.".into());
    }

    if !settings.accent_color.starts_with('#')
        || settings.accent_color.len() != 7
        || !settings.accent_color[1..].chars().all(|c| c.is_ascii_hexdigit())
    {
        return Err("Accent color must be a six-digit hex color.".into());
    }

    let path = data_dir(&app)?.join("settings.json");
    let contents = serde_json::to_string_pretty(&settings)
        .map_err(|e| e.to_string())?;

    fs::write(path, contents).map_err(|e| e.to_string())
}

fn instance_game_dir(app: &AppHandle, instance_id: &str) -> Result<PathBuf, String> {
    let path = data_dir(app)?.join("instances.json");
    let contents = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let data: Value = serde_json::from_str(&contents).map_err(|e| e.to_string())?;

    let instances = data["instances"]
        .as_array()
        .ok_or("The instance library is invalid.")?;

    let instance = instances
        .iter()
        .find(|item| item["id"].as_str() == Some(instance_id))
        .ok_or("Instance not found.")?;

    let game_dir = instance["gameDir"]
        .as_str()
        .ok_or("Instance has no game directory.")?;

    Ok(PathBuf::from(game_dir))
}

#[tauri::command]
pub fn get_minecraft_options(
    app: AppHandle,
    instance_id: String,
) -> Result<HashMap<String, String>, String> {
    let options_path = instance_game_dir(&app, &instance_id)?.join("options.txt");

    if !options_path.exists() {
        return Ok(HashMap::new());
    }

    let contents = fs::read_to_string(options_path).map_err(|e| e.to_string())?;
    let mut options = HashMap::new();

    for line in contents.lines() {
        if let Some((key, value)) = line.split_once(':') {
            options.insert(key.to_string(), value.to_string());
        }
    }

    Ok(options)
}

#[tauri::command]
pub fn save_minecraft_options(
    app: AppHandle,
    instance_id: String,
    changes: HashMap<String, String>,
) -> Result<(), String> {
    const ALLOWED: &[&str] = &[
        "music",
        "sound",
        "mouseSensitivity",
        "fov",
        "gamma",
        "renderDistance",
        "simulationDistance",
        "guiScale",
        "maxFps",
        "enableVsync",
        "entityShadows",
        "viewBobbing",
        "ao",
        "particles",
        "fullscreen",
        "invertYMouse",
        "rawMouseInput",
    ];

    let options_path = instance_game_dir(&app, &instance_id)?.join("options.txt");

    if !options_path.exists() {
        return Err("Minecraft has not created options.txt for this instance yet. Launch it once first.".into());
    }

    let original = fs::read_to_string(&options_path).map_err(|e| e.to_string())?;
    let mut options: HashMap<String, String> = HashMap::new();
    let mut order: Vec<String> = Vec::new();

    for line in original.lines() {
        if let Some((key, value)) = line.split_once(':') {
            if !options.contains_key(key) {
                order.push(key.to_string());
            }
            options.insert(key.to_string(), value.to_string());
        }
    }

    for (key, value) in changes {
        if !ALLOWED.contains(&key.as_str()) {
            return Err(format!("Changing the Minecraft option '{}' is not supported.", key));
        }

        if value.contains('\n') || value.contains('\r') {
            return Err("Invalid option value.".into());
        }

        if !options.contains_key(&key) {
            order.push(key.clone());
        }

        options.insert(key, value);
    }

    let mut output = String::new();

    for key in order {
        if let Some(value) = options.get(&key) {
            output.push_str(&key);
            output.push(':');
            output.push_str(value);
            output.push('\n');
        }
    }

    fs::write(options_path, output).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfflineAccount {
    pub id: String,
    pub username: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AccountStore {
    pub accounts: Vec<OfflineAccount>,
    pub selected_account: Option<String>,
}

fn accounts_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(data_dir(app)?.join("accounts.json"))
}

fn read_accounts(app: &AppHandle) -> Result<AccountStore, String> {
    let path = accounts_path(app)?;

    if !path.exists() {
        return Ok(AccountStore::default());
    }

    let contents = fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&contents).map_err(|e| e.to_string())
}

fn write_accounts(app: &AppHandle, store: &AccountStore) -> Result<(), String> {
    let contents = serde_json::to_string_pretty(store).map_err(|e| e.to_string())?;
    fs::write(accounts_path(app)?, contents).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_accounts(app: AppHandle) -> Result<AccountStore, String> {
    read_accounts(&app)
}

#[tauri::command]
pub fn add_offline_account(
    app: AppHandle,
    username: String,
) -> Result<AccountStore, String> {
    let username = username.trim();

    if !(3..=16).contains(&username.len())
        || !username.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err("Use a Minecraft username with 3–16 letters, numbers, or underscores.".into());
    }

    let mut store = read_accounts(&app)?;

    if store.accounts.iter().any(|a| a.username.eq_ignore_ascii_case(username)) {
        return Err("That offline profile already exists.".into());
    }

    let account = OfflineAccount {
        id: Uuid::new_v4().to_string(),
        username: username.to_string(),
    };

    store.accounts.push(account.clone());
    store.selected_account = Some(account.id);
    write_accounts(&app, &store)?;

    Ok(store)
}

#[tauri::command]
pub fn select_account(
    app: AppHandle,
    account_id: String,
) -> Result<AccountStore, String> {
    let mut store = read_accounts(&app)?;

    if !store.accounts.iter().any(|a| a.id == account_id) {
        return Err("Account not found.".into());
    }

    store.selected_account = Some(account_id);
    write_accounts(&app, &store)?;

    Ok(store)
}

#[tauri::command]
pub fn delete_account(
    app: AppHandle,
    account_id: String,
) -> Result<AccountStore, String> {
    let mut store = read_accounts(&app)?;
    store.accounts.retain(|a| a.id != account_id);

    if store.selected_account.as_deref() == Some(&account_id) {
        store.selected_account = store.accounts.first().map(|a| a.id.clone());
    }

    write_accounts(&app, &store)?;
    Ok(store)
}

