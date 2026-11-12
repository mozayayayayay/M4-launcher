
mod settings_accounts;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha1::{Digest, Sha1};
use sha2::Sha256;
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{BufRead, BufReader, Cursor, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
};
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

const MOJANG_MANIFEST: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const MODRINTH_API: &str = "https://api.modrinth.com/v2";

// --------------------------------------------------
// Data structures
// --------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Instance {
    id: String,
    name: String,
    version: String,
    loader: String,
    loader_version: String,
    icon: String,
    status: String,
    game_dir: String,
    memory_mb: u32,
    java_path: Option<String>,
    #[serde(default)]
    launch_version_id: Option<String>,
    #[serde(default)]
    mods: usize,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct LauncherData {
    instances: Vec<Instance>,
    selected_instance: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LaunchResult {
    pid: u32,
    instance_id: String,
    username: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConsoleOutput {
    instance_id: String,
    stream: String,
    line: String,
}

fn pipe_console_output<R>(app: AppHandle, instance_id: String, stream: &'static str, reader: R)
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        for line in BufReader::new(reader).lines() {
            match line {
                Ok(line) => {
                    let _ = app.emit(
                        "console-output",
                        ConsoleOutput {
                            instance_id: instance_id.clone(),
                            stream: stream.to_string(),
                            line,
                        },
                    );
                }
                Err(error) => {
                    let _ = app.emit(
                        "console-output",
                        ConsoleOutput {
                            instance_id: instance_id.clone(),
                            stream: "system".to_string(),
                            line: format!("Could not read process output: {error}"),
                        },
                    );
                    break;
                }
            }
        }
    });
}

// --------------------------------------------------
// Data storage
// --------------------------------------------------

fn data_file(app: &AppHandle) -> Result<PathBuf, String> {
    let directory = app.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    Ok(directory.join("instances.json"))
}

fn load_data(app: &AppHandle) -> Result<LauncherData, String> {
    let path = data_file(app)?;

    if !path.exists() {
        return Ok(LauncherData::default());
    }

    let contents = fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&contents).map_err(|e| e.to_string())
}

fn save_data(app: &AppHandle, data: &LauncherData) -> Result<(), String> {
    let path = data_file(app)?;
    let contents = serde_json::to_string_pretty(data).map_err(|e| e.to_string())?;
    fs::write(path, contents).map_err(|e| e.to_string())
}

fn count_mods(instance: &Instance) -> usize {
    let path = PathBuf::from(&instance.game_dir).join("mods");

    fs::read_dir(path)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|entry| {
                    entry.path().extension().is_some_and(|ext| ext == "jar")
                })
                .count()
        })
        .unwrap_or(0)
}

// --------------------------------------------------
// Instance management
// --------------------------------------------------

#[tauri::command]
fn get_instances(app: AppHandle) -> Result<LauncherData, String> {
    let mut data = load_data(&app)?;

    let selection_exists = data.selected_instance.as_ref().is_some_and(|id| {
        data.instances.iter().any(|instance| &instance.id == id)
    });

    if !selection_exists {
        data.selected_instance = data.instances.first().map(|i| i.id.clone());
        save_data(&app, &data)?;
    }

    for instance in &mut data.instances {
        instance.mods = count_mods(instance);
    }

    Ok(data)
}

#[tauri::command]
fn create_instance(
    app: AppHandle,
    name: String,
    version: String,
    loader: String,
    loader_version: String,
) -> Result<Instance, String> {
    let mut data = load_data(&app)?;
    let id = Uuid::new_v4().to_string();

    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("instances")
        .join(&id);

    fs::create_dir_all(&base).map_err(|e| e.to_string())?;

    let instance = Instance {
        id: id.clone(),
        name,
        version,
        loader,
        loader_version,
        icon: String::new(),
        status: "Not installed".into(),
        game_dir: base.to_string_lossy().to_string(),
        memory_mb: 2048,
        java_path: None,
        launch_version_id: None,
        mods: 0,
    };

    if data.selected_instance.is_none() {
        data.selected_instance = Some(id);
    }

    data.instances.push(instance.clone());
    save_data(&app, &data)?;

    Ok(instance)
}

#[tauri::command]
fn delete_instance(app: AppHandle, id: String) -> Result<(), String> {
    let mut data = load_data(&app)?;

    if !data.instances.iter().any(|instance| instance.id == id) {
        return Err("Instance not found".into());
    }

    data.instances.retain(|instance| instance.id != id);

    if data.selected_instance.as_deref() == Some(&id) {
        data.selected_instance = data.instances.first().map(|i| i.id.clone());
    }

    save_data(&app, &data)
}

#[tauri::command]
fn select_instance(app: AppHandle, id: String) -> Result<(), String> {
    let mut data = load_data(&app)?;

    if !data.instances.iter().any(|instance| instance.id == id) {
        return Err("Instance not found".into());
    }

    data.selected_instance = Some(id);
    save_data(&app, &data)
}

#[tauri::command]
fn update_instance(
    app: AppHandle,
    id: String,
    name: String,
    version: String,
    loader: String,
    loader_version: String,
) -> Result<Instance, String> {
    let mut data = load_data(&app)?;

    let instance = data
        .instances
        .iter_mut()
        .find(|i| i.id == id)
        .ok_or_else(|| "Instance not found".to_string())?;

    instance.name = name;
    instance.version = version;
    instance.loader = loader;
    instance.loader_version = loader_version;

    let updated = instance.clone();
    save_data(&app, &data)?;

    Ok(updated)
}

// --------------------------------------------------
// Java and folders
// --------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JavaInfo {
    installed: bool,
    executable: Option<String>,
    version: Option<String>,
}

#[tauri::command]
fn detect_java() -> JavaInfo {
    match Command::new("java").arg("-version").output() {
        Ok(output) if output.status.success() || !output.stderr.is_empty() => {
            let text = String::from_utf8_lossy(&output.stderr).to_string();

            JavaInfo {
                installed: true,
                executable: Some("java".into()),
                version: Some(
                    text.lines()
                        .next()
                        .unwrap_or("Java detected")
                        .to_string(),
                ),
            }
        }
        _ => JavaInfo {
            installed: false,
            executable: None,
            version: None,
        },
    }
}

#[tauri::command]
fn open_instance_folder(id: String, app: AppHandle) -> Result<(), String> {
    let data = load_data(&app)?;

    let instance = data
        .instances
        .iter()
        .find(|i| i.id == id)
        .ok_or_else(|| "Instance not found".to_string())?;

    let path = PathBuf::from(&instance.game_dir);
    fs::create_dir_all(&path).map_err(|e| e.to_string())?;

    #[cfg(target_os = "linux")]
    let result = Command::new("xdg-open").arg(&path).spawn();

    #[cfg(target_os = "windows")]
    let result = Command::new("explorer").arg(&path).spawn();

    #[cfg(target_os = "macos")]
    let result = Command::new("open").arg(&path).spawn();

    result.map(|_| ()).map_err(|e| e.to_string())
}

// --------------------------------------------------
// Download helpers
// --------------------------------------------------

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct InstallProgress {
    instance_id: String,
    completed: usize,
    total: usize,
    message: String,
}

fn emit_progress(
    app: &AppHandle,
    id: &str,
    completed: usize,
    total: usize,
    message: impl Into<String>,
) {
    let _ = app.emit(
        "install-progress",
        InstallProgress {
            instance_id: id.to_string(),
            completed,
            total,
            message: message.into(),
        },
    );
}

fn sha1_matches(path: &Path, expected: &str) -> bool {
    if expected.is_empty() || !path.exists() {
        return false;
    }

    let Ok(bytes) = fs::read(path) else {
        return false;
    };

    let mut hasher = Sha1::new();
    hasher.update(bytes);

    hex::encode(hasher.finalize()).eq_ignore_ascii_case(expected)
}

async fn download_file(
    client: &reqwest::Client,
    url: &str,
    destination: &Path,
    expected_sha1: Option<&str>,
) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err("Downloads must use HTTPS".into());
    }

    if let Some(hash) = expected_sha1 {
        if sha1_matches(destination, hash) {
            return Ok(());
        }
    } else if destination.exists() {
        return Ok(());
    }

    let parent = destination
        .parent()
        .ok_or("Invalid download destination")?;

    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|e| e.to_string())?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Download request failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Download failed: {e}"))?;

    let bytes = response.bytes().await.map_err(|e| e.to_string())?;

    if let Some(expected) = expected_sha1 {
        let mut hasher = Sha1::new();
        hasher.update(&bytes);

        let actual = hex::encode(hasher.finalize());

        if !actual.eq_ignore_ascii_case(expected) {
            return Err(format!("SHA-1 verification failed for {url}"));
        }
    }

    let filename = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("Invalid download filename")?;

    let temporary_path = parent.join(format!(
        ".{filename}.{}.tmp",
        Uuid::new_v4()
    ));

    tokio::fs::write(&temporary_path, &bytes)
        .await
        .map_err(|e| e.to_string())?;

    if let Err(error) = tokio::fs::rename(&temporary_path, destination).await {
        let _ = tokio::fs::remove_file(&temporary_path).await;
        return Err(format!("Could not save downloaded file: {error}"));
    }

    Ok(())
}

async fn download_many(
    client: &reqwest::Client,
    downloads: Vec<(String, PathBuf, Option<String>)>,
) -> Result<(), String> {
    // Keep network concurrency bounded: Minecraft asset manifests can contain
    // thousands of entries, so never create an unbounded number of requests.
    for batch in downloads.chunks(8) {
        let mut tasks = tokio::task::JoinSet::new();

        for (url, destination, expected_sha1) in batch.iter().cloned() {
            let client = client.clone();
            tasks.spawn(async move {
                download_file(
                    &client,
                    &url,
                    &destination,
                    expected_sha1.as_deref(),
                )
                .await
            });
        }

        while let Some(result) = tasks.join_next().await {
            result.map_err(|error| format!("Download worker failed: {error}"))??;
        }
    }

    Ok(())
}

fn library_allowed(library: &Value) -> bool {
    let Some(rules) = library.get("rules").and_then(Value::as_array) else {
        return true;
    };

    let mut allowed = false;

    for rule in rules {
        let action = rule.get("action").and_then(Value::as_str).unwrap_or("");
        let os = rule.get("os").and_then(Value::as_object);

        let matches_os = match os {
            None => true,
            Some(os) => {
                let name_matches = os
                    .get("name")
                    .and_then(Value::as_str)
                    .is_none_or(|name| name == std::env::consts::OS);

                let arch_matches = os
                    .get("arch")
                    .and_then(Value::as_str)
                    .is_none_or(|arch| arch == std::env::consts::ARCH);

                name_matches && arch_matches
            }
        };

        if matches_os {
            allowed = action == "allow";
        }
    }

    allowed
}

fn safe_relative_path(value: &str) -> Result<PathBuf, String> {
    let normalized = value.replace('\\', "/");
    let path = Path::new(&normalized);

    if normalized.is_empty()
        || normalized.starts_with('/')
        || normalized.contains(':')
        || normalized.contains('\0')
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(format!("Unsafe file path: {value}"));
    }

    Ok(path.to_path_buf())
}

// --------------------------------------------------
// Official Minecraft version manifest
// --------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MinecraftVersion {
    id: String,
    version_type: String,
    url: String,
    release_time: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MinecraftVersionList {
    latest_release: String,
    latest_snapshot: String,
    versions: Vec<MinecraftVersion>,
}

#[tauri::command]
async fn get_minecraft_versions() -> Result<MinecraftVersionList, String> {
    let client = reqwest::Client::builder()
        .user_agent("M0Z4-Launcher/0.1.0")
        .build()
        .map_err(|e| e.to_string())?;

    let manifest: Value = client
        .get(MOJANG_MANIFEST)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let latest_release = manifest["latest"]["release"]
        .as_str()
        .unwrap_or("")
        .to_string();

    let latest_snapshot = manifest["latest"]["snapshot"]
        .as_str()
        .unwrap_or("")
        .to_string();

    let versions = manifest["versions"]
        .as_array()
        .ok_or("Invalid Minecraft version manifest")?
        .iter()
        .filter_map(|item| {
            Some(MinecraftVersion {
                id: item["id"].as_str()?.to_string(),
                version_type: item["type"].as_str()?.to_string(),
                url: item["url"].as_str()?.to_string(),
                release_time: item["releaseTime"].as_str()?.to_string(),
            })
        })
        .collect();

    Ok(MinecraftVersionList {
        latest_release,
        latest_snapshot,
        versions,
    })
}

// --------------------------------------------------
// Vanilla Minecraft installation
// --------------------------------------------------

#[tauri::command]
async fn install_minecraft_version(
    app: AppHandle,
    id: String,
    version: String,
) -> Result<(), String> {
    let mut data = load_data(&app)?;

    let instance_index = data
        .instances
        .iter()
        .position(|instance| instance.id == id)
        .ok_or_else(|| "Instance not found".to_string())?;

    let game_dir = PathBuf::from(&data.instances[instance_index].game_dir);

    let client = reqwest::Client::builder()
        .user_agent("M0Z4-Launcher/0.1.0")
        .build()
        .map_err(|e| e.to_string())?;

    emit_progress(&app, &id, 0, 1, "Fetching Minecraft version manifest");

    let manifest: Value = client
        .get(MOJANG_MANIFEST)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let version_entry = manifest["versions"]
        .as_array()
        .and_then(|versions| {
            versions
                .iter()
                .find(|entry| entry["id"].as_str() == Some(version.as_str()))
        })
        .ok_or_else(|| format!("Minecraft version {version} was not found"))?;

    let version_url = version_entry["url"]
        .as_str()
        .ok_or("Version metadata URL is missing")?;

    let version_json: Value = client
        .get(version_url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let version_dir = game_dir.join("versions").join(&version);

    tokio::fs::create_dir_all(&version_dir)
        .await
        .map_err(|e| e.to_string())?;

    let metadata_path = version_dir.join(format!("{version}.json"));

    tokio::fs::write(
        &metadata_path,
        serde_json::to_vec_pretty(&version_json).map_err(|e| e.to_string())?,
    )
    .await
    .map_err(|e| e.to_string())?;

    let client_download = &version_json["downloads"]["client"];

    let client_url = client_download["url"]
        .as_str()
        .ok_or("Minecraft client download is missing")?;

    let client_path = version_dir.join(format!("{version}.jar"));

    emit_progress(&app, &id, 0, 1, "Downloading Minecraft client");

    download_file(
        &client,
        client_url,
        &client_path,
        client_download["sha1"].as_str(),
    )
    .await?;

    let libraries = version_json["libraries"]
        .as_array()
        .ok_or("Minecraft libraries list is missing")?;

    let mut library_downloads: Vec<(String, PathBuf, Option<String>)> = Vec::new();
    let mut total_libraries = 0usize;

    for library in libraries {
        if !library_allowed(library) {
            continue;
        }

        total_libraries += 1;
        let artifact = &library["downloads"]["artifact"];

        if let (Some(url), Some(relative_path)) = (
            artifact["url"].as_str(),
            artifact["path"].as_str(),
        ) {
            let relative = safe_relative_path(relative_path)?;
            let destination = game_dir.join("libraries").join(relative);
            library_downloads.push((
                url.to_string(),
                destination,
                artifact["sha1"].as_str().map(str::to_string),
            ));
        }
    }

    download_many(&client, library_downloads).await?;
    emit_progress(
        &app,
        &id,
        total_libraries,
        total_libraries.max(1),
        "Minecraft libraries ready",
    );

    if let Some(asset_index) = version_json.get("assetIndex") {
        let index_url = asset_index["url"]
            .as_str()
            .ok_or("Asset index URL is missing")?;

        let index_id = asset_index["id"]
            .as_str()
            .ok_or("Asset index ID is missing")?;

        let index_path = game_dir
            .join("assets")
            .join("indexes")
            .join(format!("{index_id}.json"));

        download_file(
            &client,
            index_url,
            &index_path,
            asset_index["sha1"].as_str(),
        )
        .await?;

        let index_contents = tokio::fs::read(&index_path)
            .await
            .map_err(|e| e.to_string())?;

        let index_json: Value =
            serde_json::from_slice(&index_contents).map_err(|e| e.to_string())?;

        let objects = index_json["objects"]
            .as_object()
            .ok_or("Asset index objects are missing")?;

        let total_assets = objects.len().max(1);
        let mut asset_downloads: Vec<(String, PathBuf, Option<String>)> =
            Vec::with_capacity(objects.len());

        for object in objects.values() {
            let hash = object["hash"]
                .as_str()
                .ok_or("Asset hash is missing")?;

            if hash.len() != 40 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err("Invalid Minecraft asset hash".into());
            }

            let destination = game_dir
                .join("assets")
                .join("objects")
                .join(&hash[0..2])
                .join(hash);

            let url = format!(
                "https://resources.download.minecraft.net/{}/{}",
                &hash[0..2],
                hash
            );

            asset_downloads.push((url, destination, Some(hash.to_string())));
        }

        let mut finished_assets = 0usize;
        for batch in asset_downloads.chunks(8) {
            download_many(&client, batch.to_vec()).await?;
            finished_assets += batch.len();

            emit_progress(
                &app,
                &id,
                finished_assets,
                total_assets,
                "Downloading Minecraft assets",
            );
        }
    }

    let mod_count = count_mods(&data.instances[instance_index]);

    if data.instances[instance_index].version != version {
        data.instances[instance_index].launch_version_id = None;
        data.instances[instance_index].loader = "Vanilla".into();
        data.instances[instance_index].loader_version = "—".into();
    }
    data.instances[instance_index].version = version;
    data.instances[instance_index].status = "Installed".into();
    data.instances[instance_index].mods = mod_count;

    save_data(&app, &data)?;

    emit_progress(&app, &id, 1, 1, "Minecraft files downloaded");

    Ok(())
}

// --------------------------------------------------
// Minecraft launch helpers
// --------------------------------------------------

fn read_json_file(path: &Path) -> Result<Value, String> {
    let contents = fs::read_to_string(path)
        .map_err(|e| format!("Could not read {}: {e}", path.display()))?;

    serde_json::from_str(&contents)
        .map_err(|e| format!("Invalid JSON in {}: {e}", path.display()))
}

fn selected_offline_account(app: &AppHandle) -> Result<(String, String), String> {
    let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let path = app_dir.join("accounts.json");

    if !path.exists() {
        return Err("No account found. Add an offline account first.".into());
    }

    let store = read_json_file(&path)?;

    let selected_id = store["selectedAccount"]
        .as_str()
        .or_else(|| store["selected_account"].as_str())
        .ok_or("No account is selected")?;

    let accounts = store["accounts"]
        .as_array()
        .ok_or("Account list is missing")?;

    let account = accounts
        .iter()
        .find(|account| account["id"].as_str() == Some(selected_id))
        .ok_or("Selected account could not be found")?;

    let username = account["username"]
        .as_str()
        .ok_or("Selected account has no username")?
        .to_string();

    let uuid = account["id"]
        .as_str()
        .ok_or("Selected account has no ID")?
        .replace('-', "");

    if username.is_empty() || uuid.len() != 32 {
        return Err("Selected offline account is invalid".into());
    }

    Ok((username, uuid))
}

fn launcher_settings(app: &AppHandle) -> Result<Value, String> {
    let path = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("settings.json");

    if !path.exists() {
        return Ok(Value::Object(Default::default()));
    }

    read_json_file(&path)
}

fn json_string<'a>(value: &'a Value, camel: &str, snake: &str) -> Option<&'a str> {
    value[camel]
        .as_str()
        .or_else(|| value[snake].as_str())
}

fn json_u32(value: &Value, camel: &str, snake: &str) -> Option<u32> {
    value[camel]
        .as_u64()
        .or_else(|| value[snake].as_u64())
        .and_then(|n| u32::try_from(n).ok())
}

fn required_java_major(version_json: &Value, minecraft_version: &str) -> u32 {
    if let Some(major) = version_json["javaVersion"]["majorVersion"].as_u64() {
        return major as u32;
    }

    // Compatibility fallback for old or third-party version metadata.
    let parts: Vec<u32> = minecraft_version
        .split('.')
        .filter_map(|part| part.parse::<u32>().ok())
        .collect();
    if parts.first() == Some(&1) {
        let minor = *parts.get(1).unwrap_or(&0);
        let patch = *parts.get(2).unwrap_or(&0);
        if minor <= 16 { 8 }
        else if minor == 17 { 16 }
        else if minor < 20 || (minor == 20 && patch < 5) { 17 }
        else if minor <= 21 { 21 }
        else { 25 }
    } else if parts.first().is_some_and(|major| *major >= 26) {
        // Minecraft's post-1.x version scheme (for example 26.2) uses Java 25.
        25
    } else {
        21
    }
}

fn java_executable_name() -> &'static str {
    if cfg!(windows) { "java.exe" } else { "java" }
}

fn java_major_version(executable: &Path) -> Option<u32> {
    let output = Command::new(executable).arg("-version").output().ok()?;
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    for line in text.lines() {
        if !line.to_ascii_lowercase().contains("version") {
            continue;
        }
        let version = line.split('"').nth(1).unwrap_or(line);
        let mut parts = version.split('.');
        let first = parts.next()?.trim_matches(|c: char| !c.is_ascii_digit()).parse::<u32>().ok()?;
        if first == 1 {
            return parts.next()?.split(|c: char| !c.is_ascii_digit()).next()?.parse().ok();
        }
        return Some(first);
    }
    None
}

fn find_java_in_directory(directory: &Path, depth: usize) -> Option<PathBuf> {
    if depth > 7 { return None; }
    let entries = fs::read_dir(directory).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file()
            && path.file_name().and_then(|name| name.to_str()) == Some(java_executable_name())
        {
            return Some(path);
        }
        if path.is_dir() {
            if let Some(found) = find_java_in_directory(&path, depth + 1) {
                return Some(found);
            }
        }
    }
    None
}

fn installed_java_candidates(preferred: Option<&str>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(value) = preferred.filter(|value| !value.trim().is_empty()) {
        candidates.push(PathBuf::from(value));
    }

    if let Some(home) = std::env::var_os("JAVA_HOME") {
        candidates.push(PathBuf::from(home).join("bin").join(java_executable_name()));
        #[cfg(target_os = "macos")]
        candidates.push(PathBuf::from(home).join("Contents/Home/bin/java"));
    }

    if let Some(path) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&path) {
            candidates.push(directory.join(java_executable_name()));
        }
    }

    #[cfg(target_os = "linux")]
    for root in ["/usr/lib/jvm", "/usr/java", "/opt/java"] {
        if let Some(found) = find_java_in_directory(Path::new(root), 0) {
            candidates.push(found);
        }
    }

    #[cfg(target_os = "macos")]
    if let Some(found) = find_java_in_directory(Path::new("/Library/Java/JavaVirtualMachines"), 0) {
        candidates.push(found);
    }

    #[cfg(target_os = "windows")]
    for variable in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
        if let Some(root) = std::env::var_os(variable) {
            for folder in ["Java", "Eclipse Adoptium", "Microsoft"] {
                if let Some(found) = find_java_in_directory(&PathBuf::from(&root).join(folder), 0) {
                    candidates.push(found);
                }
            }
        }
    }

    candidates
}

async fn resolve_java_runtime(
    app: &AppHandle,
    client: &reqwest::Client,
    version_json: &Value,
    minecraft_version: &str,
    preferred: Option<&str>,
    instance_id: &str,
) -> Result<PathBuf, String> {
    let required = required_java_major(version_json, minecraft_version);

    for candidate in installed_java_candidates(preferred) {
        if java_major_version(&candidate) == Some(required) {
            return Ok(candidate);
        }
    }

    let os = if cfg!(target_os = "windows") { "windows" }
        else if cfg!(target_os = "macos") { "mac" }
        else { "linux" };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "aarch64",
        "x86" => "x86",
        other => return Err(format!("Automatic Java {required} installation is not supported on CPU architecture {other}.")),
    };

    let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let runtime_dir = app_dir.join("runtimes").join(format!("temurin-{required}-{os}-{arch}"));
    if let Some(found) = find_java_in_directory(&runtime_dir, 0) {
        if java_major_version(&found) == Some(required) {
            return Ok(found);
        }
    }

    emit_progress(app, instance_id, 0, 1, format!("Downloading Java {required} runtime"));
    fs::create_dir_all(&runtime_dir).map_err(|e| format!("Could not create Java runtime directory: {e}"))?;

    let extension = if cfg!(windows) { "zip" } else { "tar.gz" };
    let archive_path = runtime_dir.join(format!("temurin-{required}.{extension}"));
    let url = format!(
        "https://api.adoptium.net/v3/binary/latest/{required}/ga/{os}/{arch}/jdk/hotspot/normal/eclipse"
    );
    let response = client.get(&url).send().await
        .map_err(|e| format!("Could not download Java {required}: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Java {required} download failed: {e}"))?;
    let bytes = response.bytes().await.map_err(|e| format!("Could not read Java {required} download: {e}"))?;
    tokio::fs::write(&archive_path, &bytes).await
        .map_err(|e| format!("Could not save Java {required} runtime: {e}"))?;

    emit_progress(app, instance_id, 0, 1, format!("Extracting Java {required} runtime"));
    #[cfg(target_os = "windows")]
    {
        let file = fs::File::open(&archive_path).map_err(|e| e.to_string())?;
        let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("Invalid Java runtime archive: {e}"))?;
        archive.extract(&runtime_dir).map_err(|e| format!("Could not extract Java {required}: {e}"))?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        let status = Command::new("tar")
            .arg("-xzf").arg(&archive_path)
            .arg("-C").arg(&runtime_dir)
            .status()
            .map_err(|e| format!("Could not extract Java {required}; tar is unavailable: {e}"))?;
        if !status.success() {
            return Err(format!("Could not extract the Java {required} runtime archive."));
        }
    }

    let executable = find_java_in_directory(&runtime_dir, 0)
        .ok_or_else(|| format!("Java {required} downloaded, but its executable could not be found."))?;
    let detected = java_major_version(&executable);
    if detected != Some(required) {
        return Err(format!(
            "Downloaded Java runtime reported version {:?}; Minecraft requires Java {required}.",
            detected
        ));
    }

    let _ = fs::remove_file(archive_path);
    Ok(executable)
}


fn current_os_name() -> &'static str {
    #[cfg(target_os = "linux")]
    {
        "linux"
    }
    #[cfg(target_os = "windows")]
    {
        "windows"
    }
    #[cfg(target_os = "macos")]
    {
        "osx"
    }
}

fn rule_matches(rule: &Value, features: &HashMap<String, bool>) -> bool {
    if let Some(os) = rule.get("os").and_then(Value::as_object) {
        if let Some(name) = os.get("name").and_then(Value::as_str) {
            if name != current_os_name() {
                return false;
            }
        }

        if let Some(arch) = os.get("arch").and_then(Value::as_str) {
            if arch != std::env::consts::ARCH {
                return false;
            }
        }
    }

    if let Some(required_features) = rule.get("features").and_then(Value::as_object) {
        for (name, required) in required_features {
            let expected = required.as_bool().unwrap_or(false);
            let actual = features.get(name).copied().unwrap_or(false);

            if actual != expected {
                return false;
            }
        }
    }

    true
}

fn argument_allowed(value: &Value, features: &HashMap<String, bool>) -> Vec<String> {
    if let Some(text) = value.as_str() {
        return vec![text.to_string()];
    }

    if let Some(array) = value.as_array() {
        return array
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
    }

    if let Some(object) = value.as_object() {
        let rules = object.get("rules").and_then(Value::as_array);

        let mut allowed = rules.is_none();

        if let Some(rules) = rules {
            for rule in rules {
                if rule_matches(rule, features) {
                    allowed = rule["action"].as_str() == Some("allow");
                }
            }
        }

        if !allowed {
            return Vec::new();
        }

        return argument_allowed(
            object.get("value").unwrap_or(&Value::Null),
            features,
        );
    }

    Vec::new()
}

fn tokenize_legacy_arguments(input: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;

    for character in input.chars() {
        match character {
            '"' | '\'' if quote == Some(character) => quote = None,
            '"' | '\'' if quote.is_none() => quote = Some(character),
            c if c.is_whitespace() && quote.is_none() => {
                if !current.is_empty() {
                    result.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }

    if !current.is_empty() {
        result.push(current);
    }

    result
}

fn substitute_arguments(
    arguments: Vec<String>,
    replacements: &HashMap<String, String>,
) -> Vec<String> {
    arguments
        .into_iter()
        .map(|mut argument| {
            for (key, value) in replacements {
                argument = argument.replace(key, value);
            }
            argument
        })
        .collect()
}

fn extract_natives(
    archive_path: &Path,
    destination: &Path,
    excludes: &[String],
) -> Result<(), String> {
    let file = fs::File::open(archive_path)
        .map_err(|e| format!("Could not open native archive: {e}"))?;

    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| format!("Invalid native archive: {e}"))?;

    fs::create_dir_all(destination).map_err(|e| e.to_string())?;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|e| e.to_string())?;

        if entry.is_dir() {
            continue;
        }

        let Some(relative) = entry.enclosed_name() else {
            continue;
        };

        let relative_string = relative.to_string_lossy().replace('\\', "/");

        if relative_string.starts_with("META-INF/")
            || excludes.iter().any(|exclude| relative_string.starts_with(exclude))
        {
            continue;
        }

        let safe_relative = safe_relative_path(&relative_string)?;
        let output_path = destination.join(safe_relative);

        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        let mut contents = Vec::new();
        entry
            .read_to_end(&mut contents)
            .map_err(|e| e.to_string())?;

        fs::write(output_path, contents).map_err(|e| e.to_string())?;
    }

    Ok(())
}


fn maven_artifact_path(coordinate: &str) -> Option<String> {
    let (coordinate, extension) = coordinate.split_once('@').map(|(c, e)| (c, e)).unwrap_or((coordinate, "jar"));
    let parts: Vec<&str> = coordinate.split(':').collect();
    if parts.len() < 3 { return None; }
    let group = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    let classifier = parts.get(3).copied();
    let suffix = classifier.map(|c| format!("-{c}")).unwrap_or_default();
    Some(format!("{group}/{artifact}/{version}/{artifact}-{version}{suffix}.{extension}"))
}

async fn prepare_libraries_and_natives(
    client: &reqwest::Client,
    version_json: &Value,
    game_dir: &Path,
    version: &str,
) -> Result<(String, PathBuf), String> {
    let libraries = version_json["libraries"]
        .as_array()
        .ok_or("Minecraft libraries list is missing")?;

    let natives_dir = game_dir
        .join("versions")
        .join(version)
        .join("natives");

    fs::create_dir_all(&natives_dir).map_err(|e| e.to_string())?;

    let mut classpath_paths: Vec<PathBuf> = Vec::new();
    let mut downloads: Vec<(String, PathBuf, Option<String>)> = Vec::new();
    let mut native_archives: Vec<(PathBuf, Vec<String>)> = Vec::new();

    for library in libraries {
        if !library_allowed(library) {
            continue;
        }

        let artifact = &library["downloads"]["artifact"];
        let fallback_path = library["name"].as_str().and_then(maven_artifact_path);
        let relative_path = artifact["path"].as_str().or(fallback_path.as_deref());

        if let Some(relative_path) = relative_path {
            let relative = safe_relative_path(relative_path)?;
            let artifact_path = game_dir.join("libraries").join(relative);

            let fallback_url = library["url"]
                .as_str()
                .unwrap_or("https://libraries.minecraft.net/");
            let url = artifact["url"].as_str().map(str::to_string).or_else(|| {
                fallback_path.as_ref().map(|path| {
                    format!(
                        "{}{}",
                        fallback_url.trim_end_matches('/'),
                        format!("/{path}")
                    )
                })
            });

            if let Some(url) = url {
                if !url.starts_with("https://") {
                    return Err("A modloader library supplied a non-HTTPS URL".into());
                }
                downloads.push((
                    url,
                    artifact_path.clone(),
                    artifact["sha1"].as_str().map(str::to_string),
                ));
            }

            classpath_paths.push(artifact_path);
        }

        let natives = &library["natives"];
        let native_os = current_os_name();

        if let Some(native_template) = natives[native_os].as_str() {
            let arch = if std::env::consts::ARCH == "x86_64" {
                "64"
            } else {
                "32"
            };

            let classifier = native_template.replace("${arch}", arch);
            let native_download = &library["downloads"]["classifiers"][&classifier];

            if let (Some(url), Some(relative_path)) = (
                native_download["url"].as_str(),
                native_download["path"].as_str(),
            ) {
                let relative = safe_relative_path(relative_path)?;
                let archive_path = game_dir.join("libraries").join(relative);
                downloads.push((
                    url.to_string(),
                    archive_path.clone(),
                    native_download["sha1"].as_str().map(str::to_string),
                ));

                let excludes: Vec<String> = library["extract"]["exclude"]
                    .as_array()
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default();

                native_archives.push((archive_path, excludes));
            }
        }
    }

    download_many(client, downloads).await?;

    for (archive_path, excludes) in native_archives {
        extract_natives(&archive_path, &natives_dir, &excludes)?;
    }

    let classpath_entries: Vec<String> = classpath_paths
        .into_iter()
        .filter(|path| path.exists())
        .map(|path| path.to_string_lossy().to_string())
        .collect();

    let classpath_separator = if cfg!(windows) { ";" } else { ":" };
    Ok((classpath_entries.join(classpath_separator), natives_dir))
}


// --------------------------------------------------
// Modloader installation (Fabric, Quilt, Forge, NeoForge)
// --------------------------------------------------

fn merge_loader_profile(base: &Value, profile: &Value, launch_id: &str) -> Value {
    let mut merged = base.clone();
    if let Some(object) = merged.as_object_mut() {
        object.insert("id".into(), Value::String(launch_id.to_string()));
        if let Some(main_class) = profile.get("mainClass") { object.insert("mainClass".into(), main_class.clone()); }
        if let Some(libraries) = profile.get("libraries").and_then(Value::as_array) {
            let mut all = object.get("libraries").and_then(Value::as_array).cloned().unwrap_or_default();
            all.extend(libraries.clone());
            object.insert("libraries".into(), Value::Array(all));
        }
        if let Some(profile_args) = profile.get("arguments").and_then(Value::as_object) {
            let mut arguments = object.get("arguments").and_then(Value::as_object).cloned().unwrap_or_default();
            for key in ["jvm", "game"] {
                if let Some(extra) = profile_args.get(key).and_then(Value::as_array) {
                    let mut combined = arguments.get(key).and_then(Value::as_array).cloned().unwrap_or_default();
                    combined.extend(extra.clone());
                    arguments.insert(key.to_string(), Value::Array(combined));
                }
            }
            object.insert("arguments".into(), Value::Object(arguments));
        }
        if let Some(legacy) = profile.get("minecraftArguments") { object.insert("minecraftArguments".into(), legacy.clone()); }
        if let Some(logging) = profile.get("logging") { object.insert("logging".into(), logging.clone()); }
    }
    merged
}

async fn latest_maven_version(
    client: &reqwest::Client,
    metadata_url: &str,
    required_prefix: &str,
) -> Result<String, String> {
    let xml = client
        .get(metadata_url)
        .send()
        .await
        .map_err(|e| format!("Could not fetch loader versions: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Loader version service returned an error: {e}"))?
        .text()
        .await
        .map_err(|e| format!("Could not read loader version metadata: {e}"))?;

    let mut versions: Vec<String> = xml
        .split("<version>")
        .skip(1)
        .filter_map(|part| part.split_once("</version>").map(|(value, _)| value.trim().to_string()))
        .filter(|value| value.starts_with(required_prefix))
        .collect();

    versions.sort_by(|a, b| {
        let numeric = |value: &str| -> Vec<u64> {
            value
                .split(|character: char| !character.is_ascii_digit())
                .filter(|part| !part.is_empty())
                .filter_map(|part| part.parse::<u64>().ok())
                .collect()
        };
        numeric(a).cmp(&numeric(b))
    });

    versions.pop().ok_or_else(|| {
        format!("No compatible loader version was found for Minecraft {required_prefix}.")
    })
}

#[tauri::command]
async fn install_modloader(
    app: AppHandle,
    instance_id: String,
    loader: String,
    loader_version: Option<String>,
) -> Result<String, String> {
    let mut data = load_data(&app)?;
    let index = data.instances.iter().position(|i| i.id == instance_id)
        .ok_or_else(|| "Instance not found".to_string())?;
    let mut instance = data.instances[index].clone();
    let loader_key = loader.to_lowercase();
    if !["fabric", "quilt", "forge", "neoforge"].contains(&loader_key.as_str()) {
        return Err("Supported loaders are Fabric, Quilt, Forge and NeoForge.".into());
    }

    let client = reqwest::Client::builder().user_agent("M0Z4-Launcher/0.1.0")
        .build().map_err(|e| e.to_string())?;
    let game_dir = PathBuf::from(&instance.game_dir);
    emit_progress(&app, &instance_id, 0, 4, format!("Resolving {loader_key} for Minecraft {}", instance.version));
    let base_json_path = game_dir.join("versions").join(&instance.version).join(format!("{}.json", instance.version));
    if !base_json_path.exists() {
        install_minecraft_version(app.clone(), instance_id.clone(), instance.version.clone()).await?;
    }
    let base_json = read_json_file(&base_json_path)?;

    let (resolved_version, profile) = if loader_key == "fabric" || loader_key == "quilt" {
        let api_base = if loader_key == "fabric" { "https://meta.fabricmc.net/v2/versions/loader" } else { "https://meta.quiltmc.org/v3/versions/loader" };
        let requested = loader_version.unwrap_or_default();
        let selected = if requested.trim().is_empty() {
            let list: Value = client.get(format!("{api_base}/{}", instance.version)).send().await.map_err(|e| e.to_string())?
                .error_for_status().map_err(|e| e.to_string())?.json().await.map_err(|e| e.to_string())?;
            list.as_array().and_then(|items| items.first()).and_then(|item| item["loader"]["version"].as_str())
                .ok_or("No loader version was returned by the official metadata service")?.to_string()
        } else { requested };
        emit_progress(&app, &instance_id, 1, 4, format!("Fetching {loader_key} launch profile"));
        let profile_url = format!("{api_base}/{}/{}/profile/json", instance.version, selected);
        let profile: Value = client.get(&profile_url).send().await.map_err(|e| e.to_string())?
            .error_for_status().map_err(|e| e.to_string())?.json().await.map_err(|e| e.to_string())?;
        emit_progress(&app, &instance_id, 2, 4, format!("Preparing {loader_key} files"));
        (selected, profile)
    } else {
        let requested = loader_version.unwrap_or_default();
        let selected = if !requested.trim().is_empty() {
            requested.trim().to_string()
        } else if loader_key == "forge" {
            let prefix = format!("{}-", instance.version);
            let full = latest_maven_version(
                &client,
                "https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml",
                &prefix,
            ).await?;
            full.strip_prefix(&prefix).unwrap_or(&full).to_string()
        } else {
            let parts: Vec<&str> = instance.version.split('.').collect();
            if parts.len() < 2 || parts[0] != "1" {
                return Err(format!("Cannot determine a compatible NeoForge line for Minecraft {}.", instance.version));
            }
            let prefix = format!("{}.{}.", parts[1], parts.get(2).copied().unwrap_or("0"));
            latest_maven_version(
                &client,
                "https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml",
                &prefix,
            ).await?
        };
        let installer_url = if loader_key == "forge" {
            format!("https://maven.minecraftforge.net/net/minecraftforge/forge/{}-{}/forge-{}-{}-installer.jar", instance.version, selected, instance.version, selected)
        } else {
            format!("https://maven.neoforged.net/releases/net/neoforged/neoforge/{0}/neoforge-{0}-installer.jar", selected)
        };
        emit_progress(&app, &instance_id, 1, 4, format!("Downloading {loader_key} installer"));
        let installer_path = game_dir.join("launcher-downloads").join(format!("{loader_key}-{selected}-installer.jar"));
        download_file(&client, &installer_url, &installer_path, None).await?;
        let settings = launcher_settings(&app)?;
        let java = json_string(&settings, "javaPath", "java_path").filter(|v| !v.trim().is_empty()).unwrap_or("java");
        emit_progress(&app, &instance_id, 2, 4, format!("Running {loader_key} installer"));
        let output = Command::new(java).arg("-jar").arg(&installer_path).arg("--installClient").arg(&game_dir)
            .output().map_err(|e| format!("Could not run the {loader_key} installer: {e}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("{loader_key} installer failed: {}", stderr.chars().take(1200).collect::<String>()));
        }
        let versions_dir = game_dir.join("versions");
        let mut candidates = Vec::new();
        for entry in fs::read_dir(&versions_dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|v| v.to_str()) {
                    if name.to_lowercase().contains(&loader_key) && name.contains(&selected) {
                        let json_path = path.join(format!("{name}.json"));
                        if json_path.exists() { candidates.push((name.to_string(), read_json_file(&json_path)?)); }
                    }
                }
            }
        }
        candidates.sort_by(|a, b| a.0.cmp(&b.0));
        candidates.pop().ok_or_else(|| format!("The {loader_key} installer completed but no launch profile was found. Check its installer log."))?
    };

    emit_progress(&app, &instance_id, 3, 4, format!("Configuring {loader_key} launch profile"));
    let launch_id = profile["id"].as_str().filter(|v| !v.is_empty())
        .map(str::to_string).unwrap_or_else(|| format!("{}-loader-{}-{}", loader_key, resolved_version, instance.version));
    let merged = merge_loader_profile(&base_json, &profile, &launch_id);
    let version_dir = game_dir.join("versions").join(&launch_id);
    fs::create_dir_all(&version_dir).map_err(|e| e.to_string())?;
    fs::write(version_dir.join(format!("{launch_id}.json")), serde_json::to_vec_pretty(&merged).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let base_jar = game_dir.join("versions").join(&instance.version).join(format!("{}.jar", instance.version));
    let loader_jar = version_dir.join(format!("{launch_id}.jar"));
    if !loader_jar.exists() { fs::copy(&base_jar, &loader_jar).map_err(|e| format!("Could not link the base Minecraft client JAR: {e}"))?; }

    instance.loader = match loader_key.as_str() { "fabric" => "Fabric", "quilt" => "Quilt", "forge" => "Forge", _ => "NeoForge" }.into();
    instance.loader_version = resolved_version.clone();
    instance.launch_version_id = Some(launch_id.clone());
    instance.status = "Installed".into();
    data.instances[index] = instance;
    save_data(&app, &data)?;
    emit_progress(&app, &instance_id, 4, 4, format!("{} installed", loader_key));
    Ok(launch_id)
}

// --------------------------------------------------
// Launch Minecraft
// --------------------------------------------------

#[tauri::command]
async fn launch_minecraft(
    app: AppHandle,
    id: String,
) -> Result<LaunchResult, String> {
    let data = load_data(&app)?;

    let instance = data
        .instances
        .iter()
        .find(|instance| instance.id == id)
        .ok_or_else(|| "Instance not found".to_string())?
        .clone();

    if !instance.loader.eq_ignore_ascii_case("vanilla") && instance.launch_version_id.is_none() {
        return Err(format!("{} is selected but its launch profile is not installed. Reinstall the loader for this instance.", instance.loader));
    }

    let game_dir = PathBuf::from(&instance.game_dir);
    let version = instance.launch_version_id.as_deref().unwrap_or(&instance.version);

    let version_dir = game_dir.join("versions").join(version);
    let metadata_path = version_dir.join(format!("{version}.json"));
    let client_jar = version_dir.join(format!("{version}.jar"));

    if !metadata_path.exists() || !client_jar.exists() {
        return Err("Minecraft is not installed for this instance. Install or update it first.".into());
    }

    let version_json = read_json_file(&metadata_path)?;

    let (username, uuid) = selected_offline_account(&app)?;
    let settings = launcher_settings(&app)?;

    let preferred_java = json_string(&settings, "javaPath", "java_path")
        .filter(|value| !value.trim().is_empty())
        .or(instance.java_path.as_deref());

    let memory = json_u32(&settings, "memoryMb", "memory_mb")
        .unwrap_or(instance.memory_mb)
        .max(512);

    let width = json_u32(&settings, "resolutionWidth", "resolution_width")
        .unwrap_or(854);

    let height = json_u32(&settings, "resolutionHeight", "resolution_height")
        .unwrap_or(480);

    let fullscreen = settings["fullscreen"].as_bool().unwrap_or(false);

    let client = reqwest::Client::builder()
        .user_agent("M0Z4-Launcher/0.1.0")
        .build()
        .map_err(|e| e.to_string())?;

    let java = resolve_java_runtime(
        &app,
        &client,
        &version_json,
        &instance.version,
        preferred_java,
        &id,
    ).await?;

    emit_progress(&app, &id, 0, 1, "Preparing Minecraft libraries");

    let (library_classpath, natives_dir) =
        prepare_libraries_and_natives(
            &client,
            &version_json,
            &game_dir,
            version,
        )
        .await?;

    let classpath = if library_classpath.is_empty() {
        client_jar.to_string_lossy().to_string()
    } else {
        format!(
            "{}{}{}",
            library_classpath,
            if cfg!(windows) { ";" } else { ":" },
            client_jar.to_string_lossy()
        )
    };

    let asset_index = version_json["assetIndex"]["id"]
        .as_str()
        .unwrap_or(version);

    let version_type = version_json["type"]
        .as_str()
        .unwrap_or("release");

    let mut replacements = HashMap::new();

    replacements.insert("${natives_directory}".into(), natives_dir.to_string_lossy().to_string());
    replacements.insert("${launcher_name}".into(), "M0Z4".into());
    replacements.insert("${launcher_version}".into(), "0.1.0".into());
    replacements.insert("${classpath}".into(), classpath.clone());
    replacements.insert("${classpath_separator}".into(), if cfg!(windows) { ";" } else { ":" }.into());
    replacements.insert("${library_directory}".into(), game_dir.join("libraries").to_string_lossy().to_string());
    replacements.insert("${version_name}".into(), version.to_string());
    replacements.insert("${game_directory}".into(), game_dir.to_string_lossy().to_string());
    replacements.insert("${assets_root}".into(), game_dir.join("assets").to_string_lossy().to_string());
    replacements.insert("${assets_index_name}".into(), asset_index.into());
    replacements.insert("${auth_player_name}".into(), username.clone());
    replacements.insert("${auth_uuid}".into(), uuid.clone());
    replacements.insert("${auth_access_token}".into(), "0".into());
    replacements.insert("${user_type}".into(), "legacy".into());
    replacements.insert("${version_type}".into(), version_type.into());
    replacements.insert("${resolution_width}".into(), width.to_string());
    replacements.insert("${resolution_height}".into(), height.to_string());

    let mut features = HashMap::new();
    features.insert("has_custom_resolution".into(), true);
    features.insert("is_demo_user".into(), false);
    features.insert("has_quick_plays_support".into(), false);
    features.insert("is_quick_play_singleplayer".into(), false);
    features.insert("is_quick_play_multiplayer".into(), false);
    features.insert("is_quick_play_realms".into(), false);

    let mut jvm_arguments = Vec::new();
    let mut game_arguments = Vec::new();

    if let Some(arguments) = version_json["arguments"]["jvm"].as_array() {
        for argument in arguments {
            jvm_arguments.extend(argument_allowed(argument, &features));
        }
    }

    if let Some(arguments) = version_json["arguments"]["game"].as_array() {
        for argument in arguments {
            game_arguments.extend(argument_allowed(argument, &features));
        }
    } else if let Some(legacy) = version_json["minecraftArguments"].as_str() {
        game_arguments = tokenize_legacy_arguments(legacy);
    }

    jvm_arguments = substitute_arguments(jvm_arguments, &replacements);
    game_arguments = substitute_arguments(game_arguments, &replacements);

    if !jvm_arguments.iter().any(|arg| arg.starts_with("-Xmx")) {
        jvm_arguments.insert(0, format!("-Xmx{memory}M"));
    }

    if !jvm_arguments.iter().any(|arg| arg.starts_with("-Xms")) {
        jvm_arguments.insert(1.min(jvm_arguments.len()), "-Xms512M".into());
    }

    if !jvm_arguments
        .iter()
        .any(|arg| arg.starts_with("-Djava.library.path="))
    {
        jvm_arguments.push(format!(
            "-Djava.library.path={}",
            natives_dir.to_string_lossy()
        ));
    }

    if !jvm_arguments.iter().any(|arg| arg == "-cp" || arg == "-classpath") {
        jvm_arguments.push("-cp".into());
        jvm_arguments.push(classpath);
    }

    if !game_arguments.iter().any(|arg| arg == "--width") {
        game_arguments.push("--width".into());
        game_arguments.push(width.to_string());
    }

    if !game_arguments.iter().any(|arg| arg == "--height") {
        game_arguments.push("--height".into());
        game_arguments.push(height.to_string());
    }

    if fullscreen && !game_arguments.iter().any(|arg| arg == "--fullscreen") {
        game_arguments.push("--fullscreen".into());
    }

    let main_class = version_json["mainClass"]
        .as_str()
        .ok_or("Minecraft main class is missing")?;

    let mut command = Command::new(java);

    command
        .args(&jvm_arguments)
        .arg(main_class)
        .args(&game_arguments)
        .current_dir(&game_dir);

    emit_progress(&app, &id, 1, 1, "Starting Minecraft");

    command.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .map_err(|e| format!("Could not start Java. Check your Java path: {e}"))?;

    let pid = child.id();

    if let Some(stdout) = child.stdout.take() {
        pipe_console_output(app.clone(), id.clone(), "stdout", stdout);
    }

    if let Some(stderr) = child.stderr.take() {
        pipe_console_output(app.clone(), id.clone(), "stderr", stderr);
    }

    let exit_app = app.clone();
    let exit_instance_id = id.clone();
    thread::spawn(move || {
        let line = match child.wait() {
            Ok(status) => format!("Minecraft process exited: {status}"),
            Err(error) => format!("Could not monitor Minecraft process: {error}"),
        };
        let _ = exit_app.emit(
            "console-output",
            ConsoleOutput {
                instance_id: exit_instance_id,
                stream: "system".to_string(),
                line,
            },
        );
    });

    let _ = app.emit(
        "console-output",
        ConsoleOutput {
            instance_id: id.clone(),
            stream: "system".to_string(),
            line: format!("Started Minecraft process (PID {pid})."),
        },
    );

    Ok(LaunchResult {
        pid,
        instance_id: id,
        username,
    })
}

// --------------------------------------------------
// Modrinth search
// --------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModrinthHit {
    project_id: String,
    slug: String,
    title: String,
    author: String,
    downloads: u64,
    icon_url: Option<String>,
    latest_version: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ModrinthSearchResult {
    hits: Vec<ModrinthHit>,
    total_hits: u64,
}

#[tauri::command]
async fn search_modrinth(
    query: String,
    game_version: Option<String>,
    loader: Option<String>,
    category: Option<String>,
    offset: Option<u32>,
    limit: Option<u32>,
    project_type: Option<String>,
) -> Result<ModrinthSearchResult, String> {
    let client = reqwest::Client::builder()
        .user_agent("M0Z4-Launcher/0.1.0")
        .build()
        .map_err(|e| e.to_string())?;

    let project_type = project_type.unwrap_or_else(|| "mod".into());

    let allowed_types = ["mod", "modpack"];

    if !allowed_types.contains(&project_type.as_str()) {
        return Err("Unsupported Modrinth project type".into());
    }

    let mut facets: Vec<Vec<String>> =
        vec![vec![format!("project_type:{project_type}")]];

    if let Some(version) = game_version {
        if !version.is_empty() {
            facets.push(vec![format!("versions:{version}")]);
        }
    }

    if project_type == "mod" {
        if let Some(loader) = loader {
            let loader = loader.to_lowercase();

            if ["fabric", "forge", "neoforge", "quilt"].contains(&loader.as_str()) {
                facets.push(vec![format!("categories:{loader}")]);
            }
        }
    }

    let allowed_categories = [
        "adventure",
        "decoration",
        "economy",
        "equipment",
        "food",
        "game-mechanics",
        "library",
        "magic",
        "management",
        "minigame",
        "mobs",
        "optimization",
        "social",
        "technology",
        "utility",
        "worldgen",
    ];

    if let Some(category) = category {
        if !category.is_empty()
            && allowed_categories.contains(&category.as_str())
        {
            facets.push(vec![format!("categories:{category}")]);
        }
    }

    let facets_json =
        serde_json::to_string(&facets).map_err(|e| e.to_string())?;

    let offset_string = offset.unwrap_or(0).to_string();
    let limit_string = limit.unwrap_or(30).clamp(1, 100).to_string();

    let result: Value = client
        .get(format!("{MODRINTH_API}/search"))
        .query(&[
            ("query", query.as_str()),
            ("facets", facets_json.as_str()),
            ("limit", limit_string.as_str()),
            ("offset", offset_string.as_str()),
            ("index", "downloads"),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let hits = result["hits"]
        .as_array()
        .ok_or("Invalid Modrinth search response")?
        .iter()
        .filter_map(|hit| {
            Some(ModrinthHit {
                project_id: hit["project_id"].as_str()?.to_string(),
                slug: hit["slug"].as_str()?.to_string(),
                title: hit["title"].as_str()?.to_string(),
                author: hit["author"].as_str()?.to_string(),
                downloads: hit["downloads"].as_u64().unwrap_or(0),
                icon_url: hit["icon_url"].as_str().map(str::to_string),
                latest_version: hit["latest_version"]
                    .as_str()
                    .map(str::to_string),
            })
        })
        .collect();

    Ok(ModrinthSearchResult {
        hits,
        total_hits: result["total_hits"].as_u64().unwrap_or(0),
    })
}


// --------------------------------------------------
// Modrinth mod installation
// --------------------------------------------------

#[tauri::command]
async fn install_modrinth_mod(
    app: AppHandle,
    instance_id: String,
    project_id: String,
) -> Result<String, String> {
    let data = load_data(&app)?;
    let instance = data.instances.iter().find(|i| i.id == instance_id)
        .ok_or_else(|| "Instance not found".to_string())?.clone();
    if !["fabric", "forge", "neoforge", "quilt"].contains(&instance.loader.to_lowercase().as_str()) {
        return Err("Install a supported mod loader before adding mods.".into());
    }
    let client = reqwest::Client::builder().user_agent("M0Z4-Launcher/0.1.0")
        .build().map_err(|e| e.to_string())?;
    let loaders = serde_json::to_string(&vec![instance.loader.to_lowercase()]).map_err(|e| e.to_string())?;
    let game_versions = serde_json::to_string(&vec![instance.version.clone()]).map_err(|e| e.to_string())?;
    let versions: Value = client.get(format!("{MODRINTH_API}/project/{project_id}/version"))
        .query(&[("loaders", loaders.as_str()), ("game_versions", game_versions.as_str())])
        .send().await.map_err(|e| e.to_string())?.error_for_status().map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())?;
    let version_id = versions.as_array().and_then(|items| items.first())
        .and_then(|item| item["id"].as_str()).ok_or("No compatible release found for this instance")?;
    let installed = install_modrinth_version_tree(&app, &client, &instance, version_id).await?;
    Ok(installed.join(", "))
}

// --------------------------------------------------

#[tauri::command]
async fn search_curseforge_modpacks(
    app: AppHandle, query: String, game_version: Option<String>,
    offset: Option<u32>, limit: Option<u32>,
) -> Result<ModrinthSearchResult, String> {
    let key = settings_accounts::read_curseforge_api_key(&app)?;
    let client = reqwest::Client::builder().user_agent("M0Z4-Launcher/0.1.6").build().map_err(|e| e.to_string())?;
    let offset_s = offset.unwrap_or(0).to_string();
    let limit_s = limit.unwrap_or(20).clamp(1, 50).to_string();
    let mut request = client.get("https://api.curseforge.com/v1/mods/search")
        .header("x-api-key", key)
        .query(&[("gameId", "432"), ("classId", "4471"), ("searchFilter", query.as_str()),
            ("index", offset_s.as_str()), ("pageSize", limit_s.as_str())]);
    if let Some(version) = game_version.filter(|v| !v.is_empty()) {
        request = request.query(&[("gameVersion", version)]);
    }
    let value: Value = request.send().await.map_err(|e| e.to_string())?
        .error_for_status().map_err(|e| format!("CurseForge request failed: {e}"))?
        .json().await.map_err(|e| e.to_string())?;
    let hits = value["data"].as_array().cloned().unwrap_or_default().into_iter().map(|item| {
        ModrinthHit {
            project_id: item["id"].as_u64().unwrap_or(0).to_string(),
            slug: item["slug"].as_str().unwrap_or("").to_string(),
            title: item["name"].as_str().unwrap_or("Untitled modpack").to_string(),
            author: item["authors"].as_array().and_then(|a| a.first()).and_then(|a| a["name"].as_str()).unwrap_or("CurseForge creator").to_string(),
            downloads: item["downloadCount"].as_u64().unwrap_or(0),
            icon_url: item["logo"]["url"].as_str().map(str::to_string),
            latest_version: None,
        }
    }).collect();
    Ok(ModrinthSearchResult { hits, total_hits: value["pagination"]["totalCount"].as_u64().unwrap_or(0) })
}

#[tauri::command]
async fn get_curseforge_modpack_files(app: AppHandle, mod_id: String) -> Result<Vec<Value>, String> {
    let key = settings_accounts::read_curseforge_api_key(&app)?;
    let client = reqwest::Client::builder().user_agent("M0Z4-Launcher/0.1.6").build().map_err(|e| e.to_string())?;
    let value: Value = client.get(format!("https://api.curseforge.com/v1/mods/{mod_id}/files"))
        .header("x-api-key", key).query(&[("pageSize", "50"), ("index", "0")])
        .send().await.map_err(|e| e.to_string())?.error_for_status().map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())?;
    Ok(value["data"].as_array().cloned().unwrap_or_default())
}


#[tauri::command]
async fn install_curseforge_modpack(app: AppHandle, instance_id: String, project_id: String, file_id: String) -> Result<Value, String> {
    let key = settings_accounts::read_curseforge_api_key(&app)?;
    let data = load_data(&app)?;
    let instance = data.instances.iter().find(|i| i.id == instance_id).cloned().ok_or("Instance not found")?;
    let client = reqwest::Client::builder().user_agent("M0Z4-Launcher/0.1.6").build().map_err(|e| e.to_string())?;
    let meta: Value = client.get(format!("https://api.curseforge.com/v1/mods/{project_id}/files/{file_id}"))
        .header("x-api-key", &key).send().await.map_err(|e| e.to_string())?
        .error_for_status().map_err(|e| e.to_string())?.json().await.map_err(|e| e.to_string())?;
    let url = meta["data"]["downloadUrl"].as_str().ok_or("CurseForge did not provide a direct modpack download URL.")?;
    if !url.starts_with("https://") { return Err("Insecure modpack URL rejected.".into()); }
    emit_progress(&app, &instance_id, 0, 1, "Downloading CurseForge modpack");
    let bytes = client.get(url).send().await.map_err(|e| e.to_string())?.error_for_status().map_err(|e| e.to_string())?.bytes().await.map_err(|e| e.to_string())?;
    if bytes.len() > 512 * 1024 * 1024 { return Err("Modpack archive exceeds the 512 MB safety limit.".into()); }
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes.to_vec())).map_err(|e| format!("Invalid CurseForge archive: {e}"))?;
    let manifest: Value = {
        let mut entry = archive.by_name("manifest.json").map_err(|_| "Archive has no manifest.json.")?;
        let mut raw = Vec::new();
        entry.take(4 * 1024 * 1024).read_to_end(&mut raw).map_err(|e| e.to_string())?;
        serde_json::from_slice(&raw).map_err(|e| e.to_string())?
    };
    let pack_mc_version = manifest["minecraft"]["version"].as_str().ok_or("CurseForge manifest is missing its Minecraft version.")?;
    if pack_mc_version != instance.version { return Err(format!("This modpack requires Minecraft {pack_mc_version}; the selected instance uses {}. Create a matching instance first.", instance.version)); }
    let files = manifest["files"].as_array().ok_or("Invalid CurseForge manifest.")?;
    let required_loader = manifest["minecraft"]["modLoaders"].as_array().and_then(|a| a.iter().find(|v| v["primary"].as_bool() == Some(true)).or_else(|| a.first())).and_then(|v| v["id"].as_str()).unwrap_or("");
    if !required_loader.is_empty() {
        let loader = required_loader.split('-').next().unwrap_or("");
        if !instance.loader.eq_ignore_ascii_case(loader) { return Err(format!("This pack requires {loader}. Create an instance with that loader first.")); }
    }
    let game_dir = PathBuf::from(&instance.game_dir);
    let mut installed = 0usize;
    for item in files {
        if item["required"].as_bool() == Some(false) { continue; }
        let project = item["projectID"].as_u64().ok_or("Invalid project ID in manifest")?;
        let file = item["fileID"].as_u64().ok_or("Invalid file ID in manifest")?;
        let value: Value = client.get(format!("https://api.curseforge.com/v1/mods/{project}/files/{file}"))
            .header("x-api-key", &key).send().await.map_err(|e| e.to_string())?
            .error_for_status().map_err(|e| e.to_string())?.json().await.map_err(|e| e.to_string())?;
        let download = value["data"]["downloadUrl"].as_str().ok_or("A required mod file is unavailable for direct download.")?;
        if !download.starts_with("https://") { return Err("Insecure mod download rejected.".into()); }
        let filename = value["data"]["fileName"].as_str().unwrap_or("mod.jar");
        if filename.contains('/') || filename.contains('\\') || filename.contains("..") { return Err("Unsafe mod filename rejected.".into()); }
        let content = client.get(download).send().await.map_err(|e| e.to_string())?.error_for_status().map_err(|e| e.to_string())?.bytes().await.map_err(|e| e.to_string())?;
        if content.len() > 256 * 1024 * 1024 { return Err("Mod file exceeds the 256 MB safety limit.".into()); }
        let mods = game_dir.join("mods");
        tokio::fs::create_dir_all(&mods).await.map_err(|e| e.to_string())?;
        tokio::fs::write(mods.join(filename), &content).await.map_err(|e| e.to_string())?;
        installed += 1;
        emit_progress(&app, &instance_id, installed, files.len(), "Downloading CurseForge mods");
    }
    let mut overrides = 0u32;
    for n in 0..archive.len() {
        let mut entry = archive.by_index(n).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();
        let Some(relative) = name.strip_prefix("overrides/") else { continue; };
        if relative.is_empty() || relative.contains('\\') || relative.split('/').any(|p| p == ".." || p.is_empty()) { continue; }
        let destination = game_dir.join(relative);
        if entry.is_dir() { tokio::fs::create_dir_all(destination).await.map_err(|e| e.to_string())?; }
        else {
            if entry.size() > 128 * 1024 * 1024 { return Err("Override file exceeds the 128 MB safety limit.".into()); }
            if let Some(parent) = destination.parent() { tokio::fs::create_dir_all(parent).await.map_err(|e| e.to_string())?; }
            let mut raw = Vec::new();
            entry.take(128 * 1024 * 1024).read_to_end(&mut raw).map_err(|e| e.to_string())?;
            tokio::fs::write(destination, raw).await.map_err(|e| e.to_string())?;
            overrides += 1;
        }
    }
    emit_progress(&app, &instance_id, 1, 1, "CurseForge modpack installed");
    Ok(serde_json::json!({"name": manifest["name"], "minecraftVersion": manifest["minecraft"]["version"], "loader": required_loader, "installedMods": installed, "overrideFiles": overrides}))
}


// Modrinth version selection
// --------------------------------------------------

#[tauri::command]
async fn get_modrinth_project_versions(
    project_id: String,
    game_version: Option<String>,
    loader: Option<String>,
) -> Result<Vec<Value>, String> {
    let client = reqwest::Client::builder()
        .user_agent("M0Z4-Launcher/0.1.0")
        .build().map_err(|e| e.to_string())?;

    let mut request = client.get(format!("{MODRINTH_API}/project/{project_id}/version"));
    if let Some(version) = game_version.filter(|v| !v.is_empty()) {
        request = request.query(&[("game_versions", format!("[\"{version}\"]"))]);
    }
    if let Some(loader) = loader.filter(|v| !v.is_empty() && v != "Vanilla") {
        request = request.query(&[("loaders", format!("[\"{}\"]", loader.to_lowercase()))]);
    }

    request.send().await.map_err(|e| e.to_string())?
        .error_for_status().map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())
}

async fn fetch_modrinth_version(client: &reqwest::Client, version_id: &str) -> Result<Value, String> {
    client.get(format!("{MODRINTH_API}/version/{version_id}"))
        .send().await.map_err(|e| e.to_string())?
        .error_for_status().map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())
}

async fn install_modrinth_version_tree(
    app: &AppHandle,
    client: &reqwest::Client,
    instance: &Instance,
    first_version_id: &str,
) -> Result<Vec<String>, String> {
    let mods_dir = PathBuf::from(&instance.game_dir).join("mods");
    tokio::fs::create_dir_all(&mods_dir).await.map_err(|e| e.to_string())?;

    let mut pending = vec![first_version_id.to_string()];
    let mut visited: HashSet<String> = HashSet::new();
    let mut installed = Vec::new();

    while let Some(version_id) = pending.pop() {
        if !visited.insert(version_id.clone()) { continue; }
        if visited.len() > 64 {
            return Err("Dependency graph is unexpectedly large; stopping to protect the installation.".into());
        }

        let version = fetch_modrinth_version(client, &version_id).await?;
        let compatible_game = version["game_versions"].as_array().is_some_and(|items| items.iter().any(|v| v.as_str() == Some(instance.version.as_str())));
        let loader_name = instance.loader.to_lowercase();
        let compatible_loader = version["loaders"].as_array().is_some_and(|items| items.iter().any(|v| v.as_str() == Some(loader_name.as_str())));
        if !compatible_game || !compatible_loader {
            return Err(format!("Selected mod version is not compatible with Minecraft {} / {}.", instance.version, instance.loader));
        }
        let dependencies = version["dependencies"].as_array().cloned().unwrap_or_default();
        for dependency in dependencies {
            if dependency["dependency_type"].as_str() != Some("required") { continue; }
            if let Some(required_version) = dependency["version_id"].as_str() {
                if !visited.contains(required_version) { pending.push(required_version.to_string()); }
            } else if let Some(project_id) = dependency["project_id"].as_str() {
                let loaders = serde_json::to_string(&vec![instance.loader.to_lowercase()]).map_err(|e| e.to_string())?;
                let game_versions = serde_json::to_string(&vec![instance.version.clone()]).map_err(|e| e.to_string())?;
                let compatible: Value = client
                    .get(format!("{MODRINTH_API}/project/{project_id}/version"))
                    .query(&[("loaders", loaders.as_str()), ("game_versions", game_versions.as_str())])
                    .send().await.map_err(|e| e.to_string())?
                    .error_for_status().map_err(|e| e.to_string())?
                    .json().await.map_err(|e| e.to_string())?;
                if let Some(id) = compatible.as_array().and_then(|items| items.first()).and_then(|item| item["id"].as_str()) {
                    if !visited.contains(id) { pending.push(id.to_string()); }
                } else {
                    return Err(format!("A required dependency ({project_id}) has no compatible version for Minecraft {} / {}.", instance.version, instance.loader));
                }
            }
        }

        let files = version["files"].as_array().ok_or("A mod version has no files")?;
        let file = files.iter().find(|f| f["primary"].as_bool() == Some(true)).or_else(|| files.first()).ok_or("A mod version has no downloadable files")?;
        let url = file["url"].as_str().ok_or("Mod download URL is missing")?;
        let filename = file["filename"].as_str().ok_or("Mod filename is missing")?;
        let safe_name = Path::new(filename).file_name().ok_or("Invalid mod filename")?;
        let destination = mods_dir.join(safe_name);
        download_file(client, url, &destination, file["hashes"]["sha1"].as_str()).await?;
        installed.push(safe_name.to_string_lossy().to_string());
        emit_progress(app, &instance.id, visited.len(), visited.len() + pending.len(), format!("Installed {}", filename));
    }

    installed.sort();
    installed.dedup();
    Ok(installed)
}

#[tauri::command]
async fn install_modrinth_mod_version(
    app: AppHandle,
    instance_id: String,
    version_id: String,
) -> Result<Vec<String>, String> {
    let data = load_data(&app)?;
    let instance = data.instances.iter().find(|i| i.id == instance_id)
        .ok_or_else(|| "Instance not found".to_string())?.clone();
    if !["fabric", "forge", "neoforge", "quilt"].contains(&instance.loader.to_lowercase().as_str()) {
        return Err("Install a supported mod loader before adding mods.".into());
    }
    let client = reqwest::Client::builder().user_agent("M0Z4-Launcher/0.1.0")
        .build().map_err(|e| e.to_string())?;
    install_modrinth_version_tree(&app, &client, &instance, &version_id).await
}


// --------------------------------------------------
// Modrinth .mrpack installation
// --------------------------------------------------

#[tauri::command]
async fn install_modrinth_modpack(
    app: AppHandle,
    instance_id: String,
    project_id: String,
    version_id: String,
) -> Result<Value, String> {
    let mut data = load_data(&app)?;
    let index = data.instances.iter().position(|i| i.id == instance_id)
        .ok_or_else(|| "Instance not found".to_string())?;
    let mut instance = data.instances[index].clone();
    let client = reqwest::Client::builder().user_agent("M0Z4-Launcher/0.1.0")
        .build().map_err(|e| e.to_string())?;

    let version = fetch_modrinth_version(&client, &version_id).await?;
    let file = version["files"].as_array().and_then(|files| files.iter().find(|f| f["primary"].as_bool() == Some(true)).or_else(|| files.first()))
        .ok_or("Selected modpack version has no file")?;
    let url = file["url"].as_str().ok_or("Modpack download URL is missing")?;
    let filename = file["filename"].as_str().unwrap_or("pack.mrpack");
    if !filename.to_ascii_lowercase().ends_with(".mrpack") {
        return Err("This Modrinth project version is not an .mrpack file.".into());
    }
    if !url.starts_with("https://") { return Err("Modpack downloads must use HTTPS".into()); }

    emit_progress(&app, &instance_id, 0, 1, "Downloading modpack archive");
    let response = client.get(url).send().await.map_err(|e| e.to_string())?
        .error_for_status().map_err(|e| e.to_string())?;
    let archive_bytes = response.bytes().await.map_err(|e| e.to_string())?;
    if archive_bytes.len() > 512 * 1024 * 1024 { return Err("Modpack archive is larger than the 512 MB safety limit.".into()); }

    let mut archive = zip::ZipArchive::new(Cursor::new(archive_bytes.to_vec()))
        .map_err(|e| format!("Invalid .mrpack archive: {e}"))?;
    let manifest_bytes = {
        let mut manifest_file = archive.by_name("modrinth.index.json")
            .map_err(|_| "This archive is missing modrinth.index.json".to_string())?;
        let mut bytes = Vec::new();
        manifest_file.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        bytes
    };
    let manifest: Value = serde_json::from_slice(&manifest_bytes).map_err(|e| format!("Invalid modpack manifest: {e}"))?;
    let mc_version = manifest["dependencies"]["minecraft"]["version"].as_str()
        .ok_or("Modpack manifest does not specify a Minecraft version")?.to_string();
    let loaders = manifest["dependencies"].as_object().ok_or("Modpack loader requirements are missing")?;
    let (loader, loader_version) = ["fabric", "forge", "neoforge", "quilt"].iter()
        .find_map(|key| loaders.get(*key).and_then(Value::as_str).map(|v| ((*key).to_string(), v.to_string())))
        .unwrap_or_else(|| ("Vanilla".to_string(), "—".to_string()));

    // Install the vanilla base before placing modpack files. The selected instance's worlds remain untouched.
    install_minecraft_version(app.clone(), instance_id.clone(), mc_version.clone()).await?;
    let game_dir = PathBuf::from(&instance.game_dir);
    let file_entries = manifest["files"].as_array().ok_or("Modpack manifest file list is missing")?;
    let total = file_entries.len().max(1);
    let mut completed = 0usize;
    let mut modpack_mods: Vec<String> = Vec::new();
    for entry in file_entries {
        if entry["env"]["client"].as_str() == Some("unsupported") { continue; }
        let relative = entry["path"].as_str().ok_or("Modpack contains a file without a path")?;
        let relative_path = safe_relative_path(relative)?;
        if relative.replace('\\', "/").to_ascii_lowercase().starts_with("mods/")
            && relative_path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("jar"))
        {
            modpack_mods.push(relative.replace('\\', "/"));
        }
        let downloads = entry["downloads"].as_array().ok_or("Modpack file has no download URLs")?;
        let download_url = downloads.iter().filter_map(Value::as_str).find(|u| u.starts_with("https://"))
            .ok_or_else(|| format!("No HTTPS download URL is available for {relative}"))?;
        let sha1 = entry["hashes"]["sha1"].as_str();
        let destination = game_dir.join(relative_path);
        download_file(&client, download_url, &destination, sha1).await?;
        completed += 1;
        if completed % 5 == 0 || completed == total {
            emit_progress(&app, &instance_id, completed, total, "Downloading modpack files");
        }
    }

    // Only extract the two client-side override roots defined by the .mrpack format.
    let archive_data = archive.into_inner().into_inner();
    let mut overrides = zip::ZipArchive::new(Cursor::new(archive_data)).map_err(|e| e.to_string())?;
    let mut extracted = 0usize;
    for index in 0..overrides.len() {
        let mut entry = overrides.by_index(index).map_err(|e| e.to_string())?;
        let name = entry.name().replace('\\', "/");
        let relative = if let Some(path) = name.strip_prefix("overrides/") {
            Some(path)
        } else if let Some(path) = name.strip_prefix("client-overrides/") {
            Some(path)
        } else { None };
        let Some(relative) = relative else { continue; };
        if relative.is_empty() || entry.is_dir() { continue; }
        if entry.size() > 128 * 1024 * 1024 { return Err(format!("Override file is too large: {relative}")); }
        let safe_path = safe_relative_path(relative)?;
        if relative.replace('\\', "/").to_ascii_lowercase().starts_with("mods/")
            && safe_path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("jar"))
        {
            modpack_mods.push(relative.replace('\\', "/"));
        }
        let destination = game_dir.join(safe_path);
        let parent = destination.parent().ok_or("Invalid override destination")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        fs::write(destination, bytes).map_err(|e| e.to_string())?;
        extracted += 1;
    }

    instance.version = mc_version.clone();
    instance.loader = match loader.as_str() { "fabric" => "Fabric", "forge" => "Forge", "neoforge" => "NeoForge", "quilt" => "Quilt", _ => "Vanilla" }.into();
    instance.loader_version = loader_version.clone();
    instance.launch_version_id = None;
    instance.status = if loader == "Vanilla" { "Installed".into() } else { "Modpack installed — loader setup required".into() };
    instance.mods = count_mods(&instance);
    data.instances[index] = instance.clone();
    save_data(&app, &data)?;
    if loader != "Vanilla" {
        install_modloader(app.clone(), instance_id.clone(), loader.to_lowercase(), Some(loader_version.clone())).await?;
    }

    modpack_mods.sort();
    modpack_mods.dedup();
    let metadata = serde_json::json!({
        "projectId": project_id,
        "versionId": version_id,
        "name": manifest["name"].as_str().unwrap_or("Modrinth Modpack"),
        "version": manifest["versionId"].as_str().unwrap_or("unknown"),
        "minecraftVersion": mc_version,
        "loader": loader,
        "loaderVersion": loader_version,
        "installedOverrides": extracted,
        "mods": modpack_mods,
    });
    fs::write(game_dir.join(".m0z4-modpack.json"), serde_json::to_vec_pretty(&metadata).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    emit_progress(&app, &instance_id, total, total, "Modpack files installed");
    Ok(metadata)
}


#[tauri::command]
fn get_installed_modpack(app: AppHandle, instance_id: String) -> Result<Option<Value>, String> {
    let data = load_data(&app)?;
    let instance = data.instances.iter().find(|i| i.id == instance_id)
        .ok_or_else(|| "Instance not found".to_string())?;
    let path = PathBuf::from(&instance.game_dir).join(".m0z4-modpack.json");
    if !path.exists() { return Ok(None); }
    Ok(Some(read_json_file(&path)?))
}

// --------------------------------------------------
// Installed mods
// --------------------------------------------------

#[tauri::command]
fn remove_mod(
    app: AppHandle,
    instance_id: String,
    filename: String,
) -> Result<(), String> {
    let data = load_data(&app)?;

    let instance = data
        .instances
        .iter()
        .find(|instance| instance.id == instance_id)
        .ok_or_else(|| "Instance not found".to_string())?;

    let safe_name = Path::new(&filename)
        .file_name()
        .ok_or("Invalid filename")?;

    let path = PathBuf::from(&instance.game_dir)
        .join("mods")
        .join(safe_name);

    if path.exists() {
        fs::remove_file(path).map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[tauri::command]
fn get_installed_mods(
    app: AppHandle,
    instance_id: String,
) -> Result<Vec<String>, String> {
    let data = load_data(&app)?;

    let instance = data
        .instances
        .iter()
        .find(|instance| instance.id == instance_id)
        .ok_or_else(|| "Instance not found".to_string())?;

    let directory = PathBuf::from(&instance.game_dir).join("mods");

    if !directory.exists() {
        return Ok(Vec::new());
    }

    let mut mods = Vec::new();

    for entry in fs::read_dir(directory).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();

        if path.extension().is_some_and(|ext| ext == "jar") {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                mods.push(name.to_string());
            }
        }
    }

    mods.sort();
    Ok(mods)
}


// --------------------------------------------------
// Direct instance sharing (archive preparation and safe import)
// --------------------------------------------------

const SHARE_MAX_ARCHIVE_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const SHARE_CHUNK_MAX_BYTES: usize = 64 * 1024;
const SHARE_MAX_FILES: usize = 100_000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ShareArchiveInfo {
    archive_id: String,
    size: u64,
    sha256: String,
    name: String,
    version: String,
    loader: String,
    loader_version: String,
    mod_count: usize,
    include_saves: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ShareManifest {
    format_version: u32,
    name: String,
    version: String,
    loader: String,
    loader_version: String,
    launch_version_id: Option<String>,
    memory_mb: u32,
    icon: String,
}

fn share_temp_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let path = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("instance-sharing");
    fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    Ok(path)
}

fn share_archive_path(app: &AppHandle, archive_id: &str) -> Result<PathBuf, String> {
    Uuid::parse_str(archive_id).map_err(|_| "Invalid transfer identifier".to_string())?;
    Ok(share_temp_dir(app)?.join(format!("{archive_id}.zip")))
}

fn share_file_is_private(relative: &str, include_saves: bool) -> bool {
    let normalized = relative.replace('\\', "/");
    let parts: Vec<String> = normalized.split('/').map(|part| part.to_ascii_lowercase()).collect();
    if parts.is_empty() { return true; }
    let first = parts[0].as_str();
    let base = parts.last().map(String::as_str).unwrap_or("");
    if !include_saves && first == "saves" { return true; }
    if parts.iter().any(|part| ["logs", "crash-reports", "screenshots", "cache", "caches", ".cache", "webcache"].contains(&part.as_str())) {
        return true;
    }
    [
        "launcher_accounts.json", "launcher_profiles.json", "accounts.json",
        "credentials.json", "tokens.json", "msa_credentials.bin",
        "launcher_msa_credentials.bin", "access_token.json", "refresh_token.json",
        "session.json", "microsoft_auth.json", "msa.json",
    ].contains(&base)
}

fn add_share_directory(
    writer: &mut zip::ZipWriter<fs::File>,
    root: &Path,
    current: &Path,
    include_saves: bool,
    total_bytes: &mut u64,
    file_count: &mut usize,
) -> Result<(), String> {
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for entry in fs::read_dir(current).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let file_type = entry.file_type().map_err(|e| e.to_string())?;
        if file_type.is_symlink() { continue; }
        let path = entry.path();
        let relative = path.strip_prefix(root).map_err(|e| e.to_string())?;
        let relative_string = relative.to_string_lossy().replace('\\', "/");
        if share_file_is_private(&relative_string, include_saves) { continue; }
        if file_type.is_dir() {
            writer.add_directory(format!("{relative_string}/"), options).map_err(|e| e.to_string())?;
            add_share_directory(writer, root, &path, include_saves, total_bytes, file_count)?;
        } else if file_type.is_file() {
            let metadata = entry.metadata().map_err(|e| e.to_string())?;
            *total_bytes = total_bytes.checked_add(metadata.len()).ok_or("Instance size overflow")?;
            *file_count += 1;
            if *total_bytes > SHARE_MAX_ARCHIVE_BYTES || *file_count > SHARE_MAX_FILES {
                return Err("This instance is too large to share (maximum 8 GiB and 100,000 files).".into());
            }
            writer.start_file(relative_string, options).map_err(|e| e.to_string())?;
            let mut input = fs::File::open(path).map_err(|e| e.to_string())?;
            std::io::copy(&mut input, writer).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[tauri::command]
async fn create_instance_share_archive(
    app: AppHandle,
    instance_id: String,
    include_saves: bool,
) -> Result<ShareArchiveInfo, String> {
    tauri::async_runtime::spawn_blocking(move || {
    let data = load_data(&app)?;
    let instance = data.instances.iter().find(|item| item.id == instance_id)
        .ok_or_else(|| "Instance not found".to_string())?;
    let game_dir = PathBuf::from(&instance.game_dir);
    if !game_dir.is_dir() { return Err("Instance folder is missing.".into()); }

    let archive_id = Uuid::new_v4().to_string();
    let archive_path = share_archive_path(&app, &archive_id)?;
    let file = fs::File::create(&archive_path).map_err(|e| e.to_string())?;
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let manifest = ShareManifest {
        format_version: 1,
        name: instance.name.clone(),
        version: instance.version.clone(),
        loader: instance.loader.clone(),
        loader_version: instance.loader_version.clone(),
        launch_version_id: instance.launch_version_id.clone(),
        memory_mb: instance.memory_mb.clamp(512, 65536),
        icon: String::new(),
    };
    writer.start_file("m4-share-manifest.json", options).map_err(|e| e.to_string())?;
    writer.write_all(&serde_json::to_vec(&manifest).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let mut total_bytes = 0u64;
    let mut file_count = 0usize;
    if let Err(error) = add_share_directory(
        &mut writer, &game_dir, &game_dir, include_saves, &mut total_bytes, &mut file_count
    ) {
        let _ = fs::remove_file(&archive_path);
        return Err(error);
    }
    let file = writer.finish().map_err(|e| e.to_string())?;
    let size = file.metadata().map_err(|e| e.to_string())?.len();
    if size > SHARE_MAX_ARCHIVE_BYTES {
        let _ = fs::remove_file(&archive_path);
        return Err("The compressed instance archive exceeds the 8 GiB sharing limit.".into());
    }
    let mut reader = fs::File::open(&archive_path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 1024 * 1024];
    loop {
        let count = reader.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 { break; }
        hasher.update(&buffer[..count]);
    }
    Ok(ShareArchiveInfo {
        archive_id,
        size,
        sha256: hex::encode(hasher.finalize()),
        name: instance.name.clone(),
        version: instance.version.clone(),
        loader: instance.loader.clone(),
        loader_version: instance.loader_version.clone(),
        mod_count: count_mods(instance),
        include_saves,
    })

    })
    .await
    .map_err(|error| format!("Archive preparation task failed: {error}"))?
}

#[tauri::command]
fn read_instance_share_chunk(
    app: AppHandle,
    archive_id: String,
    offset: u64,
    length: usize,
) -> Result<Vec<u8>, String> {
    if length == 0 || length > SHARE_CHUNK_MAX_BYTES {
        return Err("Invalid transfer chunk size.".into());
    }
    let path = share_archive_path(&app, &archive_id)?;
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let size = file.metadata().map_err(|e| e.to_string())?.len();
    if offset > size || offset.saturating_add(length as u64) > size {
        return Err("Requested transfer chunk is outside the archive.".into());
    }
    file.seek(SeekFrom::Start(offset)).map_err(|e| e.to_string())?;
    let mut bytes = vec![0u8; length];
    file.read_exact(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes)
}

#[tauri::command]
fn delete_instance_share_archive(app: AppHandle, archive_id: String) -> Result<(), String> {
    let path = share_archive_path(&app, &archive_id)?;
    if path.exists() { fs::remove_file(path).map_err(|e| e.to_string())?; }
    Ok(())
}

#[tauri::command]
fn begin_instance_share_receive(
    app: AppHandle,
    archive_size: u64,
) -> Result<String, String> {
    if archive_size == 0 || archive_size > SHARE_MAX_ARCHIVE_BYTES {
        return Err("The shared instance archive is empty or exceeds the 8 GiB limit.".into());
    }
    let archive_id = Uuid::new_v4().to_string();
    let path = share_archive_path(&app, &archive_id)?;
    let file = fs::File::create(path).map_err(|e| e.to_string())?;
    file.set_len(archive_size).map_err(|e| e.to_string())?;
    Ok(archive_id)
}

#[tauri::command]
fn discard_instance_share_receive(app: AppHandle, archive_id: String) -> Result<(), String> {
    let path = share_archive_path(&app, &archive_id)?;
    if path.exists() { fs::remove_file(path).map_err(|e| e.to_string())?; }
    Ok(())
}

#[tauri::command]
fn write_instance_share_chunk(
    app: AppHandle,
    archive_id: String,
    offset: u64,
    archive_size: u64,
    bytes: Vec<u8>,
) -> Result<(), String> {
    if bytes.is_empty() || bytes.len() > SHARE_CHUNK_MAX_BYTES
        || archive_size == 0 || archive_size > SHARE_MAX_ARCHIVE_BYTES
        || offset.saturating_add(bytes.len() as u64) > archive_size {
        return Err("Invalid incoming transfer chunk.".into());
    }
    let path = share_archive_path(&app, &archive_id)?;
    let mut file = fs::OpenOptions::new().write(true).open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() != archive_size {
        return Err("Incoming archive size changed during transfer.".into());
    }
    file.seek(SeekFrom::Start(offset)).map_err(|e| e.to_string())?;
    file.write_all(&bytes).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn finish_instance_share_receive(
    app: AppHandle,
    archive_id: String,
    expected_sha256: String,
) -> Result<Instance, String> {
    let archive_path = share_archive_path(&app, &archive_id)?;
    let mut source = fs::File::open(&archive_path).map_err(|e| e.to_string())?;
    let size = source.metadata().map_err(|e| e.to_string())?.len();
    if size == 0 || size > SHARE_MAX_ARCHIVE_BYTES {
        let _ = fs::remove_file(&archive_path);
        return Err("Incoming archive size is invalid.".into());
    }
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 1024 * 1024];
    loop {
        let count = source.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 { break; }
        hasher.update(&buffer[..count]);
    }
    if hex::encode(hasher.finalize()) != expected_sha256.to_ascii_lowercase() {
        let _ = fs::remove_file(&archive_path);
        return Err("Instance transfer integrity check failed. The archive was not imported.".into());
    }

    let file = fs::File::open(&archive_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("Invalid instance archive: {e}"))?;
    let manifest = {
        let mut entry = archive.by_name("m4-share-manifest.json")
            .map_err(|_| "Shared instance is missing its manifest.".to_string())?;
        if entry.size() > 64 * 1024 { return Err("Shared instance manifest is too large.".into()); }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        serde_json::from_slice::<ShareManifest>(&bytes).map_err(|e| format!("Invalid share manifest: {e}"))?
    };
    if manifest.format_version != 1 || manifest.name.trim().is_empty() {
        return Err("This M4 instance archive format is not supported.".into());
    }
    if archive.len() > SHARE_MAX_FILES + 1 {
        return Err("The shared instance contains too many files.".into());
    }

    let id = Uuid::new_v4().to_string();
    let mut data = load_data(&app)?;
    let base_name = manifest.name.trim().chars().take(64).collect::<String>();
    let mut imported_name = base_name.clone();
    let mut suffix = 2usize;
    while data.instances.iter().any(|item| item.name.eq_ignore_ascii_case(&imported_name)) {
        imported_name = format!("{base_name} ({suffix})");
        suffix += 1;
    }
    let game_dir = app.path().app_data_dir().map_err(|e| e.to_string())?
        .join("instances").join(&id);
    fs::create_dir_all(&game_dir).map_err(|e| e.to_string())?;
    let mut total_uncompressed = 0u64;
    let mut extracted_count = 0usize;
    let extraction_result: Result<(), String> = (|| {
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).map_err(|e| e.to_string())?;
            let name = entry.name().replace('\\', "/");
            if name == "m4-share-manifest.json" || name.is_empty() || entry.is_dir() { continue; }
            if entry.unix_mode().is_some_and(|mode| mode & 0o170000 == 0o120000) {
                return Err("The shared archive contains an unsafe symbolic link.".into());
            }
            let relative = safe_relative_path(&name)?;
            if share_file_is_private(&name, true) { continue; }
            if entry.size() > 1024 * 1024 * 1024 {
                return Err(format!("Shared file is too large: {name}"));
            }
            total_uncompressed = total_uncompressed.checked_add(entry.size()).ok_or("Archive size overflow")?;
            extracted_count += 1;
            if total_uncompressed > SHARE_MAX_ARCHIVE_BYTES || extracted_count > SHARE_MAX_FILES {
                return Err("The expanded instance exceeds the import safety limits.".into());
            }
            let destination = game_dir.join(relative);
            let parent = destination.parent().ok_or("Invalid archive destination")?;
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            let mut output = fs::File::create(destination).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut output).map_err(|e| e.to_string())?;
        }
        Ok(())
    })();
    if let Err(error) = extraction_result {
        let _ = fs::remove_dir_all(&game_dir);
        let _ = fs::remove_file(&archive_path);
        return Err(error);
    }

    let installed = game_dir.join("versions").is_dir();
    let instance = Instance {
        id: id.clone(),
        name: imported_name,
        version: manifest.version,
        loader: manifest.loader,
        loader_version: manifest.loader_version,
        icon: String::new(),
        status: if installed { "Installed".into() } else { "Not installed".into() },
        game_dir: game_dir.to_string_lossy().to_string(),
        memory_mb: manifest.memory_mb.clamp(512, 65536),
        java_path: None,
        launch_version_id: manifest.launch_version_id,
        mods: 0,
    };
    if data.selected_instance.is_none() { data.selected_instance = Some(id); }
    data.instances.push(instance.clone());
    if let Err(error) = save_data(&app, &data) {
        let _ = fs::remove_dir_all(&game_dir);
        let _ = fs::remove_file(&archive_path);
        return Err(error);
    }
    let _ = fs::remove_file(&archive_path);
    Ok(instance)
}

// --------------------------------------------------
// Tauri application
// --------------------------------------------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new()
            .pubkey(option_env!("M0Z4_UPDATER_PUBLIC_KEY").unwrap_or(""))
            .build())
        .invoke_handler(tauri::generate_handler![
            get_instances,
            create_instance,
            delete_instance,
            select_instance,
            update_instance,
            detect_java,
            open_instance_folder,
            get_minecraft_versions,
            install_minecraft_version,
            install_modloader,
            launch_minecraft,
            search_modrinth,
            search_curseforge_modpacks,
            get_curseforge_modpack_files,
            install_curseforge_modpack,
            install_modrinth_mod,
            get_modrinth_project_versions,
            install_modrinth_mod_version,
            install_modrinth_modpack,
            get_installed_modpack,
            remove_mod,
            get_installed_mods,
            create_instance_share_archive,
            read_instance_share_chunk,
            delete_instance_share_archive,
            begin_instance_share_receive,
            discard_instance_share_receive,
            write_instance_share_chunk,
            finish_instance_share_receive,

            // Settings and accounts
            settings_accounts::get_launcher_settings,
            settings_accounts::save_launcher_settings,
            settings_accounts::get_curseforge_key_status,
            settings_accounts::save_curseforge_api_key,
            settings_accounts::clear_curseforge_api_key,
            settings_accounts::get_minecraft_options,
            settings_accounts::save_minecraft_options,
            settings_accounts::reset_minecraft_options,
            settings_accounts::get_accounts,
            settings_accounts::add_offline_account,
            settings_accounts::select_account,
            settings_accounts::delete_account
        ])
        .run(tauri::generate_context!())
        .expect("error while running M0Z4");
}

