# M4 direct instance sharing — signaling service

This Worker is signaling-only. It forwards WebRTC offer/answer/ICE negotiation messages between two M4 clients. It does not accept file chunks, archives, or arbitrary binary payloads. Minecraft data must travel through the encrypted WebRTC data channel directly between the two devices.

## Deploy

1. Create a Cloudflare account and enable its `workers.dev` subdomain.
2. From this directory run `npx wrangler login`.
3. Run `npx wrangler deploy`.
4. The Worker URL will be `https://m4-instance-signaling.<your-workers-dev-subdomain>.workers.dev`.
5. Set that URL as the repository Actions variable `M4_SIGNALING_URL` (without a trailing slash).

The code is eight characters, expires after ten minutes, and permits one receiver. No room history is retained. Durable Object state stores only the expiry and one-time-join flag. Signaling messages are limited to 48 KiB and an explicit allowlist.

## Direct-only networking

WebRTC direct connections can expose each participant's public IP address to the other peer. The M4 dialog warns users and recommends sharing only with people they trust.

The client must configure STUN only and must not configure TURN servers. STUN helps peers discover possible network paths; it does not carry instance files. If ICE cannot establish a direct connection, M4 must end the transfer and explain that the network blocks direct peer-to-peer connections. It must never fall back to a hosted file relay.

## Before production

- Deploy this Worker and configure its URL in the M4 build.
- Test two real devices on different networks, including a restrictive NAT.
- Verify no archive/file payload is sent to this Worker.
- Add abuse/rate controls if the service becomes public.

## Automated deployment from GitHub

To let the repository deploy the Worker after changes are merged to `main`, configure these repository settings:

- Actions secret `CLOUDFLARE_API_TOKEN`: a Cloudflare API token with permission to edit Workers and Durable Objects for the target account.
- Actions variable `CLOUDFLARE_ACCOUNT_ID`: the Cloudflare account ID.
- Actions variable `M4_SIGNALING_URL`: the deployed Worker URL, without a trailing slash.

The deploy workflow intentionally skips when the Cloudflare credentials are absent. The Windows/Linux release workflow refuses to publish if `M4_SIGNALING_URL/health` is not reachable, so a build cannot advertise a non-deployed signaling endpoint.
