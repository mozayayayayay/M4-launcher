# M0Z4 Launcher — development status

## Agreed scope
- Modrinth is the only supported source for mods, modpacks, texture packs and shaders. TLauncher / TLMods integration was scrapped. CurseForge support and API-key settings are out of scope.
- Local offline profiles only. Built-in Microsoft authentication is out of scope.
- No Nightly section, Nightly instances, Nightly runtime integration or external Horizon launcher hand-off.
- Mod and modpack search results use a Download action and compatible-version picker; no descriptions or gallery views.
- Appearance options are Galaxy, Midnight and Snowy. Snowy replaces the former Light option and uses an icy background with cyan text; saved `light` settings migrate to Snowy.
- Preserve the existing UI and the original bottom Play Instance control. Make no unrequested layout or visual changes.
- The Home bottom action is a single Launch Instance button; the separate Home mod-loader install panel/button is removed.
- Selecting Fabric, Quilt, Forge or NeoForge during instance creation automatically installs Minecraft and the selected loader. Forge/NeoForge resolve a latest compatible release when no explicit loader version is entered; Launch Instance retries an incomplete loader installation.
- Snowy uses a consistent icy/frosted-glass palette across navigation, cards, forms, mod pages, settings, accounts, dialogs and console, with cyan text.
- Home dashboard is constrained to the available viewport and does not scroll; its overview cards remain in a three-column row at supported window sizes.

## Implemented in the current source
- Separate Minecraft instances with independent game folders.
- Mojang version manifest and vanilla installation.
- Vanilla launch and modloader launch-profile support.
- Instance creation with automatic selected-loader installation and automatic compatible Forge/NeoForge version resolution.
- Fabric, Quilt, Forge and NeoForge installer integration.
- Modrinth project discovery, compatible version selection and download/install actions.
- Dedicated top-level Texture Packs and Shaders pages backed by Modrinth resourcepack/shader project types, with Minecraft-version filtering and installation into the instance `resourcepacks` and `shaderpacks` folders.
- TLauncher / TLMods source entry is intentionally unavailable pending confirmation of an officially supported public API; no undocumented/private API or scraping.
- Required dependency installation for selected Modrinth mod versions.
- Modrinth `.mrpack` modpack installation, including client files and overrides.
- Dedicated Mods-page sub-navigation with Discover and Installed Mods views, including per-instance removal and live refresh.
- Modpack-specific mod manifest display.
- Visible installation-progress dialog with a progress bar for Minecraft, loader and Modrinth install events.
- Live Minecraft console output.
- Theme, accent, Java, memory, resolution and Minecraft options settings.
- Restore-defaults action.
- Linux `.deb` and Windows NSIS setup GitHub Actions workflow; version 0.1.9 is configured for release.

## Verification still required
- Complete TypeScript production build and Rust/Tauri compilation from a clean dependency install.
- Confirm the new installation-progress dialog and Installed Mods sub-menu on the user machine.
- End-to-end installation and launch for vanilla, Fabric, Quilt, Forge and NeoForge.
- Modrinth dependency resolution and `.mrpack` import against real projects.
- Console streaming, settings persistence, complete Snowy styling, Home viewport fit and restore-defaults behavior.
- Linux `.deb` and Windows `Setup.exe` artifacts from CI, followed by installation tests.

Offline profiles are not Microsoft-authenticated accounts and cannot join online-mode servers that require authentication.

## Verification in this editing session
- TypeScript/TSX syntax transpilation passed for all four frontend source files after the latest changes.
- `src/styles.css` parsed successfully with PostCSS after the latest changes.
- ZIP source archive integrity passed before editing.
- `npm ci` timed out in the editing environment, leaving an incomplete `node_modules` tree; `npm run build` consequently stopped on missing installed type-definition packages. This is an incomplete dependency installation, not a successful production build.
- Rust/Cargo are not installed in the editing environment, so the Tauri backend, actual Minecraft process launch, and native installers could not be compiled or run here.
- The GitHub Actions workflow builds a Linux `.deb` and a Windows NSIS `*-setup.exe`; the Windows job installs NSIS before bundling. These artifacts still need a successful CI run and installation tests.


## v0.1.10 requested UI work
- Remove TLauncher / TLMods completely; Modrinth remains the only content source.
- Move Settings from the top navigation to a gear action anchored at the bottom of the instance sidebar.
- Compact selection menus and text inputs and prevent option labels from clipping vertically.
- Add an accent-colored memory slider with GB tick marks, synchronized with the numeric MB field.
- Reskin Galaxy as Snowy using light gray surfaces, cyan accents and cyan text throughout while retaining the Galaxy nebula/star layout.
- Add subtle mouse-parallax movement to the star layers in every theme.


## v0.1.11 requested visual refinements
- Replace pointer-driven star parallax with slow, seamless CSS-only drift on separate star layers; honor reduced-motion preferences.
- Keep the M4 top-left logo artwork and colors unchanged when switching themes.
- Make the instance sidebar translucent with a theme-matched glass effect so the full-shell starfield remains visible beneath it.
- Style notification toasts to match Galaxy, Midnight and Snowy.
