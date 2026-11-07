# M0Z4 Launcher

A custom Minecraft launcher built with Tauri 2, React, TypeScript and Rust.

## Features in this source revision

- Separate Minecraft instances with independent game folders.
- Mojang version manifest and vanilla installation.
- Vanilla launch and modloader launch-profile support.
- Fabric, Quilt, Forge and NeoForge installer integration, including automatic loader setup when creating an instance and compatible Forge/NeoForge version resolution.
- Modrinth project discovery with compatible-version selection and a Download action.
- Required dependency installation for supported mod installs.
- Modrinth `.mrpack` modpack installation.
- Installed mods and per-instance modpack details.
- Live Minecraft console output.
- Galaxy, Midnight and a cohesive Snowy theme with icy surfaces and cyan typography.
- A compact Home dashboard with a single bottom Launch Instance action.
- Theme, accent, Java, memory, resolution and Minecraft options settings.
- Restore-defaults action.
- GitHub Actions workflow for Linux `.deb` and Windows NSIS setup installers.

## Development

```bash
npm ci
npm run dev
```

For a local Tauri build, install the Tauri 2 prerequisites for your operating system, install the Rust stable toolchain, then run:

```bash
npm run tauri build
```

## Installers

The workflow at `.github/workflows/build-installers.yml` builds:

- Linux `.deb`
- Windows NSIS `*-setup.exe`

Run it from the GitHub Actions tab after pushing the project to a GitHub repository. The workflow output is not a substitute for testing the resulting installers.

## Account and external-client scope

- M0Z4 Accounts uses local offline profiles only; built-in Microsoft authentication is intentionally out of scope.
- Offline profiles cannot join online-mode servers that require authenticated Minecraft accounts.
- Nightly and Horizon integration are intentionally out of scope.

## Important

This revision has passed a TypeScript/TSX syntax parse but has not passed a full Rust/Tauri build in the editing environment. Test loader versions, modpack imports, launching and packaging before distributing it.
