use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
};
use tauri::{AppHandle, Manager};
use uuid::Uuid;

fn app_file(app: &AppHandle, filename: &str) -> Result<PathBuf, String> {
    let directory = app.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    Ok(directory.join(filename))
}

fn read_json<T: for<'de> Deserialize<'de> + Default>(
    path: &PathBuf,
) -> Result<T, String> {
    if !path.exists() {
        return Ok(T::default());
    }

    let contents = fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&contents).map_err(|e| e.to_string())
}

fn write_json<T: Serialize>(path: &PathBuf, value: &T) -> Result<(), String> {
    let contents = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    let temporary = path.with_extension("tmp");

    fs::write(&temporary, contents).map_err(|e| e.to_string())?;
    fs::rename(&temporary, path).map_err(|e| e.to_string())
}

// --------------------------------------------------
// Launcher settings
// --------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LauncherSettings {
    pub memory_mb: u32,
    pub java_path: String,
    pub resolution_width: u32,
    pub resolution_height: u32,
    pub fullscreen: bool,
    pub theme: String,
    pub accent_color: String,
    pub language: String,
    pub discord_rpc_enabled: bool,
}

impl Default for LauncherSettings {
    fn default() -> Self {
        Self {
            memory_mb: 4096,
            java_path: String::new(),
            resolution_width: 1280,
            resolution_height: 720,
            fullscreen: false,
            theme: "galaxy".into(),
            accent_color: "#9B8CFF".into(),
            language: "en".into(),
            discord_rpc_enabled: false,
        }
    }
}

#[tauri::command]
pub fn get_launcher_settings(app: AppHandle) -> Result<LauncherSettings, String> {
    let path = app_file(&app, "settings.json")?;
    read_json(&path)
}

#[tauri::command]
pub fn save_launcher_settings(
    app: AppHandle,
    settings: LauncherSettings,
) -> Result<LauncherSettings, String> {
    if !(512..=32768).contains(&settings.memory_mb) {
        return Err("Memory allocation must be between 512 MB and 32768 MB.".into());
    }

    if !(640..=7680).contains(&settings.resolution_width)
        || !(480..=4320).contains(&settings.resolution_height)
    {
        return Err("The selected resolution is outside the supported range.".into());
    }

    if !["galaxy", "dark"].contains(&settings.theme.as_str()) {
        return Err("Invalid launcher theme.".into());
    }

    if !["en", "ar"].contains(&settings.language.as_str()) {
        return Err("Invalid launcher language.".into());
    }

    let color = settings.accent_color.trim();

    if color.len() != 7
        || !color.starts_with('#')
        || !color[1..].chars().all(|c| c.is_ascii_hexdigit())
    {
        return Err("Accent color must be a hex color such as #9B8CFF.".into());
    }

    if settings.java_path.contains('\0') {
        return Err("Invalid settings value.".into());
    }

    let path = app_file(&app, "settings.json")?;
    write_json(&path, &settings)?;

    Ok(settings)
}

// --------------------------------------------------
// Minecraft options.txt
// --------------------------------------------------

const ALLOWED_OPTIONS: &[&str] = &[
    "music",
    "sound",
    "invertYMouse",
    "mouseSensitivity",
    "fov",
    "gamma",
    "renderDistance",
    "simulationDistance",
    "guiScale",
    "particles",
    "maxFps",
    "enableVsync",
    "fullscreen",
    "entityShadows",
    "cloudStatus",
    "graphicsMode",
    "mipmapLevels",
    "biomeBlendRadius",
    "entityDistanceScaling",
    "screenEffectScale",
    "showSubtitles",
    "toggleCrouch",
    "toggleSprint",
    "autoJump",
    "tutorialStep",
    "lang",
];

fn get_instance_game_dir(app: &AppHandle, instance_id: &str) -> Result<PathBuf, String> {
    let path = app_file(app, "instances.json")?;
    let contents = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let data: serde_json::Value =
        serde_json::from_str(&contents).map_err(|e| e.to_string())?;

    let instances = data["instances"]
        .as_array()
        .ok_or("Instance data is invalid.")?;

    let instance = instances
        .iter()
        .find(|item| item["id"].as_str() == Some(instance_id))
        .ok_or("Instance not found.")?;

    let game_dir = instance["gameDir"]
        .as_str()
        .ok_or("Instance game directory is missing.")?;

    Ok(PathBuf::from(game_dir))
}

#[tauri::command]
pub fn get_minecraft_options(
    app: AppHandle,
    instance_id: String,
) -> Result<BTreeMap<String, String>, String> {
    let game_dir = get_instance_game_dir(&app, &instance_id)?;
    let options_path = game_dir.join("options.txt");

    let mut options = BTreeMap::new();

    if !options_path.exists() {
        return Ok(options);
    }

    let contents = fs::read_to_string(options_path).map_err(|e| e.to_string())?;

    for line in contents.lines() {
        if let Some((key, value)) = line.split_once(':') {
            if ALLOWED_OPTIONS.contains(&key) {
                options.insert(key.to_string(), value.to_string());
            }
        }
    }

    Ok(options)
}

#[tauri::command]
pub fn save_minecraft_options(
    app: AppHandle,
    instance_id: String,
    changes: BTreeMap<String, String>,
) -> Result<(), String> {
    let game_dir = get_instance_game_dir(&app, &instance_id)?;
    fs::create_dir_all(&game_dir).map_err(|e| e.to_string())?;

    let options_path = game_dir.join("options.txt");
    let mut existing = BTreeMap::new();

    if options_path.exists() {
        let contents = fs::read_to_string(&options_path).map_err(|e| e.to_string())?;

        for line in contents.lines() {
            if let Some((key, value)) = line.split_once(':') {
                existing.insert(key.to_string(), value.to_string());
            }
        }
    }

    for (key, value) in changes {
        if !ALLOWED_OPTIONS.contains(&key.as_str()) {
            return Err(format!("Unsupported Minecraft option: {key}"));
        }

        if value.contains('\n') || value.contains('\r') || value.contains('\0') {
            return Err(format!("Invalid value for Minecraft option: {key}"));
        }

        existing.insert(key, value);
    }

    let mut output = String::new();

    for (key, value) in existing {
        output.push_str(&key);
        output.push(':');
        output.push_str(&value);
        output.push('\n');
    }

    fs::write(options_path, output).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn reset_minecraft_options(
    app: AppHandle,
    instance_id: String,
) -> Result<(), String> {
    let options_path = get_instance_game_dir(&app, &instance_id)?.join("options.txt");

    if options_path.exists() {
        fs::remove_file(options_path).map_err(|e| e.to_string())?;
    }

    Ok(())
}

// --------------------------------------------------
// Local offline profiles
// --------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalAccount {
    pub id: String,
    pub username: String,
    pub account_type: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountStore {
    accounts: Vec<LocalAccount>,
    selected_account: Option<String>,
}

#[tauri::command]
pub fn get_accounts(app: AppHandle) -> Result<AccountStoreResponse, String> {
    let path = app_file(&app, "accounts.json")?;
    let store: AccountStore = read_json(&path)?;

    Ok(AccountStoreResponse {
        accounts: store.accounts,
        selected_account: store.selected_account,
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountStoreResponse {
    pub accounts: Vec<LocalAccount>,
    pub selected_account: Option<String>,
}

#[tauri::command]
pub fn add_offline_account(
    app: AppHandle,
    username: String,
) -> Result<AccountStoreResponse, String> {
    let username = username.trim();

    if !(3..=16).contains(&username.len())
        || !username
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err(
            "Username must be 3–16 characters and contain only letters, numbers, or underscores."
                .into(),
        );
    }

    let path = app_file(&app, "accounts.json")?;
    let mut store: AccountStore = read_json(&path)?;

    if store.accounts.iter().any(|account| {
        account.username.eq_ignore_ascii_case(username)
    }) {
        return Err("That local profile already exists.".into());
    }

    let account = LocalAccount {
        id: Uuid::new_v4().to_string(),
        username: username.to_string(),
        account_type: "offline".into(),
    };

    store.selected_account = Some(account.id.clone());
    store.accounts.push(account);

    write_json(&path, &store)?;

    Ok(AccountStoreResponse {
        accounts: store.accounts,
        selected_account: store.selected_account,
    })
}

#[tauri::command]
pub fn select_account(
    app: AppHandle,
    id: String,
) -> Result<AccountStoreResponse, String> {
    let path = app_file(&app, "accounts.json")?;
    let mut store: AccountStore = read_json(&path)?;

    if !store.accounts.iter().any(|account| account.id == id) {
        return Err("Account not found.".into());
    }

    store.selected_account = Some(id);
    write_json(&path, &store)?;

    Ok(AccountStoreResponse {
        accounts: store.accounts,
        selected_account: store.selected_account,
    })
}

#[tauri::command]
pub fn delete_account(
    app: AppHandle,
    id: String,
) -> Result<AccountStoreResponse, String> {
    let path = app_file(&app, "accounts.json")?;
    let mut store: AccountStore = read_json(&path)?;

    store.accounts.retain(|account| account.id != id);

    if store.selected_account.as_deref() == Some(&id) {
        store.selected_account = store.accounts.first().map(|account| account.id.clone());
    }

    write_json(&path, &store)?;

    Ok(AccountStoreResponse {
        accounts: store.accounts,
        selected_account: store.selected_account,
    })
}
