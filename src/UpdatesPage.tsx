import { useCallback, useEffect, useState } from "react";

const CURRENT_VERSION = "0.1.3";
const RELEASES_API = "https://api.github.com/repos/mozayayayayay/M4-launcher/releases/latest";
type Asset = { name: string; browser_download_url: string };
type Release = { tag_name: string; html_url: string; body?: string; assets: Asset[] };

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
export default function UpdatesPage() {
  const [release, setRelease] = useState<Release | null>(null);
  const [checking, setChecking] = useState(true);
  const [failed, setFailed] = useState(false);
  const check = useCallback(async () => {
    setChecking(true); setFailed(false);
    try {
      const response = await fetch(RELEASES_API, { headers: { Accept: "application/vnd.github+json" }, cache: "no-store" });
      if (!response.ok) throw new Error("Release lookup failed");
      setRelease(await response.json() as Release);
    } catch { setFailed(true); }
    finally { setChecking(false); }
  }, []);
  useEffect(() => { void check(); }, [check]);
  const latest = release?.tag_name ?? "";
  const available = Boolean(latest && isNewer(latest, CURRENT_VERSION));
  const linux = release?.assets.find(asset => asset.name.toLowerCase().endsWith(".deb"));
  const windows = release?.assets.find(asset => asset.name.toLowerCase().endsWith(".exe"));
  return (
    <section className="updates-page">
      <div className="page-heading">
        <div><div className="eyebrow">M0Z4 RELEASES</div><h1>M0Z4 Launcher updates</h1><p>Check for the latest launcher release and download the installer for your system.</p></div>
        <button className="secondary-button" onClick={() => void check()} disabled={checking}>{checking ? "Checking…" : "Check again"}</button>
      </div>
      <div className="update-release-card">
        {checking ? <p className="update-message">Checking for updates…</p> : failed ? <p className="update-message">Could not check for updates. Check your internet connection and try again.</p> : release ? <>
          <div className="update-version-row">
            <div className="update-version"><span className="eyebrow">CURRENT VERSION</span><strong>v{CURRENT_VERSION}</strong></div>
            <div className="update-version"><span className="eyebrow">LATEST VERSION</span><strong>{latest}</strong></div>
          </div>
          <h2>{available ? "A new version is available." : "You are using the latest version."}</h2>
          <p className="update-message">Update downloads open the official GitHub release page. Close M0Z4 before running the installer.</p>
          <div className="update-actions">
            {linux && <a className="update-download" href={linux.browser_download_url} target="_blank" rel="noreferrer">Download Linux installer (.deb)</a>}
            {windows && <a className="update-download" href={windows.browser_download_url} target="_blank" rel="noreferrer">Download Windows installer (.exe)</a>}
            <a className="update-download secondary" href={release.html_url} target="_blank" rel="noreferrer">Release notes</a>
          </div>
          {!linux && !windows && <p className="update-message">No installer for this platform was attached to the latest release.</p>}
        </> : null}
      </div>
    </section>
  );
}
