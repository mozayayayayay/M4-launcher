import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { X, Share2, Download, ShieldCheck, Copy, LoaderCircle } from "lucide-react";

type Mode = "send" | "receive";
type Props = {
  mode: Mode;
  instanceId: string;
  onClose: () => void;
  onNotice: (message: string) => void;
  onImported: (instanceId: string) => void;
};
type ArchiveInfo = {
  archiveId: string; size: number; sha256: string; name: string; version: string;
  loader: string; loaderVersion: string; modCount: number; includeSaves: boolean;
};
type IncomingManifest = {
  size: number; sha256: string; name: string; version: string; loader: string;
  loaderVersion: string; modCount: number; includeSaves: boolean;
};
type SignalMessage = { type: string; payload?: any };

const SIGNALING_BASE = (import.meta.env.VITE_M4_SIGNALING_URL as string | undefined)?.replace(/\/+$/, "");
const CHUNK_SIZE = 16 * 1024;

function formatBytes(value: number): string {
  if (value < 1024) return value + " B";
  if (value < 1024 * 1024) return (value / 1024).toFixed(1) + " KB";
  if (value < 1024 * 1024 * 1024) return (value / (1024 * 1024)).toFixed(1) + " MB";
  return (value / (1024 * 1024 * 1024)).toFixed(2) + " GB";
}
function waitForOpen(socket: WebSocket): Promise<void> {
  return new Promise((resolve, reject) => {
    if (socket.readyState === WebSocket.OPEN) return resolve();
    socket.addEventListener("open", () => resolve(), { once: true });
    socket.addEventListener("error", () => reject(new Error("Could not connect to the signaling service.")), { once: true });
  });
}

export default function InstanceSharing({ mode, instanceId, onClose, onNotice, onImported }: Props) {
  const [includeSaves, setIncludeSaves] = useState(false);
  const [code, setCode] = useState("");
  const [enteredCode, setEnteredCode] = useState("");
  const [status, setStatus] = useState("Ready");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [manifest, setManifest] = useState<IncomingManifest | null>(null);
  const [approved, setApproved] = useState(false);
  const [progress, setProgress] = useState(0);
  const [transferred, setTransferred] = useState(0);
  const [total, setTotal] = useState(0);
  const socketRef = useRef<WebSocket | null>(null);
  const peerIdRef = useRef("m4-share-" + Math.random().toString(36).slice(2));
  const pendingArchiveRef = useRef<ArchiveInfo | null>(null);
  const pendingSignalsRef = useRef<Array<{ type: string; payload: unknown }>>([]);
  const archiveRef = useRef<string | null>(null);
  const receiveRef = useRef<string | null>(null);
  const offsetRef = useRef(0);
  const manifestRef = useRef<IncomingManifest | null>(null);
  const approvedRef = useRef(false);
  const transferStartedRef = useRef(false);
  const sessionEndedRef = useRef(false);
  const signalingCompleteRef = useRef(false);
  const writeQueueRef = useRef<Promise<void>>(Promise.resolve());

  useEffect(() => {
    let disposed = false;
    const removers: Array<() => void> = [];
    Promise.all([
      listen<{ peerId: string; candidate: unknown }>("native-share-ice", e => { if (e.payload.peerId === peerIdRef.current) sendSignal("ice", e.payload.candidate); }),
      listen<{ peerId: string; state: string }>("native-share-channel", e => {
        if (e.payload.peerId !== peerIdRef.current) return;
        if (e.payload.state === "open") {
          if (mode === "send" && pendingArchiveRef.current) {
            const info = pendingArchiveRef.current;
            void sendNativeText({ type: "manifest", payload: {
              size: info.size, sha256: info.sha256, name: info.name, version: info.version,
              loader: info.loader, loaderVersion: info.loaderVersion, modCount: info.modCount, includeSaves: info.includeSaves
            }}).then(() => setStatus("Waiting for recipient approval...")).catch(e => fail(String(e)));
          } else setStatus("Connected directly. Waiting for instance details...");
        }
        if (e.payload.state === "closed" && !sessionEndedRef.current) fail("The direct connection closed.");
      }),
      listen<{ peerId: string; isString: boolean; data: number[] }>("native-share-message", e => {
        if (e.payload.peerId === peerIdRef.current) void handleNativeMessage(e.payload.isString, e.payload.data);
      }),
      listen<{ peerId: string; state: string }>("native-share-connection", e => {
        if (e.payload.peerId !== peerIdRef.current) return;
        if (e.payload.state === "connected") { signalingCompleteRef.current = true; setStatus("Direct encrypted connection established"); }
        if (e.payload.state === "failed") fail("A direct peer-to-peer connection could not be established. M4 will not relay your files.");
      })
    ]).then(unsubs => { if (disposed) unsubs.forEach(unsub => unsub()); else removers.push(...unsubs); })
      .catch(e => { if (!disposed) fail("Could not initialize native sharing events: " + String(e)); });
    return () => {
      disposed = true; removers.forEach(unsub => unsub());
      void invoke("native_share_close_peer", { peerId: peerIdRef.current }).catch(() => {});
      try { socketRef.current?.close(); } catch { /* ignore */ }
      if (archiveRef.current) void invoke("delete_instance_share_archive", { archiveId: archiveRef.current }).catch(() => {});
      if (receiveRef.current) void invoke("discard_instance_share_receive", { archiveId: receiveRef.current }).catch(() => {});
    };
  }, []);

  function fail(message: string) {
    sessionEndedRef.current = true;
    setError(message);
    setStatus("Transfer stopped");
    setBusy(false);
    void invoke("native_share_close_peer", { peerId: peerIdRef.current }).catch(() => {});
    try { socketRef.current?.close(); } catch { /* ignore */ }
    if (archiveRef.current) {
      void invoke("delete_instance_share_archive", { archiveId: archiveRef.current }).catch(() => {});
      archiveRef.current = null;
    }
    if (receiveRef.current) {
      // The temporary partial archive is discarded when the receiver cancels.
      void invoke("discard_instance_share_receive", { archiveId: receiveRef.current }).catch(() => {});
      receiveRef.current = null;
    }
  }

  function sendSignal(type: string, payload?: unknown) {
    const socket = socketRef.current;
    if (socket?.readyState === WebSocket.OPEN) socket.send(JSON.stringify({ type, payload }));
    else pendingSignalsRef.current.push({ type, payload });
  }
  function flushSignals() {
    const socket = socketRef.current;
    if (socket?.readyState === WebSocket.OPEN) pendingSignalsRef.current.splice(0).forEach(m => socket.send(JSON.stringify(m)));
  }
  function sendNativeText(message: unknown) {
    return invoke("native_share_send_text", { peerId: peerIdRef.current, text: JSON.stringify(message) });
  }
  async function handleNativeMessage(isString: boolean, data: number[]) {
    if (isString) {
      let message: any;
      try { message = JSON.parse(new TextDecoder().decode(new Uint8Array(data))); } catch { return; }
      if (mode === "send") {
        const info = pendingArchiveRef.current;
        if (message.type === "accept" && info) void sendArchive(info);
        if (message.type === "reject") fail("The recipient declined the instance.");
        if (message.type === "imported" && info) {
          sessionEndedRef.current = true; setStatus("Instance sent successfully"); setBusy(false);
          void invoke("delete_instance_share_archive", { archiveId: info.archiveId }).catch(() => {});
          archiveRef.current = null; pendingArchiveRef.current = null; onNotice('Shared "' + info.name + '" successfully.');
        }
      } else if (message.type === "manifest") {
        const received = message.payload as IncomingManifest;
        if (!received || typeof received.name !== "string" || !Number.isFinite(received.size) || received.size <= 0 ||
          received.size > 8 * 1024 * 1024 * 1024 || typeof received.sha256 !== "string" || !/^[a-f0-9]{64}$/i.test(received.sha256)) {
          fail("The sender provided an invalid instance manifest."); return;
        }
        manifestRef.current = received; setManifest(received); setTotal(received.size); setStatus("Review the instance before accepting it.");
      } else if (message.type === "complete") {
        const current = manifestRef.current, archiveId = receiveRef.current;
        if (!current || !archiveId) return;
        setStatus("Verifying files and importing instance...");
        try {
          await writeQueueRef.current;
          if (sessionEndedRef.current || receiveRef.current !== archiveId) return;
          const imported = await invoke<{ id: string; name: string }>("finish_instance_share_receive", { archiveId, expectedSha256: current.sha256 });
          receiveRef.current = null; sessionEndedRef.current = true; setStatus("Instance imported successfully"); setBusy(false);
          onImported(imported.id); onNotice('Imported "' + imported.name + '" into your instance list.');
          await sendNativeText({ type: "imported" }); sendSignal("close");
        } catch (cause) { fail("Couldn't import the shared instance: " + String(cause)); }
      }
    } else if (mode === "receive") {
      const current = manifestRef.current, archiveId = receiveRef.current;
      if (!current || !archiveId || !approvedRef.current) return;
      const bytes = new Uint8Array(data);
      writeQueueRef.current = writeQueueRef.current.then(async () => {
        const offset = offsetRef.current;
        await invoke("write_instance_share_chunk", { archiveId, offset, archiveSize: current.size, bytes: Array.from(bytes) });
        offsetRef.current += bytes.byteLength; setTransferred(offsetRef.current); setProgress(Math.min(100, offsetRef.current / current.size * 100));
      }).catch(cause => fail("Couldn't save an incoming file chunk: " + String(cause)));
    }
  }

  async function sendArchive(info: ArchiveInfo) {
    if (transferStartedRef.current) return;
    transferStartedRef.current = true;
    setStatus("Sending instance files directly...");
    setTotal(info.size);
    let offset = 0;
    try {
      while (offset < info.size) {
        const length = Math.min(CHUNK_SIZE, info.size - offset);
        const bytes = await invoke<number[]>("read_instance_share_chunk", { archiveId: info.archiveId, offset, length });
        await invoke("native_share_send_binary", { peerId: peerIdRef.current, bytes });
        offset += length;
        setTransferred(offset);
        setProgress(Math.min(100, (offset / info.size) * 100));
      }
      await sendNativeText({ type: "complete" });
      setStatus("Files sent. Waiting for the recipient to verify and import the instance...");
    } catch (cause) { fail("Transfer failed: " + String(cause)); }
  }

  async function startSending() {
    if (!SIGNALING_BASE) {
      setError("Instance sharing is not configured yet. Deploy the M4 signaling service and add its URL to the launcher build.");
      return;
    }
    sessionEndedRef.current = false;
    signalingCompleteRef.current = false;
    setBusy(true); setError(""); setStatus("Preparing instance archive...");
    try {
      const info = await invoke<ArchiveInfo>("create_instance_share_archive", { instanceId, includeSaves });
      archiveRef.current = info.archiveId;
      setTotal(info.size);
      const response = await fetch(SIGNALING_BASE + "/api/rooms", { method: "POST" });
      if (!response.ok) throw new Error("Signaling service returned " + response.status + ".");
      const room = await response.json() as { code: string };
      setStatus("Connecting to the signaling service...");
      pendingArchiveRef.current = info;
      await invoke("native_share_create_peer", { peerId: peerIdRef.current, role: "sender" });
      const socket = new WebSocket(SIGNALING_BASE.replace(/^http/, "ws") + "/api/rooms/" + room.code + "/ws?role=sender");
      socketRef.current = socket;
      socket.onmessage = async (event) => {
        let message: SignalMessage;
        try { message = JSON.parse(String(event.data)); } catch { return; }
        if (message.type === "ready") {
          try { const offer = await invoke("native_share_create_offer", { peerId: peerIdRef.current }); sendSignal("offer", offer); }
          catch (cause) { fail("Could not negotiate a direct connection: " + String(cause)); }
        } else if (message.type === "answer" && message.payload) {
          try { await invoke("native_share_set_remote", { peerId: peerIdRef.current, description: message.payload }); }
          catch (cause) { fail("Invalid connection response: " + String(cause)); }
        } else if (message.type === "ice" && message.payload) {
          try { await invoke("native_share_add_ice", { peerId: peerIdRef.current, candidate: message.payload }); } catch { /* stale candidate */ }
        } else if (message.type === "close" && !sessionEndedRef.current && !signalingCompleteRef.current) fail("The other launcher disconnected.");
      };
      socket.onopen = flushSignals;
      socket.onerror = () => fail("Lost connection to the signaling service.");
      socket.onclose = () => {
        if (!signalingCompleteRef.current && !sessionEndedRef.current) fail("The signaling session closed before the direct connection was established.");
      };
      await waitForOpen(socket);
      setCode(room.code);
      setStatus("Waiting for the other M4 user to enter your code...");
    } catch (cause) { fail("Couldn't start sharing: " + String(cause)); }
  }

  async function connectToSender() {
    if (!SIGNALING_BASE) {
      setError("Instance sharing is not configured yet. Deploy the M4 signaling service and add its URL to the launcher build.");
      return;
    }
    const normalized = enteredCode.trim().toUpperCase();
    if (!/^[23456789A-HJ-NP-Z]{8}$/.test(normalized)) { setError("Enter the eight-character sharing code."); return; }
    sessionEndedRef.current = false;
    signalingCompleteRef.current = false;
    setBusy(true); setError(""); setStatus("Connecting to sender...");
    try {
      await invoke("native_share_create_peer", { peerId: peerIdRef.current, role: "receiver" });
      const socket = new WebSocket(SIGNALING_BASE.replace(/^http/, "ws") + "/api/rooms/" + normalized + "/ws?role=receiver");
      socketRef.current = socket;
      socket.onmessage = async (event) => {
        let message: SignalMessage;
        try { message = JSON.parse(String(event.data)); } catch { return; }
        if (message.type === "offer" && message.payload) {
          try {
            await invoke("native_share_set_remote", { peerId: peerIdRef.current, description: message.payload });
            const answer = await invoke("native_share_create_answer", { peerId: peerIdRef.current });
            sendSignal("answer", answer);
          } catch (cause) { fail("Could not answer the sender: " + String(cause)); }
        } else if (message.type === "ice" && message.payload) {
          try { await invoke("native_share_add_ice", { peerId: peerIdRef.current, candidate: message.payload }); } catch { /* stale candidate */ }
        } else if (message.type === "close" && !sessionEndedRef.current && !signalingCompleteRef.current) fail("The sender disconnected.");
      };
      socket.onopen = flushSignals;
      socket.onerror = () => fail("Lost connection to the signaling service.");
      socket.onclose = () => {
        if (!signalingCompleteRef.current && !sessionEndedRef.current) fail("The signaling session closed before the direct connection was established.");
      };
      await waitForOpen(socket);
    } catch (cause) { fail("Couldn't connect using that code: " + String(cause)); }
  }

  async function approveImport() {
    const current = manifestRef.current;
    if (!current) return;
    try {
      const archiveId = await invoke<string>("begin_instance_share_receive", { archiveSize: current.size });
      receiveRef.current = archiveId;
      offsetRef.current = 0;
      approvedRef.current = true;
      writeQueueRef.current = Promise.resolve();
      setApproved(true);
      setStatus("Receiving instance files directly...");
      await sendNativeText({ type: "accept" });
    } catch (cause) { fail("Couldn't prepare the import: " + String(cause)); }
  }

  function declineImport() {
    approvedRef.current = false;
    void sendNativeText({ type: "reject" }).catch(() => {});
    sendSignal("reject");
    fail("You declined this instance.");
  }

  function copyCode() {
    if (!code) return;
    void navigator.clipboard?.writeText(code).then(() => onNotice("Sharing code copied.")).catch(() => {});
  }

  const isSending = mode === "send";
  return (
    <div className="modal-backdrop share-modal-backdrop" onClick={() => !busy && onClose()}>
      <div className="modal share-modal" onClick={(event) => event.stopPropagation()}>
        <div className="modal-heading">
          <div><span className="eyebrow">M4 DIRECT TRANSFER</span><h3>{isSending ? "Share instance" : "Receive instance"}</h3></div>
          <button className="icon-button" onClick={onClose} disabled={busy}><X size={18} /></button>
        </div>

        {!SIGNALING_BASE && <div className="modal-note">The signaling service has not been configured in this build yet.</div>}

        {isSending ? (
          <>
            <p className="share-description">Send this instance directly to another M4 user. Both launchers must stay open until the transfer finishes.</p>
            <label className="share-checkbox">
              <input type="checkbox" checked={includeSaves} onChange={(event) => setIncludeSaves(event.target.checked)} disabled={busy || Boolean(code)} />
              Include single-player worlds (larger transfer)
            </label>
            {!code ? (
              <button className="primary-button full" onClick={() => void startSending()} disabled={busy || !SIGNALING_BASE}>
                {busy ? <LoaderCircle size={16} className="share-spinner" /> : <Share2 size={16} />}
                {busy ? "Preparing..." : "Generate sharing code"}
              </button>
            ) : (
              <div className="share-code-panel">
                <span className="eyebrow">YOUR ONE-TIME CODE</span>
                <div className="share-code-row"><strong className="share-code">{code}</strong><button className="icon-button" title="Copy code" onClick={copyCode}><Copy size={16} /></button></div>
                <p>Give this code to the other M4 user. It expires in 10 minutes and accepts one recipient.</p>
              </div>
            )}
          </>
        ) : (
          <>
            <p className="share-description">Enter the one-time code from the sender. Only accept instances from people you trust.</p>
            {!manifest && (
              <div className="share-code-entry">
                <label htmlFor="m4-share-code">Sharing code</label>
                <input id="m4-share-code" value={enteredCode} onChange={(event) => setEnteredCode(event.target.value.toUpperCase().slice(0, 8))} placeholder="8-character code" autoComplete="off" disabled={busy} />
                <button className="primary-button full" onClick={() => void connectToSender()} disabled={busy || enteredCode.trim().length !== 8 || !SIGNALING_BASE}>
                  {busy ? <LoaderCircle size={16} className="share-spinner" /> : <Download size={16} />}
                  {busy ? "Connecting..." : "Connect"}
                </button>
              </div>
            )}
            {manifest && !approved && (
              <div className="share-incoming-card">
                <div className="share-trust"><ShieldCheck size={18} /> Review before importing</div>
                <h4>{manifest.name}</h4>
                <p>Minecraft {manifest.version} · {manifest.loader} {manifest.loaderVersion}</p>
                <p>{manifest.modCount} mods · {formatBytes(manifest.size)}</p>
                <p>{manifest.includeSaves ? "Includes single-player worlds." : "Worlds are not included."}</p>
                <div className="share-approval-actions">
                  <button className="secondary-button" onClick={declineImport}>Decline</button>
                  <button className="primary-button" onClick={() => void approveImport()}>Accept and import</button>
                </div>
              </div>
            )}
          </>
        )}

        {busy && (
          <div className="share-transfer">
            <div className="share-transfer-heading"><span>{status}</span><strong>{total ? progress.toFixed(0) + "%" : ""}</strong></div>
            <div className={"share-progress-track" + (total <= 0 ? " is-indeterminate" : "")}><div className="share-progress-fill" style={{ width: total > 0 ? progress + "%" : "38%" }} /></div>
            {total > 0 ? <p>{formatBytes(transferred)} of {formatBytes(total)}</p> : <p>Preparing the instance archive and secure share link. Large instances can take a little while.</p>}
          </div>
        )}
        {!busy && status !== "Ready" && <p className="share-status">{status}</p>}
        {error && <div className="modal-note share-error">{error}</div>}
        <div className="share-security-note"><ShieldCheck size={14} /> Encrypted direct transfer · No file relay. Your public IP may be visible to the other participant.</div>
        <div className="share-footer">
          <button className="secondary-button" onClick={() => { if (busy) fail("Transfer cancelled."); onClose(); }}>Close</button>
        </div>
      </div>
    </div>
  );
}
