use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};
use std::{
    sync::mpsc::{self, Receiver, RecvTimeoutError, Sender},
    thread,
    time::Duration,
};
use tauri::State;

const DISCORD_APPLICATION_ID: &str = "1570879017203015891";

enum PresenceMessage {
    Update { enabled: bool, details: String, state: String },
}

pub struct DiscordPresenceState {
    sender: Sender<PresenceMessage>,
}

impl DiscordPresenceState {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || run_presence_worker(receiver));
        Self { sender }
    }
}

fn run_presence_worker(receiver: Receiver<PresenceMessage>) {
    let mut client: Option<DiscordIpcClient> = None;
    let mut current: Option<(String, String)> = None;

    loop {
        match receiver.recv_timeout(Duration::from_secs(12)) {
            Ok(PresenceMessage::Update { enabled, details, state }) => {
                if enabled {
                    current = Some((details, state));
                } else {
                    current = None;
                    if let Some(mut connected) = client.take() {
                        let _ = connected.clear_activity();
                        let _ = connected.close();
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }

        let Some((details, state)) = current.as_ref() else {
            continue;
        };

        if client.is_none() {
            let mut candidate = DiscordIpcClient::new(DISCORD_APPLICATION_ID);
            if candidate.connect().is_ok() {
                client = Some(candidate);
            } else {
                continue;
            }
        }

        if let Some(connected) = client.as_mut() {
            let activity = activity::Activity::new()
                .details(details.as_str())
                .state(state.as_str());
            if connected.set_activity(activity).is_err() {
                let _ = connected.close();
                client = None;
            }
        }
    }

    if let Some(mut connected) = client {
        let _ = connected.clear_activity();
        let _ = connected.close();
    }
}

#[tauri::command]
pub fn update_discord_presence(
    presence: State<'_, DiscordPresenceState>,
    enabled: bool,
    details: String,
    state_text: String,
) -> Result<(), String> {
    let details = details.chars().take(128).collect();
    let state = state_text.chars().take(128).collect();
    presence.sender.send(PresenceMessage::Update { enabled, details, state })
        .map_err(|error| format!("Could not update Discord Rich Presence: {error}"))
}
