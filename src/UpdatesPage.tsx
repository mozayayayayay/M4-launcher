import { useCallback, useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { Download, RefreshCw } from "lucide-react";

type Asset = { name: string; browser_download_url: string };
type Release = { tag_name: string; html_url: string; body?: string; assets: Asset[] };
type UpdateProgress = { instanceId: string; completed: number; total: number; message: string };
type Props = {
  onProgress: (progress: UpdateProgress | null) => void;
  onNotice: (message: string) => void;
};

function versionParts(value: string) {
  return value.replace(/^v/i, "").split(".").map(part => Number.parseInt(part, 10) || 0);
}
function isNewer(candidate: string, current: string) {
  const a = versionParts(candidate), b = versionParts(current);
  for (let i = 0; i < Math.max(a.length, b.length); i++) {
    if ((a[i] || 0) !== (b[i] || 0)) return (a[i] || 0) > (b[i] || 0);
  }
  return false;
}

const RELEASES_API = "https://api.github.com/repos/mozayayayayay/M4-launcher/releases/latest";

export default function UpdatesPage({ onProgress, onNotice }: Props) {
  const [release, setRelease] = useState<Release | null>(null);
  const [checking, setChecking] = useState(true);
  const [failed, setFailed] = useState(false);
  const [currentVersion, setCurrentVersion] = useState("0.1.7");
  const [nativeApp, setNativeApp] = useState(false);
  const [pendingUpdate, setPendingUpdate] = useState<Update | null>(null);
  const [installing, setInstalling] = useState(false);

  const checkForUpdates = useCallback(async () => {
    setChecking(true);
    setFailed(false);
    setPendingUpdate(null);

    try {
      const response = await fetch(RELEASES_API, {
        headers: { Accept: "application/vnd.github+json" },
        cache: "no-store"
      });
      if (!response.ok) throw new Error("Release lookup failed");
      setRelease(await response.json() as Release);
    } catch {
      setFailed(true);
    }

    try {
      const installedVersion = await getVersion();
      setNativeApp(true);
      setCurrentVersion(installedVersion);
      const update = await check();
      setPendingUpdate(update);
    } catch {
      // In a browser preview, getVersion is unavailable. If the native app has
      // no signed update yet, the regular release download links remain usable.
    } finally {
      setChecking(false);
    }
  }, []);

  useEffect(() => {
    void checkForUpdates();
  }, [checkForUpdates]);

  const latest = release?.tag_name ?? "";
  const available = Boolean(latest && isNewer(latest, currentVersion));
  const linux = release?.assets.find(asset => asset.name.toLowerCase().endsWith(".deb"));
  const windows = release?.assets.find(asset => asset.name.toLowerCase().endsWith(".exe"));

  async function installUpdate() {
    if (!pendingUpdate || installing) return;
    setInstalling(true);
    let downloaded = 0;
    let total = 1;
    onProgress({
      instanceId: "launcher-update",
      completed: 0,
      total,
      message: "Downloading M0Z4 launcher update..."
    });

    try {
      await pendingUpdate.downloadAndInstall((event) => {
        if (event.event === "Started") {
          total = event.data.contentLength ?? 1;
          onProgress({
            instanceId: "launcher-update",
            completed: downloaded,
            total: Math.max(total, 1),
            message: "Downloading M0Z4 launcher update..."
          });
        } else if (event.event === "Progress") {
          downloaded += event.data.chunkLength;
          onProgress({
            instanceId: "launcher-update",
            completed: downloaded,
            total: Math.max(total, downloaded || 1),
            message: "Downloading M0Z4 launcher update..."
          });
        } else if (event.event === "Finished") {
          onProgress({
            instanceId: "launcher-update",
            completed: 1,
            total: 1,
            message: "Installing update and restarting M0Z4..."
          });
        }
      });
      await relaunch();
    } catch (error) {
      onProgress(null);
      onNotice(`Couldn't install the launcher update: ${String(error)}`);
      setInstalling(false);
      try { await pendingUpdate.close(); } catch { /* no-op */ }
    }
  }

  return (
    <section className="updates-page">
      <div className="page-heading">
        <div><div className="eyebrow">M0Z4 RELEASES</div><h1>M0Z4 Launcher updates</h1><p>Check for the latest launcher release and update M0Z4 without manually running a new installer.</p></div>
        <button
          type="button"
          className="secondary-button restored-action-button"
          onClick={() => void checkForUpdates()}
          disabled={checking || installing}
        >
          <RefreshCw size={15} /> {checking ? "Checking…" : "Check again"}
        </button>
      </div>
      <div className="update-release-card">
        {checking ? <p className="update-message">Checking for updates…</p> : failed && !release ? <p className="update-message">Could not check for updates. Check your internet connection and try again.</p> : release ? <>
          <div className="update-version-row">
            <div className="update-version"><span className="eyebrow">CURRENT VERSION</span><strong>v{currentVersion}</strong></div>
            <div className="update-version"><span className="eyebrow">LATEST VERSION</span><strong>{latest}</strong></div>
          </div>
          <h2>{pendingUpdate || available ? "A new version is available." : "You are using the latest version."}</h2>
          <p className="update-message">{pendingUpdate ? "Click Update now to download and install the signed update. Your instances, mods, modpacks and settings stay in your launcher data folder." : nativeApp && available ? "This release is available, but its signed in-app update package is not ready. You can still use the installer links below." : "Updates are checked against the official M0Z4 GitHub releases."}</p>
          {release.body?.trim() && (
            <div className="update-release-notes">
              <span className="eyebrow">WHAT'S INCLUDED</span>
              <p className="update-message" style={{ whiteSpace: "pre-wrap" }}>{release.body.trim()}</p>
            </div>
          )}
          <div className="update-actions">
            <button
              type="button"
              className="update-download restored-action-button"
              onClick={() => void installUpdate()}
              disabled={!pendingUpdate || installing || checking}
              title={!pendingUpdate ? "No signed in-app update is currently available." : "Download and install the latest signed update."}
            >
              <Download size={15} /> {installing ? "Installing…" : "Update now"}
            </button>
            {linux && <a className="update-download" href={linux.browser_download_url} target="_blank" rel="noreferrer">Download Linux installer (.deb)</a>}
            {windows && <a className="update-download" href={windows.browser_download_url} target="_blank" rel="noreferrer">Download Windows installer (.exe)</a>}

          </div>
          {!linux && !windows && <p className="update-message">No installer for this platform was attached to the latest release.</p>}
        </> : null}
      </div>
    </section>
  );
}
