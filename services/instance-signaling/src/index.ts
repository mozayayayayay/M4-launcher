export interface Env {
  ROOMS: DurableObjectNamespace;
}

const CODE_ALPHABET = "23456789ABCDEFGHJKLMNPQRSTUVWXYZ";
const ROOM_TTL_MS = 10 * 60 * 1000;
const MAX_SIGNAL_BYTES = 48 * 1024;
const ALLOWED_TYPES = new Set(["offer", "answer", "ice", "ready", "reject", "close"]);

function json(data: unknown, status = 200): Response {
  return new Response(JSON.stringify(data), {
    status,
    headers: {
      "content-type": "application/json; charset=utf-8",
      "cache-control": "no-store",
      "access-control-allow-origin": "*",
      "access-control-allow-methods": "GET, POST, OPTIONS",
      "access-control-allow-headers": "content-type",
    },
  });
}

function makeCode(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(8));
  return Array.from(bytes, (byte) => CODE_ALPHABET[byte % CODE_ALPHABET.length]).join("");
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);
    if (request.method === "OPTIONS") return new Response(null, { status: 204, headers: { "access-control-allow-origin": "*", "access-control-allow-methods": "GET, POST, OPTIONS", "access-control-allow-headers": "content-type" } });
    if (url.pathname === "/health" && request.method === "GET") return json({ ok: true, service: "m4-instance-signaling" });

    if (url.pathname === "/api/rooms" && request.method === "POST") {
      // A small collision retry loop; each code is a short-lived, single-use capability.
      for (let attempt = 0; attempt < 5; attempt++) {
        const code = makeCode();
        const id = env.ROOMS.idFromName(code);
        const room = env.ROOMS.get(id);
        const created = await room.fetch("https://room.internal/create", { method: "POST" });
        if (created.ok) {
          const result = await created.json() as { expiresAt: number };
          return json({ code, expiresAt: result.expiresAt }, 201);
        }
      }
      return json({ error: "Could not allocate a sharing code. Try again." }, 503);
    }

    const match = url.pathname.match(/^\/api\/rooms\/([23456789A-HJ-NP-Z]{8})\/ws$/);
    if (match && request.method === "GET") {
      const role = url.searchParams.get("role");
      if (role !== "sender" && role !== "receiver") return json({ error: "Invalid role." }, 400);
      const id = env.ROOMS.idFromName(match[1]);
      return env.ROOMS.get(id).fetch(request);
    }

    return json({ error: "Not found." }, 404);
  },
};

export class ShareRoom {
  private sockets = new Map<string, WebSocket>();

  constructor(private state: DurableObjectState, _env: Env) {}

  async fetch(request: Request): Promise<Response> {
    const url = new URL(request.url);

    if (url.pathname === "/create" && request.method === "POST") {
      const existing = await this.state.storage.get<number>("expiresAt");
      if (existing && existing > Date.now()) return new Response("exists", { status: 409 });
      const expiresAt = Date.now() + ROOM_TTL_MS;
      await this.state.storage.put({
        expiresAt,
        receiverUsed: false,
        consumed: false,
      });
      return Response.json({ expiresAt });
    }

    if (request.headers.get("Upgrade")?.toLowerCase() !== "websocket") {
      return new Response("WebSocket upgrade required.", { status: 426 });
    }

    const role = url.searchParams.get("role");
    if (role !== "sender" && role !== "receiver") return new Response("Invalid role.", { status: 400 });

    const expiresAt = await this.state.storage.get<number>("expiresAt");
    if (!expiresAt || expiresAt <= Date.now()) return new Response("This sharing code has expired.", { status: 410 });

    if (role === "receiver") {
      const used = await this.state.storage.get<boolean>("receiverUsed");
      if (used) return new Response("This sharing code has already been used.", { status: 409 });
      if (!this.sockets.has("sender")) return new Response("The sender is not connected yet.", { status: 409 });
      await this.state.storage.put("receiverUsed", true);
    } else if (this.sockets.has("sender")) {
      return new Response("The sender is already connected.", { status: 409 });
    }

    const pair = new WebSocketPair();
    const client = pair[0];
    const server = pair[1];
    server.accept();
    this.sockets.set(role, server);

    server.addEventListener("message", (event) => {
      if (typeof event.data !== "string" || new TextEncoder().encode(event.data).byteLength > MAX_SIGNAL_BYTES) {
        server.close(1009, "Signaling message too large");
        return;
      }
      let message: Record<string, unknown>;
      try {
        message = JSON.parse(event.data);
      } catch {
        server.close(1003, "Invalid signaling message");
        return;
      }
      if (!message || typeof message.type !== "string" || !ALLOWED_TYPES.has(message.type)) {
        server.close(1008, "Unsupported signaling message");
        return;
      }
      // Only SDP/ICE negotiation metadata is forwarded. File/data-channel payloads are never accepted here.
      const payload = message.payload;
      if (payload !== undefined && typeof payload !== "object" && typeof payload !== "string") {
        server.close(1008, "Invalid signaling payload");
        return;
      }
      const peer = this.sockets.get(role === "sender" ? "receiver" : "sender");
      if (peer && peer.readyState === WebSocket.READY_STATE_OPEN) {
        try { peer.send(JSON.stringify({ type: message.type, payload })); } catch { /* peer disconnected */ }
      }
      if (message.type === "close" || message.type === "reject") {
        this.closeRoom(1000, "Sharing session ended");
      }
    });

    server.addEventListener("close", () => {
      this.sockets.delete(role);
      const peer = this.sockets.get(role === "sender" ? "receiver" : "sender");
      if (peer && peer.readyState === WebSocket.READY_STATE_OPEN) {
        try { peer.send(JSON.stringify({ type: "close" })); } catch { /* ignore */ }
      }
    });

    server.addEventListener("error", () => {
      this.sockets.delete(role);
      this.closeRoom(1011, "Signaling connection failed");
    });

    const peerRole = role === "sender" ? "receiver" : "sender";
    const peer = this.sockets.get(peerRole);
    if (peer && peer.readyState === WebSocket.READY_STATE_OPEN) {
      try { peer.send(JSON.stringify({ type: "ready" })); } catch { /* ignore */ }
      try { server.send(JSON.stringify({ type: "ready" })); } catch { /* ignore */ }
    }

    return new Response(null, { status: 101, webSocket: client });
  }

  private closeRoom(code: number, reason: string) {
    for (const socket of this.sockets.values()) {
      try { socket.close(code, reason); } catch { /* ignore */ }
    }
    this.sockets.clear();
  }

  webSocketMessage(_socket: WebSocket, _message: string | ArrayBuffer) {
    // Events are handled by listeners registered during fetch.
  }

  webSocketClose(socket: WebSocket, code: number, reason: string) {
    for (const [role, current] of this.sockets.entries()) {
      if (current === socket) this.sockets.delete(role);
    }
    const peer = this.sockets.values().next().value as WebSocket | undefined;
    if (peer?.readyState === WebSocket.READY_STATE_OPEN) {
      try { peer.send(JSON.stringify({ type: "close", payload: { code, reason } })); } catch { /* ignore */ }
    }
  }

  webSocketError(socket: WebSocket) {
    this.webSocketClose(socket, 1011, "Signaling error");
  }
}
