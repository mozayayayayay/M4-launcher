use std::{collections::HashMap, sync::{Arc, Mutex}};
use bytes::BytesMut;
use serde_json::Value;
use tauri::{AppHandle, Emitter, State};
use webrtc::{
    data_channel::{DataChannel, DataChannelEvent},
    peer_connection::{
        PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler,
        RTCConfigurationBuilder, RTCIceServer, RTCPeerConnectionIceEvent,
        RTCPeerConnectionState, RTCIceCandidateInit, RTCSessionDescription,
    },
};

#[derive(Clone, Default)]
pub struct NativeShareState {
    peers: Arc<Mutex<HashMap<String, Arc<dyn PeerConnection>>>>,
    channels: Arc<Mutex<HashMap<String, Arc<dyn DataChannel>>>>,
}

#[derive(Clone)]
struct ShareEvents {
    app: AppHandle,
    peer_id: String,
    state: NativeShareState,
}

impl ShareEvents {
    fn attach_channel(&self, channel: Arc<dyn DataChannel>) {
        let id = self.peer_id.clone();
        if let Ok(mut channels) = self.state.channels.lock() {
            channels.insert(id.clone(), channel.clone());
        }
        let app = self.app.clone();
        tauri::async_runtime::spawn(async move {
            while let Some(event) = channel.poll().await {
                match event {
                    DataChannelEvent::OnOpen => {
                        let _ = app.emit("native-share-channel", serde_json::json!({
                            "peerId": id, "state": "open"
                        }));
                    }
                    DataChannelEvent::OnMessage(message) => {
                        let _ = app.emit("native-share-message", serde_json::json!({
                            "peerId": id,
                            "isString": message.is_string,
                            "data": message.data.to_vec()
                        }));
                    }
                    DataChannelEvent::OnClose => {
                        let _ = app.emit("native-share-channel", serde_json::json!({
                            "peerId": id, "state": "closed"
                        }));
                        break;
                    }
                    _ => {}
                }
            }
        });
    }
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for ShareEvents {
    async fn on_ice_candidate(&self, event: RTCPeerConnectionIceEvent) {
        if let Ok(candidate) = serde_json::to_value(event.candidate) {
            if !candidate.is_null() {
                let _ = self.app.emit("native-share-ice", serde_json::json!({
                    "peerId": self.peer_id,
                    "candidate": candidate
                }));
            }
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        let state_name = format!("{state:?}").to_lowercase();
        let _ = self.app.emit("native-share-connection", serde_json::json!({
            "peerId": self.peer_id,
            "state": state_name
        }));
    }

    async fn on_data_channel(&self, channel: Arc<dyn DataChannel>) {
        self.attach_channel(channel);
    }
}

#[tauri::command]
pub async fn native_share_create_peer(
    app: AppHandle,
    state: State<'_, NativeShareState>,
    peer_id: String,
    role: String,
) -> Result<(), String> {
    if peer_id.trim().is_empty() || peer_id.len() > 128 {
        return Err("Invalid native sharing session identifier.".into());
    }
    if role != "sender" && role != "receiver" {
        return Err("Invalid sharing role.".into());
    }

    let events = ShareEvents { app, peer_id: peer_id.clone(), state: (*state).clone() };
    let configuration = RTCConfigurationBuilder::new()
        .with_ice_servers(vec![RTCIceServer {
            urls: vec!["stun:stun.l.google.com:19302".to_string()],
            ..Default::default()
        }])
        .build();

    let peer = PeerConnectionBuilder::new()
        .with_configuration(configuration)
        .with_handler(Arc::new(events.clone()))
        .with_udp_addrs(vec!["0.0.0.0:0"])
        .with_data_channel_send_buffer_limit(1024 * 1024)
        .build()
        .await
        .map_err(|e| format!("Could not create native WebRTC connection: {e}"))?;

    let peer: Arc<dyn PeerConnection> = Arc::new(peer);
    if role == "sender" {
        let channel = peer.create_data_channel("m4-instance-files", None).await
            .map_err(|e| format!("Could not create direct transfer channel: {e}"))?;
        events.attach_channel(channel);
    }
    state.peers.lock().map_err(|_| "Native sharing state is unavailable.")?
        .insert(peer_id, peer);
    Ok(())
}

#[tauri::command]
pub async fn native_share_create_offer(
    state: State<'_, NativeShareState>, peer_id: String
) -> Result<Value, String> {
    let peer = state.peers.lock().map_err(|_| "Native sharing state is unavailable.")?
        .get(&peer_id).cloned().ok_or("Native peer connection not found.")?;
    let offer = peer.create_offer(None).await.map_err(|e| e.to_string())?;
    peer.set_local_description(offer).await.map_err(|e| e.to_string())?;
    let local = peer.local_description().await.ok_or("Native WebRTC did not produce a local offer.")?;
    serde_json::to_value(local).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn native_share_create_answer(
    state: State<'_, NativeShareState>, peer_id: String
) -> Result<Value, String> {
    let peer = state.peers.lock().map_err(|_| "Native sharing state is unavailable.")?
        .get(&peer_id).cloned().ok_or("Native peer connection not found.")?;
    let answer = peer.create_answer(None).await.map_err(|e| e.to_string())?;
    peer.set_local_description(answer).await.map_err(|e| e.to_string())?;
    let local = peer.local_description().await.ok_or("Native WebRTC did not produce a local answer.")?;
    serde_json::to_value(local).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn native_share_set_remote(
    state: State<'_, NativeShareState>, peer_id: String, description: Value
) -> Result<(), String> {
    let peer = state.peers.lock().map_err(|_| "Native sharing state is unavailable.")?
        .get(&peer_id).cloned().ok_or("Native peer connection not found.")?;
    let description: RTCSessionDescription = serde_json::from_value(description).map_err(|e| e.to_string())?;
    peer.set_remote_description(description).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn native_share_add_ice(
    state: State<'_, NativeShareState>, peer_id: String, candidate: Value
) -> Result<(), String> {
    let peer = state.peers.lock().map_err(|_| "Native sharing state is unavailable.")?
        .get(&peer_id).cloned().ok_or("Native peer connection not found.")?;
    let candidate: RTCIceCandidateInit = serde_json::from_value(candidate).map_err(|e| e.to_string())?;
    peer.add_ice_candidate(candidate).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn native_share_send_text(
    state: State<'_, NativeShareState>, peer_id: String, text: String
) -> Result<(), String> {
    let channel = state.channels.lock().map_err(|_| "Native sharing state is unavailable.")?
        .get(&peer_id).cloned().ok_or("Direct transfer channel is not ready.")?;
    channel.send_text(&text).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn native_share_send_binary(
    state: State<'_, NativeShareState>, peer_id: String, bytes: Vec<u8>
) -> Result<(), String> {
    if bytes.is_empty() || bytes.len() > 16 * 1024 {
        return Err("Invalid direct transfer chunk size.".into());
    }
    let channel = state.channels.lock().map_err(|_| "Native sharing state is unavailable.")?
        .get(&peer_id).cloned().ok_or("Direct transfer channel is not ready.")?;
    channel.send(BytesMut::from(bytes.as_slice())).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn native_share_close_peer(
    state: State<'_, NativeShareState>, peer_id: String
) -> Result<(), String> {
    if let Some(channel) = state.channels.lock().map_err(|_| "Native sharing state is unavailable.")?
        .remove(&peer_id) {
        let _ = channel.close().await;
    }
    if let Some(peer) = state.peers.lock().map_err(|_| "Native sharing state is unavailable.")?
        .remove(&peer_id) {
        peer.close().await.map_err(|e| e.to_string())?;
    }
    Ok(())
}
