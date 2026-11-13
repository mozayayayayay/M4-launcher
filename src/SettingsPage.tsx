
import { useEffect, useState } from "react";
import type { CSSProperties } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Save, RotateCcw, Palette } from "lucide-react";

type Instance = {
  id: string;
  name: string;
  version: string;
};

type LauncherSettings = {
  memoryMb: number;
  javaPath: string;
  resolutionWidth: number;
  resolutionHeight: number;
  fullscreen: boolean;
  theme: "galaxy" | "dark";
  accentColor: string;
  language: "en" | "ar";
};

type Props = {
  selected: Instance | null;
  onNotice: (message: string) => void;
};

const defaults: LauncherSettings = {
  memoryMb: 4096,
  javaPath: "",
  resolutionWidth: 1280,
  resolutionHeight: 720,
  fullscreen: false,
  theme: "galaxy",
  accentColor: "#9B8CFF",
  language: "en",
};

const optionLabels: Record<string, string> = {
  renderDistance: "Render distance",
  simulationDistance: "Simulation distance",
  graphicsMode: "Graphics",
  particles: "Particles",
  cloudStatus: "Clouds",
  maxFps: "Maximum FPS",
  enableVsync: "VSync",
  entityShadows: "Entity shadows",
  gamma: "Brightness",
  fov: "Field of view",
  guiScale: "GUI scale"
};

const optionChoices: Record<
  string,
  { label: string; value: string }[]
> = {
  graphicsMode: [
    { label: "Fast", value: "0" },
    { label: "Fancy", value: "1" },
    { label: "Fabulous", value: "2" }
  ],
  particles: [
    { label: "All", value: "0" },
    { label: "Decreased", value: "1" },
    { label: "Minimal", value: "2" }
  ],
  cloudStatus: [
    { label: "Off", value: "0" },
    { label: "Fast", value: "1" },
    { label: "Fancy", value: "2" }
  ],
  enableVsync: [
    { label: "Off", value: "false" },
    { label: "On", value: "true" }
  ],
  entityShadows: [
    { label: "Off", value: "false" },
    { label: "On", value: "true" }
  ],
  guiScale: [
    { label: "Auto", value: "0" },
    { label: "1", value: "1" },
    { label: "2", value: "2" },
    { label: "3", value: "3" },
    { label: "4", value: "4" }
  ]
};

function normalizeSettings(
  saved?: Partial<LauncherSettings> | null
): LauncherSettings {
  const savedTheme = (saved as { theme?: string } | null | undefined)?.theme;
  return {
    ...defaults,
    ...saved,
    theme: savedTheme === "dark" ? "dark" : "galaxy",
    javaPath: saved?.javaPath ?? ""
  };
}

export default function SettingsPage({ selected, onNotice }: Props) {
  const [settings, setSettings] = useState<LauncherSettings>(defaults);

  const [minecraftOptions, setMinecraftOptions] = useState<
    Record<string, string>
  >({});

  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    document.documentElement.style.setProperty(
      "--accent-color",
      settings.accentColor
    );
    document.documentElement.dataset.theme = settings.theme;
    document.documentElement.dataset.language = settings.language;
  }, [settings.accentColor, settings.theme, settings.language]);

  useEffect(() => {
    let active = true;

    async function load() {
      setLoading(true);

      try {
        const saved = await invoke<LauncherSettings>(
          "get_launcher_settings"
        );

        if (active) {
          setSettings(normalizeSettings(saved));
        }
      } catch (error) {
        if (active) {
          onNotice(
            `Couldn't load launcher settings: ${String(error)}`
          );
        }
      }

      if (selected) {
        try {
          const options = await invoke<Record<string, string>>(
            "get_minecraft_options",
            { instanceId: selected.id }
          );

          if (active) {
            setMinecraftOptions(options ?? {});
          }
        } catch (error) {
          if (active) {
            onNotice(
              `Couldn't load Minecraft settings: ${String(error)}`
            );
          }
        }
      } else if (active) {
        setMinecraftOptions({});
      }

      if (active) {
        setLoading(false);
      }
    }

    void load();

    return () => {
      active = false;
    };
  }, [selected?.id]);

  function updateSetting<K extends keyof LauncherSettings>(
    key: K,
    value: LauncherSettings[K]
  ) {
    setSettings((current) => ({
      ...current,
      [key]: value
    }));
  }

  function updateMinecraftOption(key: string, value: string) {
    setMinecraftOptions((current) => ({
      ...current,
      [key]: value
    }));
  }

  async function saveAll() {
    if (saving) return;

    setSaving(true);

    try {
      await invoke("save_launcher_settings", { settings });

      if (selected) {
        await invoke("save_minecraft_options", {
          instanceId: selected.id,
          changes: minecraftOptions
        });
      }

      onNotice("Settings saved successfully.");
    } catch (error) {
      onNotice(`Couldn't save settings: ${String(error)}`);
    } finally {
      setSaving(false);
    }
  }

  async function restoreDefaults() {
    if (saving) return;

    setSaving(true);
    try {
      await invoke("save_launcher_settings", { settings: defaults });

      if (selected) {
        await invoke("reset_minecraft_options", {
          instanceId: selected.id
        });
      }

      setSettings(defaults);
      setMinecraftOptions({});
      onNotice("Launcher and Minecraft settings restored to defaults.");
    } catch (error) {
      onNotice(`Couldn't restore defaults: ${String(error)}`);
    } finally {
      setSaving(false);
    }
  }

  function renderOption(key: string) {
    const value = minecraftOptions[key] ?? "";

    if (optionChoices[key]) {
      return (
        <select
          value={value}
          onChange={(event) =>
            updateMinecraftOption(key, event.target.value)
          }
        >
          <option value="">Minecraft default</option>

          {optionChoices[key].map((choice) => (
            <option key={choice.value} value={choice.value}>
              {choice.label}
            </option>
          ))}
        </select>
      );
    }

    const isDecimal = key === "gamma" || key === "fov";

    return (
      <input
        type="number"
        min={
          key === "gamma"
            ? 0
            : key === "fov"
              ? 30
              : 2
        }
        max={
          key === "gamma"
            ? 1
            : key === "fov"
              ? 110
              : key === "renderDistance"
                ? 64
                : key === "simulationDistance"
                  ? 32
                  : 32
        }
        step={isDecimal ? 0.1 : 1}
        placeholder="Minecraft default"
        value={value}
        onChange={(event) =>
          updateMinecraftOption(key, event.target.value)
        }
      />
    );
  }

  return (
    <div className="settings-page">
      <div className="page-heading">
        <div>
          <div className="eyebrow">CONFIGURATION</div>
          <h1>Settings</h1>
          <p>Configure your launcher and Minecraft experience.</p>
        </div>

        <div className="settings-heading-actions">
          <button
            className="secondary-button"
            onClick={restoreDefaults}
            disabled={loading || saving}
          >
            <RotateCcw size={15} /> Restore defaults
          </button>
          <button
            className="primary-button"
            onClick={saveAll}
            disabled={loading || saving}
          >
            <Save size={16} />
            {saving ? "Saving..." : "Save changes"}
          </button>
        </div>
      </div>

      {loading ? (
        <div className="info-card">Loading settings...</div>
      ) : (
        <>
          <div className="section-title">
            <div>
              <h3>Appearance</h3>
              <p>Customize the launcher's theme and accent color.</p>
            </div>
            <Palette size={17} />
          </div>

          <div className="info-card settings-card">
            <label>
              Language
              <select value={settings.language} onChange={(event) => updateSetting("language", event.target.value as LauncherSettings["language"])}>
                <option value="en">English</option>
                <option value="ar">العربية</option>
              </select>
              <small>Choose the language used throughout M0Z4.</small>
            </label>

            <label>
              Theme
              <select
                value={settings.theme}
                onChange={(event) =>
                  updateSetting(
                    "theme",
                    event.target.value as LauncherSettings["theme"]
                  )
                }
              >
                <option value="galaxy">Galaxy</option>
                <option value="dark">Midnight</option>
              </select>
            </label>

            <label>
              Accent color
              <input
                type="color"
                value={settings.accentColor}
                onChange={(event) =>
                  updateSetting("accentColor", event.target.value.toUpperCase())
                }
              />
            </label>
          </div>

          <div className="section-title">
            <div>
              <h3>Java & memory</h3>
              <p>Configure the resources intended for Minecraft.</p>
            </div>
          </div>

          <div className="info-card settings-card">
            <label>
              Memory allocation (MB)

              <input className="memory-number-input" type="number" min={512} max={32768} step={512} value={settings.memoryMb}
                onChange={(event) => updateSetting("memoryMb", Math.min(32768, Math.max(512, Number(event.target.value) || 512)))}
              />
              <div className="memory-slider-wrap">
                <input className="memory-slider" type="range" min={512} max={32768} step={512}
                  value={Math.min(32768, Math.max(512, settings.memoryMb))}
                  style={{ "--memory-progress": `${((settings.memoryMb - 512) / (32768 - 512)) * 100}%` } as CSSProperties}
                  onChange={(event) => updateSetting("memoryMb", Number(event.target.value))}
                  aria-label="Memory allocation in gigabytes"
                />
                <div className="memory-slider-ticks" aria-hidden="true">
                  {Array.from({ length: 9 }, (_, index) => <span key={index}>{index === 0 ? "0.5" : index * 4} GB</span>)}
                </div>
              </div>
              <small>Default: 4096 MB (4 GB). Leave enough memory for your operating system.</small>
            </label>

            <label>
              Java executable path

              <input
                value={settings.javaPath}
                onChange={(event) =>
                  updateSetting("javaPath", event.target.value)
                }
                placeholder="Leave empty to use system Java"
              />
            </label>
          </div>

          <div className="section-title">
            <div>
              <h3>Game window</h3>
              <p>Preferred Minecraft window settings.</p>
            </div>
          </div>

          <div className="info-card settings-card">
            <div className="settings-two-column">
              <label>
                Width

                <input
                  type="number"
                  min={640}
                  max={7680}
                  value={settings.resolutionWidth}
                  onChange={(event) =>
                    updateSetting(
                      "resolutionWidth",
                      Number(event.target.value) || 640
                    )
                  }
                />
              </label>

              <label>
                Height

                <input
                  type="number"
                  min={480}
                  max={4320}
                  value={settings.resolutionHeight}
                  onChange={(event) =>
                    updateSetting(
                      "resolutionHeight",
                      Number(event.target.value) || 480
                    )
                  }
                />
              </label>
            </div>

            <label className="settings-checkbox">
              <input
                type="checkbox"
                checked={settings.fullscreen}
                onChange={(event) =>
                  updateSetting("fullscreen", event.target.checked)
                }
              />

              Start in fullscreen
            </label>
          </div>

          <div className="section-title">
            <div>
              <h3>Minecraft settings</h3>
              <p>
                {selected
                  ? `Settings for ${selected.name}.`
                  : "Select an instance to edit its game settings."}
              </p>
            </div>
          </div>

          {!selected ? (
            <div className="info-card">
              Select or create an instance to edit Minecraft settings.
            </div>
          ) : (
            <div className="info-card settings-card">
              {Object.entries(optionLabels).map(([key, label]) => (
                <label key={key}>
                  {label}
                  {renderOption(key)}
                </label>
              ))}
            </div>
          )}

          <div className="modal-note" style={{ marginTop: 16 }}>
            Minecraft options are written to the selected instance's
            <strong> options.txt</strong>. Java, memory, resolution, and
            fullscreen preferences are saved and applied when Minecraft launches.
          </div>

          <button
            className="primary-button"
            style={{ marginTop: 18 }}
            onClick={saveAll}
            disabled={loading || saving}
          >
            <Save size={16} />
            {saving ? "Saving..." : "Save all settings"}
          </button>
        </>
      )}
    </div>
  );
}

