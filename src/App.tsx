import { useEffect, useMemo, useRef, useState } from "react";
import type { ChangeEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import {
  ChevronLeft, ChevronRight, PanelLeftClose, PanelLeftOpen, Cpu, Gamepad2, Layers3, Home,
  Library, Plus, Settings, UserRound, X, Image, Sparkles,
  Ellipsis, Download, Search, Trash2, FolderOpen, Package, Terminal, Trash, Share2, Pencil
} from "lucide-react";

import SettingsPage from "./SettingsPage";
import AccountsPage from "./AccountsPage";
import UpdatesPage from "./UpdatesPage";
import { installLocalization } from "./i18n";
import InstanceSharing from "./InstanceSharing";

type Instance = {
  id: string;
  name: string;
  version: string;
  loader: string;
  loaderVersion: string;
  icon: string;
  status: string;
  gameDir: string;
  memoryMb: number;
  javaPath: string | null;
  mods: number;
  launchVersionId?: string | null;
};

type MinecraftVersion = {
  id: string;
  versionType: string;
  url: string;
  releaseTime: string;
};

type ModrinthMod = {
  projectId: string;
  slug: string;
  title: string;
  author: string;
  downloads: number;
  iconUrl: string | null;
  latestVersion: string | null;
};

type ProjectVersion = {
  id: string;
  name?: string;
  versionNumber?: string;
  changelog?: string;
  gameVersions?: string[];
  loaders?: string[];
  files?: { filename?: string; primary?: boolean }[];
  fileName?: string;
  displayName?: string;
  idNumber?: number;
};

type Progress = {
  instanceId: string;
  completed: number;
  total: number;
  message: string;
};

type ConsoleEntry = {
  instanceId: string;
  stream: "stdout" | "stderr" | "system";
  line: string;
  receivedAt: number;
};

type Page = "Home" | "Mods" | "Modpacks" | "Texture Packs" | "Shaders" | "Console" | "Settings" | "Accounts" | "Updates";

const MOD_CATEGORIES = [
  ["adventure", "Adventure"],
  ["decoration", "Decoration"],
  ["economy", "Economy"],
  ["equipment", "Equipment"],
  ["food", "Food"],
  ["game-mechanics", "Game Mechanics"],
  ["library", "Libraries"],
  ["magic", "Magic"],
  ["management", "Management"],
  ["minigame", "Minigames"],
  ["mobs", "Mobs"],
  ["optimization", "Optimization"],
  ["social", "Social"],
  ["technology", "Technology"],
  ["utility", "Utility"],
  ["worldgen", "World Generation"]
];

const INSTANCE_ICONS_STORAGE_KEY = "m0z4-instance-icons";

export default function App() {
  const [instances, setInstances] = useState<Instance[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [page, setPage] = useState<Page>("Home");
  const [collapsed, setCollapsed] = useState(true);
  const [idle, setIdle] = useState(false);
  const [discordEnabled, setDiscordEnabled] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const [sharingMode, setSharingMode] = useState<"send" | "receive" | null>(null);
  const [modal, setModal] = useState<"new" | "install" | null>(null);

  const [newName, setNewName] = useState("");
  const [newVersion, setNewVersion] = useState("1.21.10");
  const [newLoader, setNewLoader] = useState("Vanilla");
  const [newLoaderVersion, setNewLoaderVersion] = useState("");

  const [versions, setVersions] = useState<MinecraftVersion[]>([]);
  const [latestRelease, setLatestRelease] = useState("");
  const [versionFilter, setVersionFilter] = useState("release");
  const [installVersion, setInstallVersion] = useState("");

  const [modQuery, setModQuery] = useState("");
  const [modResults, setModResults] = useState<ModrinthMod[]>([]);
  const [modLoading, setModLoading] = useState(false);
  const [modError, setModError] = useState("");
  const [modPage, setModPage] = useState(0);
  const [modTotalHits, setModTotalHits] = useState(0);
  const [modCategory, setModCategory] = useState("");
  const [modVersion, setModVersion] = useState("");
  const [modPageSize, setModPageSize] = useState(30);
  const [modsTab, setModsTab] = useState<"discover" | "installed">("discover");
  const [selectedProject, setSelectedProject] = useState<ModrinthMod | null>(null);
  const [projectDetails, setProjectDetails] = useState<Record<string, any> | null>(null);
  const [projectVersions, setProjectVersions] = useState<ProjectVersion[]>([]);
  const [selectedProjectVersion, setSelectedProjectVersion] = useState("");
  const [versionsLoading, setVersionsLoading] = useState(false);
  const [installedModpack, setInstalledModpack] = useState<Record<string, any> | null>(null);

  const projectsPageRef = useRef<HTMLDivElement>(null);
  const iconInputRef = useRef<HTMLInputElement>(null);

  const [iconTargetId, setIconTargetId] = useState<string | null>(null);

  const [instanceIcons, setInstanceIcons] = useState<Record<string, string>>(
    () => {
      try {
        const saved = localStorage.getItem(INSTANCE_ICONS_STORAGE_KEY);
        return saved ? JSON.parse(saved) : {};
      } catch {
        return {};
      }
    }
  );

  const [installedMods, setInstalledMods] = useState<string[]>([]);
  const [consoleEntries, setConsoleEntries] = useState<ConsoleEntry[]>([]);
  const consoleEntriesRef = useRef<ConsoleEntry[]>([]);
  const [progress, setProgress] = useState<Progress | null>(null);
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(true);
  const [notice, setNotice] = useState("");

  const selected = useMemo(() => {
    const instance = instances.find(
      (instance) => instance.id === selectedId
    );

    if (!instance) return null;

    return {
      ...instance,
      icon: instanceIcons[instance.id] ?? instance.icon
    };
  }, [instances, selectedId, instanceIcons]);

  async function refreshInstances() {
    const data = await invoke<{
      instances: Instance[];
      selectedInstance: string | null;
    }>("get_instances");

    setInstances(data.instances ?? []);
    setSelectedId(
      data.selectedInstance ?? data.instances?.[0]?.id ?? null
    );
  }

  async function refreshInstalledMods(id: string) {
    try {
      const mods = await invoke<string[]>("get_installed_mods", {
        instanceId: id
      });
      setInstalledMods(mods);
    } catch {
      setInstalledMods([]);
    }
  }

  async function refreshInstalledModpack(id: string) {
    try {
      const pack = await invoke<Record<string, any> | null>("get_installed_modpack", { instanceId: id });
      setInstalledModpack(pack);
    } catch {
      setInstalledModpack(null);
    }
  }

  useEffect(() => {
    const stopLocalization = installLocalization();
    let active = true;

    invoke<{ theme?: string; accentColor?: string; language?: string }>("get_launcher_settings")
      .then((settings) => {
        if (!active) return;
        document.documentElement.dataset.theme = settings.theme === "dark" ? "dark" : "galaxy";
        document.documentElement.dataset.language = settings.language === "ar" ? "ar" : "en";
        document.documentElement.style.setProperty(
          "--accent-color",
          settings.accentColor ?? "#9B8CFF"
        );
      })
      .catch(() => {
        // Keep the built-in appearance if saved settings cannot be loaded.
      });

    return () => {
      active = false;
      stopLocalization();
    };
  }, []);

  useEffect(() => {
    let active = true;

    invoke<{
      instances: Instance[];
      selectedInstance: string | null;
    }>("get_instances")
      .then((data) => {
        if (!active) return;

        const loadedInstances = data.instances ?? [];
        const savedSelection = data.selectedInstance;

        setInstances(loadedInstances);
        setSelectedId(
          savedSelection && loadedInstances.some((i) => i.id === savedSelection)
            ? savedSelection
            : loadedInstances[0]?.id ?? null
        );
      })
      .catch((error) => {
        if (active) setNotice(`Couldn't load instances: ${String(error)}`);
      })
      .finally(() => {
        if (active) setLoading(false);
      });

    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    let active = true;
    invoke<{ discordRpcEnabled?: boolean }>("get_launcher_settings")
      .then((settings) => { if (active) setDiscordEnabled(Boolean(settings.discordRpcEnabled)); })
      .catch(() => undefined);
    return () => { active = false; };
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;

    listen<Progress>("install-progress", (event) => {
      setProgress(event.payload);
    }).then((stop) => {
      unlisten = stop;
    });

    return () => unlisten?.();
  }, []);

  useEffect(() => {
    if (idle) return;
    if (page === "Console") setConsoleEntries(consoleEntriesRef.current);
    let unlisten: (() => void) | undefined;
    let frame = 0;
    let flushScheduled = false;

    listen<Omit<ConsoleEntry, "receivedAt">>("console-output", (event) => {
      const entry: ConsoleEntry = { ...event.payload, receivedAt: Date.now() };
      consoleEntriesRef.current = [...consoleEntriesRef.current, entry].slice(-1500);
      // Keep log output off the main React render path unless the Console is visible.
      if (page === "Console" && !flushScheduled) {
        flushScheduled = true;
        frame = requestAnimationFrame(() => {
          flushScheduled = false;
          setConsoleEntries(consoleEntriesRef.current);
        });
      }
    }).then((stop) => { unlisten = stop; });

    return () => {
      unlisten?.();
      if (frame) cancelAnimationFrame(frame);
    };
  }, [page, idle]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<string>("minecraft-process-exited", () => {
      setIdle(false);
      setNotice("Minecraft closed. M4 is ready.");
    }).then((stop) => { unlisten = stop; });
    return () => unlisten?.();
  }, []);

  useEffect(() => {
    if (selectedId) {
      refreshInstalledMods(selectedId);
      refreshInstalledModpack(selectedId);
    } else {
      setInstalledMods([]);
      setInstalledModpack(null);
    }
  }, [selectedId]);

  useEffect(() => {
    if (selected) {
      setModVersion(selected.version);
      setModPage(0);
    } else {
      setModVersion("");
    }
  }, [selectedId]);

  function chooseInstanceIcon(id: string) {
    setIconTargetId(id);
    iconInputRef.current?.click();
  }

  async function handleIconUpload(event: ChangeEvent<HTMLInputElement>) {
    const file = event.currentTarget.files?.[0];
    const targetId = iconTargetId;

    event.currentTarget.value = "";

    if (!file || !targetId) return;

    const supportedTypes = [
      "image/png",
      "image/jpeg",
      "image/webp"
    ];

    if (!supportedTypes.includes(file.type)) {
      setNotice("Choose a PNG, JPG, or WebP image.");
      return;
    }

    if (file.size > 10 * 1024 * 1024) {
      setNotice("The image must be smaller than 10 MB.");
      return;
    }

    try {
      const imageData = await new Promise<string>((resolve, reject) => {
        const reader = new FileReader();

        reader.onload = () => {
          if (typeof reader.result === "string") {
            resolve(reader.result);
          } else {
            reject(new Error("Couldn't read the image."));
          }
        };

        reader.onerror = () => reject(new Error("Couldn't read the image."));
        reader.readAsDataURL(file);
      });

      const processedImage = await new Promise<string>((resolve, reject) => {
        const image = new window.Image();

        image.onload = () => {
          const maxDimension = 256;
          const scale = Math.min(
            1,
            maxDimension / Math.max(image.width, image.height)
          );

          const canvas = document.createElement("canvas");
          canvas.width = Math.max(1, Math.round(image.width * scale));
          canvas.height = Math.max(1, Math.round(image.height * scale));

          const context = canvas.getContext("2d");

          if (!context) {
            reject(new Error("Couldn't process the image."));
            return;
          }

          context.drawImage(
            image,
            0,
            0,
            canvas.width,
            canvas.height
          );

          try {
            resolve(canvas.toDataURL("image/webp", 0.88));
          } catch {
            reject(new Error("Couldn't convert the image."));
          }
        };

        image.onerror = () => reject(new Error("Invalid image file."));
        image.src = imageData;
      });

      const updatedIcons = {
        ...instanceIcons,
        [targetId]: processedImage
      };

      localStorage.setItem(
        INSTANCE_ICONS_STORAGE_KEY,
        JSON.stringify(updatedIcons)
      );

      setInstanceIcons(updatedIcons);
      setNotice("Instance icon updated.");
    } catch (error) {
      setNotice(`Couldn't update instance icon: ${String(error)}`);
    }
  }

  async function loadVersions() {
    try {
      const data = await invoke<{
        latestRelease: string;
        latestSnapshot: string;
        versions: MinecraftVersion[];
      }>("get_minecraft_versions");

      setVersions(data.versions);
      setLatestRelease(data.latestRelease);

      if (!installVersion) {
        setInstallVersion(data.latestRelease);
      }
    } catch (error) {
      setNotice(`Couldn't fetch Minecraft versions: ${String(error)}`);
    }
  }

  async function openInstaller() {
    setModal("install");
    if (versions.length === 0) await loadVersions();
    if (selected) setInstallVersion(selected.version);
  }

  async function createInstance() {
    if (!newName.trim() || busy) return;

    setBusy(true);
    setProgress({
      instanceId: "new-instance",
      completed: 0,
      total: 1,
      message: newLoader === "Vanilla" ? "Creating instance..." : `Preparing ${newLoader}...`
    });

    try {
      const created = await invoke<Instance>("create_instance", {
        name: newName.trim(),
        version: newVersion,
        loader: newLoader,
        loaderVersion: newLoader === "Vanilla" ? "—" : newLoaderVersion.trim()
      });

      await invoke("select_instance", { id: created.id });
      setSelectedId(created.id);
      setNewName("");
      setNewVersion(latestRelease || "1.21.10");
      setNewLoader("Vanilla");
      setNewLoaderVersion("");
      setModal(null);
      setPage("Home");

      if (newLoader !== "Vanilla") {
        setProgress({
          instanceId: created.id,
          completed: 0,
          total: 1,
          message: `Installing Minecraft and ${newLoader}...`
        });
        try {
          await invoke<string>("install_modloader", {
            instanceId: created.id,
            loader: newLoader,
            loaderVersion: newLoaderVersion.trim() || null
          });
          setNotice(`Created "${created.name}" with ${newLoader} installed and ready.`);
        } catch (loaderError) {
          setNotice(`Created "${created.name}", but ${newLoader} could not be installed: ${String(loaderError)}. You can retry after checking Java and the selected Minecraft version.`);
        }
      } else {
        setNotice(`Created "${created.name}". Install Minecraft using the Launch Instance button.`);
      }

      await refreshInstances();
      setSelectedId(created.id);
    } catch (error) {
      setNotice(`Couldn't create instance: ${String(error)}`);
    } finally {
      setBusy(false);
      setProgress(null);
    }
  }

  async function selectInstance(id: string) {
    try {
      await invoke("select_instance", { id });
      setSelectedId(id);
      setPage("Home");
      setMenuOpen(false);
    } catch (error) {
      setNotice(`Couldn't select instance: ${String(error)}`);
    }
  }

  async function installMinecraft() {
    if (!selected || !installVersion || busy) return;

    setBusy(true);
    setProgress({
      instanceId: selected.id,
      completed: 0,
      total: 1,
      message: "Starting installation..."
    });

    try {
      await invoke("install_minecraft_version", {
        id: selected.id,
        version: installVersion
      });

      await refreshInstances();
      setNotice(`Minecraft ${installVersion} downloaded successfully.`);
      setModal(null);
    } catch (error) {
      setNotice(`Installation failed: ${String(error)}`);
    } finally {
      setBusy(false);
      setProgress(null);
    }
  }

  async function playMinecraft() {
    if (!selected || busy) return;

    setBusy(true);

    try {
      await invoke("launch_minecraft", {
        id: selected.id
      });

      setIdle(true);
      setNotice("");
    } catch (error) {
      setNotice(`Couldn't launch Minecraft: ${String(error)}`);
    } finally {
      setBusy(false);
      setProgress(null);
    }
  }

  async function launchSelectedInstance() {
    if (!selected || busy) return;

    if (selected.loader.toLowerCase() !== "vanilla" && !selected.launchVersionId) {
      setBusy(true);
      try {
        await invoke<string>("install_modloader", {
          instanceId: selected.id,
          loader: selected.loader,
          loaderVersion: selected.loaderVersion && selected.loaderVersion !== "—"
            ? selected.loaderVersion
            : null
        });
        await refreshInstances();
        await invoke("launch_minecraft", { id: selected.id });
        setIdle(true);
        setNotice("");
      } catch (error) {
        setNotice(`Couldn't prepare or launch ${selected.name}: ${String(error)}`);
      } finally {
        setBusy(false);
        setProgress(null);
      }
      return;
    }

    if (isSelectedInstalled) {
      await playMinecraft();
    } else {
      await openInstaller();
    }
  }

  async function searchProjects(pageOverride?: number) {
    if (!selected) {
      setModError("Create or select an instance first.");
      setModResults([]);
      setModTotalHits(0);
      return;
    }

    const requestedPage = pageOverride ?? modPage;
    const projectType = page === "Modpacks" ? "modpack" : page === "Texture Packs" ? "resourcepack" : page === "Shaders" ? "shader" : "mod";

    setModLoading(true);
    setModError("");

    try {
      const searchArgs = {
        query: modQuery.trim(),
        gameVersion: modVersion || selected.version,
        loader: selected.loader,
        category: projectType === "mod" ? modCategory || null : null,
        projectType,
        offset: requestedPage * modPageSize,
        limit: modPageSize
      };


      const result = await invoke<{ hits: ModrinthMod[]; totalHits: number }>("search_modrinth", searchArgs);

      const sortedHits = [...(result.hits ?? [])].sort(
        (a, b) => b.downloads - a.downloads
      );

      setModResults(sortedHits);
      setModTotalHits(result.totalHits ?? 0);

      if (sortedHits.length === 0) {
        setModError(
          projectType === "mod" ? "No compatible mods found for these filters."
            : projectType === "modpack" ? "No compatible modpacks found for these filters."
            : projectType === "resourcepack" ? "No compatible texture packs found for these filters."
            : "No compatible shaders found for these filters."
        );
      }
    } catch (error) {
      setModResults([]);
      setModTotalHits(0);
      setModError(`Search failed: ${String(error)}`);
    } finally {
      setModLoading(false);
    }
  }

  useEffect(() => {
    if (["Mods", "Modpacks", "Texture Packs", "Shaders"].includes(page)) {
      if (selected) {
        void searchProjects();
      } else {
        setModResults([]);
        setModTotalHits(0);
        setModError("Create or select an instance first.");
      }
    }
  }, [
    page,
    selectedId,
    selected?.version,
    selected?.loader,
    modVersion,
    modCategory,
    modPage,
    modPageSize,
  ]);

  useEffect(() => {
    if (
      (["Mods", "Modpacks", "Texture Packs", "Shaders"].includes(page)) &&
      versions.length === 0
    ) {
      void loadVersions();
    }
  }, [page]);

  useEffect(() => {
    if (
      (["Mods", "Modpacks", "Texture Packs", "Shaders"].includes(page)) &&
      projectsPageRef.current
    ) {
      projectsPageRef.current.scrollTo({
        top: 0,
        behavior: "smooth"
      });
    }
  }, [modPage]);

  async function openProjectDownload(mod: ModrinthMod) {
    setSelectedProject(mod);
    setProjectDetails(null);
    setProjectVersions([]);
    setSelectedProjectVersion("");
    setVersionsLoading(true);
    try {
      const [versions, details] = await Promise.all([
        invoke<ProjectVersion[]>("get_modrinth_project_versions", {
          projectId: mod.projectId, gameVersion: modVersion || selected?.version,
          loader: page === "Mods" ? selected?.loader : null
        }),
        invoke<Record<string, any>>("get_modrinth_project_details", {
          projectId: mod.projectId
        }).catch(() => null)
      ]);
      setProjectVersions(versions);
      setProjectDetails(details);
      setSelectedProjectVersion(versions[0]?.id ?? "");
    } catch (error) {
      setNotice(`Couldn't load downloadable versions: ${String(error)}`);
      setSelectedProject(null);
    } finally {
      setVersionsLoading(false);
    }
  }

  async function installSelectedProject() {
    if (!selected || !selectedProject || !selectedProjectVersion) return;
    if (["Mods", "Texture Packs", "Shaders"].includes(page) && modVersion !== selected.version) {
      setNotice("Select the instance Minecraft version before installing this content.");
      return;
    }
    setBusy(true);
    try {
      if (page === "Modpacks") {

        const metadata = await invoke<Record<string, any>>("install_modrinth_modpack", {
          instanceId: selected.id, projectId: selectedProject.projectId, versionId: selectedProjectVersion
        });
        setInstalledModpack(metadata);
        await refreshInstances();
        await refreshInstalledMods(selected.id);
        setNotice(`Installed modpack: ${metadata.name ?? selectedProject.title}`);
      } else if (page === "Texture Packs" || page === "Shaders") {
        const contentType = page === "Texture Packs" ? "resourcepack" : "shader";
        const filename = await invoke<string>("install_modrinth_resource_version", {
          instanceId: selected.id,
          versionId: selectedProjectVersion,
          contentType
        });
        setNotice("Installed " + filename + " in " + (contentType === "resourcepack" ? "resourcepacks" : "shaderpacks") + ".");
      } else {
        const files = await invoke<string[]>("install_modrinth_mod_version", {
          instanceId: selected.id,
          versionId: selectedProjectVersion
        });
        await refreshInstalledMods(selected.id);
        await refreshInstances();
        setNotice("Installed " + files.length + " mod file(s), including required dependencies.");
      }
      setSelectedProject(null);
    } catch (error) {
      setNotice(`Installation failed: ${String(error)}`);
    } finally {
      setBusy(false);
      setProgress(null);
    }
  }


  async function removeMod(filename: string) {
    if (!selected) return;

    try {
      await invoke("remove_mod", {
        instanceId: selected.id,
        filename
      });

      await refreshInstalledMods(selected.id);
      await refreshInstances();
      setNotice(`Removed ${filename}.`);
    } catch (error) {
      setNotice(`Couldn't remove mod: ${String(error)}`);
    }
  }

  async function renameSelectedInstance() {
    if (!selected || busy) return;

    const enteredName = window.prompt("Enter a new name for this instance:", selected.name);
    if (enteredName === null) return;

    const name = enteredName.trim();
    if (!name) {
      setNotice("Instance name cannot be empty.");
      return;
    }
    if (name === selected.name) {
      setMenuOpen(false);
      return;
    }

    try {
      const updated = await invoke<Instance>("update_instance", {
        id: selected.id,
        name,
        version: selected.version,
        loader: selected.loader,
        loaderVersion: selected.loaderVersion
      });
      setInstances((current) =>
        current.map((instance) => instance.id === updated.id ? updated : instance)
      );
      setMenuOpen(false);
      setNotice(`Renamed instance to "${updated.name}".`);
    } catch (error) {
      setNotice(`Couldn't rename instance: ${String(error)}`);
    }
  }

  async function deleteSelectedInstance() {
    if (!selected || busy) return;

    const confirmed = window.confirm(
      `Remove "${selected.name}" from the launcher? Its game files and worlds will be kept.`
    );

    if (!confirmed) return;

    try {
      await invoke("delete_instance", { id: selected.id });
      await refreshInstances();
      setMenuOpen(false);
      setNotice("Instance removed from your library.");
    } catch (error) {
      setNotice(`Couldn't delete instance: ${String(error)}`);
    }
  }

  async function importMrpack() {
    if (busy) return;
    try {
      const selectedFile = await openFileDialog({
        multiple: false,
        directory: false,
        filters: [{ name: "Modrinth modpack", extensions: ["mrpack"] }]
      });
      if (!selectedFile || Array.isArray(selectedFile)) return;
      setBusy(true);
      setProgress({ instanceId: "mrpack-import", completed: 0, total: 1, message: "Reading Modrinth modpack..." });
      const imported = await invoke<Instance>("import_mrpack", { path: selectedFile });
      await invoke("select_instance", { id: imported.id });
      await refreshInstances();
      setSelectedId(imported.id);
      setPage("Home");
      setNotice(`Imported "${imported.name}" (Minecraft ${imported.version}) successfully.`);
    } catch (error) {
      setNotice(`Couldn't import modpack: ${String(error)}`);
    } finally {
      setBusy(false);
      setProgress(null);
    }
  }

  async function openFolder() {
    if (!selected) return;

    try {
      await invoke("open_instance_folder", { id: selected.id });
    } catch (error) {
      setNotice(`Couldn't open folder: ${String(error)}`);
    }
  }

  function openNewInstance() {
    setNewName("");
    setVersionFilter("release");
    setNewLoader("Vanilla");
    setNewLoaderVersion("");
    setNewVersion(latestRelease || versions.find((version) => version.versionType === "release")?.id || "1.21.10");
    setModal("new");
    if (versions.length === 0) void loadVersions();
  }

  const filteredVersions = versions.filter(
    (version) =>
      versionFilter === "all" || version.versionType === versionFilter
  );

  const totalModPages = Math.max(
    1,
    Math.ceil(modTotalHits / modPageSize)
  );

  const isModpacksPage = page === "Modpacks";
  const isModsPage = page === "Mods";
  const isTexturePacksPage = page === "Texture Packs";
  const isShadersPage = page === "Shaders";
  const isSelectedInstalled =
    selected?.status?.toLowerCase() === "installed" &&
    (selected?.loader?.toLowerCase() === "vanilla" || Boolean(selected?.launchVersionId));
  const installedPackMods: string[] = Array.isArray(installedModpack?.mods)
    ? installedModpack.mods
    : installedMods;

  useEffect(() => {
    const instanceLabel = selected
      ? `${selected.name} · ${selected.version} · ${selected.loader}`
      : "No instance selected";
    void invoke("update_discord_presence", {
      enabled: discordEnabled,
      details: idle ? "Playing Minecraft" : "Using M4 Launcher",
      stateText: idle ? instanceLabel : `${page} · ${instanceLabel}`
    }).catch(() => undefined);
  }, [discordEnabled, idle, page, selected?.id, selected?.name, selected?.version, selected?.loader]);

  if (idle) {
    return (
      <div className="idle-screen">
        <div className="idle-message">Instance launched, going into idle state.</div>
        <button className="idle-exit-button" onClick={() => setIdle(false)}>
          Exit idle mode
        </button>
      </div>
    );
  }

  return (
    <div className={`app-shell ${collapsed ? "sidebar-collapsed" : ""}`}>
      <div className="galaxy-background" aria-hidden="true">
        <div className="galaxy-nebula nebula-one" />
        <div className="galaxy-nebula nebula-two" />
        <div className="galaxy-stars stars-one" />
        <div className="galaxy-stars stars-two" />
      </div>

      <input
        ref={iconInputRef}
        type="file"
        accept="image/png,image/jpeg,image/webp"
        style={{ display: "none" }}
        onChange={handleIconUpload}
      />

      <header className="topbar">
        <div className="brand">
          <div className="brand-mark">
            <span className="brand-star brand-star-one">✦</span>
            <span className="brand-star brand-star-two">✧</span>
            <span className="brand-star brand-star-three">·</span>
            <span className="brand-mark-text">M4</span>
            <span className="brand-sparkle">✧</span>
          </div>
          <span className="brand-byline">by M0Z4</span>
        </div>

        <nav className="top-nav">
          {(["Home", "Mods", "Modpacks", "Texture Packs", "Shaders", "Updates", "Console", "Accounts"] as Page[]).map((item) => (
            <button
              key={item}
              className={`nav-item ${page === item ? "active" : ""}`}
              onClick={() => {
                setPage(item);
                setMenuOpen(false);
                setModPage(0);
              }}
            >
              {item === "Home" && <Home size={16} />}
              {item === "Mods" && <Library size={16} />}
              {item === "Modpacks" && <Package size={16} />}
              {item === "Texture Packs" && <Image size={16} />}
              {item === "Shaders" && <Sparkles size={16} />}
              {item === "Updates" && <Download size={16} />}
              {item === "Console" && <Terminal size={16} />}
              {item === "Accounts" && <UserRound size={16} />}
              {item}
            </button>
          ))}
        </nav>
      </header>

      <div className="workspace">
        <aside className={`instance-rail ${collapsed ? "collapsed" : ""}`}>
          <div className="rail-heading">
            {!collapsed && (
              <span>INSTANCES <small>{instances.length}</small></span>
            )}

            <button
              className="icon-button subtle sidebar-toggle"
              onClick={() => setCollapsed(!collapsed)}
              title={collapsed ? "Expand sidebar" : "Collapse sidebar"}
            >
              {collapsed ? <PanelLeftOpen size={17} /> : <PanelLeftClose size={17} />}
            </button>
          </div>

          <div className="instance-list">
            {instances.map((instance) => (
              <button
                key={instance.id}
                className={`instance-item ${selectedId === instance.id ? "selected" : ""}`}
                onClick={() => selectInstance(instance.id)}
                title={instance.name}
              >
                <span className="instance-icon">
                  {instanceIcons[instance.id] || instance.icon?.startsWith("data:image/") ? (
                    <img
                      src={instanceIcons[instance.id] || instance.icon}
                      alt=""
                      className="custom-instance-icon"
                    />
                  ) : (
                    instance.icon || instance.name.slice(0, 2).toUpperCase()
                  )}
                </span>

                {!collapsed && (
                  <span className="instance-name">{instance.name}</span>
                )}

                {!collapsed && selectedId === instance.id && (
                  <span className="selected-dot" />
                )}
              </button>
            ))}
          </div>

          <button
            className={`add-instance ${collapsed ? "icon-only" : ""}`}
            onClick={openNewInstance}
            title="Add instance"
          >
            <Plus size={18} />
            {!collapsed && "Add instance"}
          </button>
          <button
            className={`add-instance mrpack-import-button ${collapsed ? "icon-only" : ""}`}
            onClick={importMrpack}
            disabled={busy}
            title="Import .mrpack"
          >
            <Package size={18} />
            {!collapsed && "Import .mrpack"}
          </button>

          <div className="rail-bottom">
            <button className={`rail-settings-button ${page === "Settings" ? "active" : ""}`} onClick={() => { setPage("Settings"); setMenuOpen(false); }} title="Settings" aria-label="Settings">
              <Settings size={18} />
              {!collapsed && <span>Settings</span>}
            </button>
            {!collapsed && <div className="rail-footer"><span className="online-dot" />{loading ? "Loading library..." : "Launcher ready"}</div>}
          </div>
        </aside>

        <main className="main-content">
          <div key={page} className="page-transition">
            {page === "Home" && (
              <>
                {!selected ? (
                  <div className="placeholder-page">
                    <div className="placeholder-icon">
                      <Gamepad2 size={22} />
                    </div>
                    <div className="eyebrow">YOUR LIBRARY</div>
                    <h1>No instances yet</h1>
                    <p>Create your first Minecraft installation to get started.</p>
                    <button
                      className="primary-button"
                      onClick={openNewInstance}
                    >
                      <Plus size={16} /> Create instance
                    </button>
                  </div>
                ) : (
                  <>
                    <div className="page-heading">
                      <div>
                        <div className="eyebrow">YOUR LIBRARY</div>
                        <h1>Home</h1>
                        <p>Pick up where you left off.</p>
                      </div>
                    </div>

                    <section
                      key={selected.id}
                      className="hero-card instance-banner instance-transition"
                    >
                      <div
                        className="banner-logo"
                        role="button"
                        tabIndex={0}
                        title="Change instance icon"
                        onClick={() => chooseInstanceIcon(selected.id)}
                        onKeyDown={(event) => {
                          if (event.key === "Enter" || event.key === " ") {
                            event.preventDefault();
                            chooseInstanceIcon(selected.id);
                          }
                        }}
                      >
                        {selected.icon?.startsWith("data:image/") ? (
                          <img
                            src={selected.icon}
                            alt={`${selected.name} icon`}
                            className="custom-instance-icon"
                          />
                        ) : (
                          <span>
                            {selected.icon || selected.name.slice(0, 2).toUpperCase()}
                          </span>
                        )}
                      </div>

                      <div className="banner-copy">
                        <div className="eyebrow">SELECTED INSTANCE</div>
                        <h2>{selected.name}</h2>
                        <p>
                          Minecraft {selected.version}
                          <span className="dot-separator">·</span>
                          {selected.loader}{" "}
                          {selected.loaderVersion !== "—"
                            ? selected.loaderVersion
                            : ""}
                        </p>
                      </div>

                      <div className="banner-decoration" aria-hidden="true">
                        <div className="banner-orbit orbit-one" />
                        <div className="banner-orbit orbit-two" />
                        <div className="banner-planet" />
                      </div>
                    </section>

                    <div className="section-title">
                      <div>
                        <h3>Instance overview</h3>
                        <p>Your installations, each with its own files.</p>
                      </div>
                    </div>

                    <div className="overview-grid">
                      <div className="info-card">
                        <div className="info-icon">
                          <Gamepad2 size={18} />
                        </div>
                        <span className="info-label">GAME VERSION</span>
                        <strong>{selected.version}</strong>
                        <small>{selected.status}</small>
                      </div>

                      <div className="info-card">
                        <div className="info-icon">
                          <Cpu size={18} />
                        </div>
                        <span className="info-label">MOD LOADER</span>
                        <strong>{selected.loader}</strong>
                        <small>{selected.loaderVersion}</small>
                      </div>

                      <div className="info-card">
                        <div className="info-icon">
                          <Layers3 size={18} />
                        </div>
                        <span className="info-label">INSTALLED MODS</span>
                        <strong>{selected.mods}</strong>
                        <small>In this instance</small>
                      </div>
                    </div>


                    <div className="bottom-play">
                      <div className="play-instance">
                        <div className="mini-instance">
                          {selected.icon?.startsWith("data:image/") ? (
                            <img
                              src={selected.icon}
                              alt=""
                              className="custom-instance-icon"
                            />
                          ) : (
                            selected.icon || "M"
                          )}
                        </div>
                        <div>
                          <strong>{selected.name}</strong>
                          <small>
                            {selected.version} · {selected.loader}
                          </small>
                        </div>
                      </div>

                      <div className="play-actions">
                        <button
                          className="play-button"
                          onClick={() => void launchSelectedInstance()}
                          disabled={busy}
                        >
                          <Gamepad2 size={18} />
                          {busy ? (isSelectedInstalled ? "Launching..." : "Preparing...") : "Launch Instance"}
                        </button>

                        <div className="menu-wrap">
                          <button
                            className={`more-button ${menuOpen ? "active" : ""}`}
                            onClick={() => setMenuOpen(!menuOpen)}
                            title="Instance options"
                          >
                            <Ellipsis size={20} />
                          </button>

                          {menuOpen && (
                            <div className="dropdown">
                              <div className="dropdown-heading">
                                SWITCH INSTANCE
                              </div>

                              {instances.map((instance) => (
                                <button
                                  className="dropdown-sub"
                                  key={instance.id}
                                  onClick={() => selectInstance(instance.id)}
                                >
                                  {instance.name}
                                  {selectedId === instance.id ? " ✓" : ""}
                                </button>
                              ))}

                              <div className="dropdown-divider" />

                              <button onClick={() => { setMenuOpen(false); setSharingMode("send"); }}>
                                <Share2 size={16} /> Share instance
                              </button>

                              <button onClick={() => { setMenuOpen(false); setSharingMode("receive"); }}>
                                <Download size={16} /> Receive instance
                              </button>

                              <div className="dropdown-divider" />

                              <button onClick={openInstaller}>
                                <Download size={16} /> Install Minecraft version
                              </button>

                              <button onClick={renameSelectedInstance}>
                                <Pencil size={16} /> Rename instance
                              </button>

                              <button onClick={openFolder}>
                                <FolderOpen size={16} /> Open instance folder
                              </button>

                              <button onClick={deleteSelectedInstance}>
                                <Trash2 size={16} /> Delete instance
                              </button>
                            </div>
                          )}
                        </div>
                      </div>
                    </div>
                  </>
                )}
              </>
            )}

            {(["Mods", "Modpacks", "Texture Packs", "Shaders"].includes(page)) && (
              <div className="mods-page" ref={projectsPageRef}>
                <div className="page-heading">
                  <div>
                    <div className="eyebrow">
                      {isModpacksPage ? "DISCOVER MODPACKS" : isTexturePacksPage ? "DISCOVER TEXTURE PACKS" : isShadersPage ? "DISCOVER SHADERS" : "DISCOVER & MANAGE"}
                    </div>
                    <h1>{isModpacksPage ? "Modpacks" : isTexturePacksPage ? "Texture Packs" : isShadersPage ? "Shaders" : "Mods"}</h1>
                    <p>
                      {isModpacksPage ? "Discover Minecraft modpacks on Modrinth." : isTexturePacksPage ? "Browse and install Minecraft texture packs from Modrinth." : isShadersPage ? "Browse and install compatible Minecraft shaders from Modrinth." : "Find mods on Modrinth for your selected instance."}
                    </p>
                  </div>
                </div>

                {!selected ? (
                  <div className="info-card">
                    Create an instance before browsing.
                  </div>
                ) : (
                  <>
                    {isModsPage && (
                      <div className="mods-subnav" role="tablist" aria-label="Mods sections">
                        <button
                          type="button"
                          role="tab"
                          aria-selected={modsTab === "discover"}
                          className={`mods-subnav-button ${modsTab === "discover" ? "active" : ""}`}
                          onClick={() => setModsTab("discover")}
                        >
                          <Search size={15} /> Discover
                        </button>
                        <button
                          type="button"
                          role="tab"
                          aria-selected={modsTab === "installed"}
                          className={`mods-subnav-button ${modsTab === "installed" ? "active" : ""}`}
                          onClick={() => {
                            setModsTab("installed");
                            void refreshInstalledMods(selected.id);
                          }}
                        >
                          <Library size={15} /> Installed Mods <span className="mods-count">{installedMods.length}</span>
                        </button>
                      </div>
                    )}

                    {(!isModsPage || modsTab === "discover") && (
                      <>
<div className="info-card" style={{ marginBottom: 18 }}>
                      <div className="eyebrow">
                        {isModpacksPage ? "MODPACK DISCOVERY" : isTexturePacksPage ? "TEXTURE PACK DISCOVERY" : isShadersPage ? "SHADER DISCOVERY" : "SEARCHING FOR"}
                      </div>

                      <strong>{selected.name}</strong>

                      <small>
                        Instance: Minecraft {selected.version} · {selected.loader}
                      </small>

                      <div
                        style={{
                          display: "flex",
                          gap: 10,
                          marginTop: 14
                        }}
                      >

                        <input
                          value={modQuery}
                          onChange={(event) => setModQuery(event.target.value)}
                          onKeyDown={(event) => {
                            if (event.key === "Enter") {
                              setModPage(0);
                              void searchProjects(0);
                            }
                          }}
                          placeholder="Search"
                          style={{ flex: 1, minWidth: 0 }}
                        />

                        <button
                          type="button"
                          className="primary-button restored-action-button"
                          onClick={() => {
                            setModPage(0);
                            void searchProjects(0);
                          }}
                          disabled={modLoading}
                          aria-label={modLoading ? "Searching Modrinth" : "Search Modrinth"}
                        >
                          <Search size={16} />
                          {modLoading ? "Searching..." : "Search"}
                        </button>
                      </div>

                      <div
                        style={{
                          display: "flex",
                          gap: 10,
                          marginTop: 12,
                          flexWrap: "wrap"
                        }}
                      >
                        {isModsPage && (
                          <select
                            value={modCategory}
                            onChange={(event) => {
                              setModCategory(event.target.value);
                              setModPage(0);
                            }}
                            style={{ flex: 1, minWidth: 140 }}
                          >
                            <option value="">All genres</option>
                            {MOD_CATEGORIES.map(([value, label]) => (
                              <option key={value} value={value}>
                                {label}
                              </option>
                            ))}
                          </select>
                        )}

                        <select
                          value={modVersion || selected.version}
                          onChange={(event) => {
                            setModVersion(event.target.value);
                            setModPage(0);
                          }}
                          style={{ flex: 1, minWidth: 140 }}
                        >
                          <option value={selected.version}>
                            Instance version ({selected.version})
                          </option>

                          {versions
                            .filter(
                              (version) =>
                                version.versionType === "release" &&
                                version.id !== selected.version
                            )
                            .map((version) => (
                              <option key={version.id} value={version.id}>
                                Minecraft {version.id}
                              </option>
                            ))}
                        </select>

                        <select
                          value={modPageSize}
                          onChange={(event) => {
                            setModPageSize(Number(event.target.value));
                            setModPage(0);
                          }}
                          style={{ flex: 1, minWidth: 140 }}
                          aria-label="Items per page"
                        >
                          <option value={10}>10 per page</option>
                          <option value={20}>20 per page</option>
                          <option value={30}>30 per page</option>
                        </select>
                      </div>

                      {!isModpacksPage && modVersion !== selected.version && (
                        <small style={{ display: "block", marginTop: 10 }}>
                          Browsing another version. Installation is disabled until you return to your instance version.
                        </small>
                      )}
                    </div>

                    {modLoading && <p>Loading results...</p>}
                    {modError && <p>{modError}</p>}

                    <div className="mod-results">
                      {modResults.map((mod) => (
                        <div
                          className="info-card"
                          key={mod.projectId}
                          style={{ marginBottom: 12 }}
                        >
                          <div
                            style={{
                              display: "flex",
                              gap: 14,
                              alignItems: "center"
                            }}
                          >
                            {mod.iconUrl ? (
                              <img
                                src={mod.iconUrl}
                                alt=""
                                style={{
                                  width: 48,
                                  height: 48,
                                  borderRadius: 10,
                                  objectFit: "cover"
                                }}
                              />
                            ) : (
                              <div className="info-icon">
                                {isModpacksPage ? <Package size={20} /> : isTexturePacksPage ? <Image size={20} /> : isShadersPage ? <Sparkles size={20} /> : <Library size={20} />}
                              </div>
                            )}

                            <div style={{ minWidth: 0, flex: 1 }}>
                              <button
        type="button"
        className="project-title-button"
        onClick={() => openProjectDownload(mod)}
        aria-label={`Open details for ${mod.title}`}
      >{mod.title}</button>
                              <small style={{ display: "block" }}>
                                by {mod.author}
                              </small>
                            </div>
                          </div>

                          <small>
                            {mod.downloads.toLocaleString()} downloads
                          </small>

                          <div style={{ marginTop: 12, display: "flex", gap: 8, flexWrap: "wrap" }}>
                            <button className="secondary-button" onClick={() => openProjectDownload(mod)}>
                              <Download size={15} /> Download
                            </button>
                          </div>
                        </div>
                      ))}
                    </div>

                    {modTotalHits > modPageSize && (
                      <div
                        style={{
                          display: "flex",
                          alignItems: "center",
                          justifyContent: "center",
                          gap: 16,
                          margin: "20px 0"
                        }}
                      >
                        <button
                          className="icon-button"
                          disabled={modPage === 0 || modLoading}
                          onClick={() =>
                            setModPage((current) => Math.max(0, current - 1))
                          }
                          title="Previous page"
                        >
                          <ChevronLeft size={18} />
                        </button>

                        <span>
                          Page {modPage + 1} of {totalModPages}
                        </span>

                        <button
                          className="icon-button"
                          disabled={
                            (modPage + 1) * modPageSize >= modTotalHits ||
                            modLoading
                          }
                          onClick={() =>
                            setModPage((current) => current + 1)
                          }
                          title="Next page"
                        >
                          <ChevronRight size={18} />
                        </button>
                      </div>
                    )}

                    </>
                    )}

                    {isModsPage && modsTab === "installed" && (
                      <section className="installed-mods-menu" role="tabpanel">
                        <div className="section-title">
                          <div>
                            <h3>Installed Mods</h3>
                            <p>{installedMods.length} mod JAR file(s) in {selected.name}.</p>
                          </div>
                        </div>

                        {installedMods.length > 0 ? (
                          <div className="installed-mods-list">
                            {installedMods.map((filename) => (
                              <div key={filename} className="installed-mod-entry">
                                <div className="installed-mod-file">
                                  <div className="installed-mod-icon"><Library size={17} /></div>
                                  <span title={filename}>{filename}</span>
                                </div>
                                <button
                                  className="icon-button"
                                  title={`Remove ${filename}`}
                                  aria-label={`Remove ${filename}`}
                                  disabled={busy}
                                  onClick={() => removeMod(filename)}
                                >
                                  <Trash2 size={16} />
                                </button>
                              </div>
                            ))}
                          </div>
                        ) : (
                          <div className="info-card installed-mods-empty">
                            <Library size={28} />
                            <strong>No installed mods yet</strong>
                            <p>Download mods from the Discover tab and they'll appear here.</p>
                            <button className="secondary-button" onClick={() => setModsTab("discover")}>
                              <Search size={15} /> Browse mods
                            </button>
                          </div>
                        )}
                      </section>
                    )}

                    {isModpacksPage && (
                      <>
                        <div className="section-title" style={{ marginTop: 24 }}>
                          <div>
                            <h3>Installed modpack</h3>
                            <p>{installedModpack?.name ?? "No Modrinth modpack is installed in this instance."}</p>
                          </div>
                        </div>
                        {installedModpack && (
                          <div className="info-card">
                            <strong>{installedModpack.name}</strong>
                            <p>Minecraft {installedModpack.minecraftVersion} · {installedModpack.loader} {installedModpack.loaderVersion}</p>
                            <strong style={{ display: "block", marginTop: 12 }}>Mods included in this modpack</strong>
                            {installedPackMods.length === 0 ? <p>No mod JARs were listed in this modpack.</p> : installedPackMods.map((filename) => <div key={filename} className="installed-mod-row">{filename}</div>)}
                          </div>
                        )}
                      </>
                    )}
                  </>
                )}
              </div>
            )}


            {page === "Console" && (
              <div className="console-page">
                <div className="page-heading">
                  <div>
                    <div className="eyebrow">LIVE OUTPUT</div>
                    <h1>Console</h1>
                    <p>
                      Minecraft process output for {selected?.name ?? "all instances"}.
                    </p>
                  </div>
                  <button
                    type="button"
                    className="secondary-button restored-action-button"
                    onClick={() => { consoleEntriesRef.current = []; setConsoleEntries([]); }}
                  >
                    <Trash size={15} /> Clear console
                  </button>
                </div>

                <div className="console-window" role="log" aria-live="polite">
                  {consoleEntries
                    .filter((entry) => !selectedId || entry.instanceId === selectedId)
                    .map((entry, index) => (
                      <div
                        className={`console-line console-${entry.stream}`}
                        key={`${entry.receivedAt}-${index}`}
                      >
                        <span className="console-stream">{entry.stream}</span>
                        <span>{entry.line}</span>
                      </div>
                    ))}
                  {consoleEntries.filter((entry) => !selectedId || entry.instanceId === selectedId).length === 0 && (
                    <div className="console-empty">
                      No output yet. Launch an installed instance to see its logs here.
                    </div>
                  )}
                </div>
              </div>
            )}

            {page === "Updates" && (
              <div className="utility-page-scroll" style={{ height: "100%", minHeight: 0, overflowY: "auto", overflowX: "hidden" }}>
                <UpdatesPage onProgress={(value) => setProgress(value)} onNotice={setNotice} />
              </div>
            )}

            {page === "Settings" && (
              <div
                className="utility-page-scroll"
                style={{
                  height: "100%",
                  minHeight: 0,
                  overflowY: "auto",
                  overflowX: "hidden"
                }}
              >
                <SettingsPage selected={selected} onNotice={setNotice} onDiscordPresenceChange={setDiscordEnabled} />
              </div>
            )}

            {page === "Accounts" && (
              <div
                className="utility-page-scroll"
                style={{
                  height: "100%",
                  minHeight: 0,
                  overflowY: "auto",
                  overflowX: "hidden"
                }}
              >
                <AccountsPage onNotice={setNotice} />
              </div>
            )}
          </div>
        </main>
      </div>

      {progress && (
        <div className="install-progress-backdrop" role="status" aria-live="polite" aria-label="Installation progress">
          <div className="install-progress-card">
            <div className="install-progress-symbol"><Download size={23} /></div>
            <span className="eyebrow">M0Z4 INSTALLER</span>
            <h2>Setting up your instance</h2>
            <p className="install-progress-message">{progress.message}</p>
            <div
              className="install-progress-track"
              role="progressbar"
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={Math.max(0, Math.min(100, Math.round((progress.completed / Math.max(progress.total, 1)) * 100)))}
            >
              <div
                className="install-progress-fill"
                style={{ width: `${Math.max(0, Math.min(100, (progress.completed / Math.max(progress.total, 1)) * 100))}%` }}
              />
            </div>
            <div className="install-progress-meta">
              <span>{progress.completed.toLocaleString()} / {progress.total.toLocaleString()} steps</span>
              <strong>{Math.round((progress.completed / Math.max(progress.total, 1)) * 100)}%</strong>
            </div>
            <small className="install-progress-hint">Please keep M0Z4 open while installation is running.</small>
          </div>
        </div>
      )}

      {notice && (
        <div className="toast">
          <span>{notice}</span>
          <button onClick={() => setNotice("")}>
            <X size={15} />
          </button>
        </div>
      )}

      {selectedProject && (
        <div className="modal-backdrop" onClick={() => !busy && setSelectedProject(null)}>
          <div className="modal project-download-modal" onClick={(event) => event.stopPropagation()}>
            <div className="modal-heading">
              <div><span className="eyebrow">{page === "Modpacks" ? "MODRINTH MODPACK" : "MODRINTH PROJECT"}</span><h3>{selectedProject.title}</h3><small>by {selectedProject.author} · {selectedProject.downloads.toLocaleString()} downloads</small></div>
              <button className="icon-button" onClick={() => setSelectedProject(null)} disabled={busy}><X size={18} /></button>
            </div>
            {versionsLoading ? <p>Loading project details and compatible versions...</p> : (
              <>
                {projectDetails?.description && (
                  <p className="project-description">{projectDetails.description}</p>
                )}
                {Array.isArray(projectDetails?.gallery) && projectDetails.gallery.length > 0 && (
                  <div className="project-gallery" aria-label="Available project images">
                    {projectDetails.gallery.slice(0, 8).map((image: any, index: number) => (
                      <a key={image.url ?? index} href={image.url} target="_blank" rel="noreferrer" title={image.title ?? `Project image ${index + 1}`}>
                        <img src={image.url} alt={image.title ?? `${selectedProject.title} screenshot ${index + 1}`} loading="lazy" />
                      </a>
                    ))}
                  </div>
                )}
                <label>
                  {page === "Modpacks" ? "Modpack version" : page === "Texture Packs" ? "Texture pack version" : page === "Shaders" ? "Shader version" : "Mod version"}
                  <select value={selectedProjectVersion} onChange={(event) => setSelectedProjectVersion(event.target.value)}>
                    {projectVersions.map((version: any) => (
                      <option key={version.id} value={version.id}>
                        {version.name ?? version.displayName ?? version.versionNumber ?? version.fileName ?? version.id}
                        {version.versionNumber ? ` (${version.versionNumber})` : ""}
                      </option>
                    ))}
                  </select>
                </label>
                {projectVersions.length === 0 && <div className="modal-note">No compatible versions were found for the selected Minecraft version.</div>}
                <button className="primary-button full" onClick={installSelectedProject} disabled={!selectedProjectVersion || busy || !selected}>
                  <Download size={16} /> {busy ? "Installing..." : "Download"}
                </button>
              </>
            )}
          </div>
        </div>
      )}

      {modal && (
        <div
          className="modal-backdrop"
          onClick={() => !busy && setModal(null)}
        >
          <div
            className="modal"
            onClick={(event) => event.stopPropagation()}
          >
            <div className="modal-heading">
              <div>
                <span className="eyebrow">
                  {modal === "new" ? "LIBRARY" : "MINECRAFT INSTALLER"}
                </span>

                <h3>
                  {modal === "new"
                    ? "Create an instance"
                    : "Install Minecraft"}
                </h3>
              </div>

              <button
                className="icon-button"
                onClick={() => setModal(null)}
                disabled={busy}
              >
                <X size={18} />
              </button>
            </div>

            {modal === "new" ? (
              <>
                <label>
                  Instance name
                  <input
                    value={newName}
                    onChange={(event) => setNewName(event.target.value)}
                    placeholder="My new instance"
                  />
                </label>

                <label>
                  Version type
                  <select
                    value={versionFilter}
                    onChange={(event) => {
                      const nextFilter = event.target.value;
                      const matching = versions.filter((version) => nextFilter === "all" || version.versionType === nextFilter);
                      setVersionFilter(nextFilter);
                      if (matching.length && !matching.some((version) => version.id === newVersion)) setNewVersion(matching[0].id);
                    }}
                  >
                    <option value="release">Releases</option>
                    <option value="snapshot">Snapshots</option>
                    <option value="old_beta">Beta</option>
                    <option value="old_alpha">Alpha</option>
                    <option value="all">All versions</option>
                  </select>
                </label>
                <label>
                  Minecraft version
                  <select value={newVersion} onChange={(event) => setNewVersion(event.target.value)}>
                    {filteredVersions.length > 0 ? filteredVersions.map((version) => (
                      <option key={version.id} value={version.id}>{version.id} · {version.versionType}</option>
                    )) : <option value={newVersion}>{newVersion} · No versions in this category</option>}
                  </select>
                </label>

                <label>
                  Mod loader
                  <select
                    value={newLoader}
                    onChange={(event) => setNewLoader(event.target.value)}
                  >
                    <option value="Vanilla">Vanilla</option>
                    <option value="Fabric">Fabric</option>
                    <option value="Quilt">Quilt</option>
                    <option value="Forge">Forge</option>
                    <option value="NeoForge">NeoForge</option>
                  </select>
                </label>

                {(newLoader === "Forge" || newLoader === "NeoForge") && (
                  <label>
                    Loader version (optional)
                    <input
                      value={newLoaderVersion}
                      onChange={(event) => setNewLoaderVersion(event.target.value)}
                      placeholder="Latest compatible version if blank"
                    />
                  </label>
                )}

                <div className="modal-note">
                  {newLoader === "Vanilla"
                    ? "Creates an isolated Minecraft instance. Install the game after creation."
                    : `M0Z4 will install Minecraft ${newVersion} and ${newLoader} automatically before the instance is ready.`}
                </div>

                <button
                  className="primary-button full"
                  onClick={createInstance}
                  disabled={!newName.trim() || busy}
                >
                  <Plus size={16} />
                  {busy ? (newLoader === "Vanilla" ? "Creating..." : "Installing...") : "Create instance"}
                </button>
              </>
            ) : (
              <>
                <label>
                  Version type
                  <select
                    value={versionFilter}
                    onChange={(event) => {
                      const nextFilter = event.target.value;
                      const matching = versions.filter((version) => nextFilter === "all" || version.versionType === nextFilter);
                      setVersionFilter(nextFilter);
                      if (matching.length && !matching.some((version) => version.id === installVersion)) setInstallVersion(matching[0].id);
                    }}
                  >
                    <option value="release">Releases</option>
                    <option value="snapshot">Snapshots</option>
                    <option value="old_beta">Beta</option>
                    <option value="old_alpha">Alpha</option>
                    <option value="all">All versions</option>
                  </select>
                </label>

                <label>
                  Minecraft version
                  <select
                    value={installVersion}
                    onChange={(event) => setInstallVersion(event.target.value)}
                  >
                    {filteredVersions.map((version) => (
                      <option key={version.id} value={version.id}>
                        {version.id}
                        {version.id === latestRelease
                          ? " (Latest release)"
                          : ""}
                      </option>
                    ))}
                  </select>
                </label>

                <div className="modal-note">
                  Installing into <strong>{selected?.name}</strong>. Existing
                  worlds and mods will be kept.
                </div>

                <button
                  className="primary-button full"
                  onClick={installMinecraft}
                  disabled={busy || !installVersion || !selected}
                >
                  <Download size={16} />
                  {busy ? "Installing..." : "Install Minecraft"}
                </button>
              </>
            )}
          </div>
        </div>
      )}

      {sharingMode && selected && (
        <InstanceSharing
          mode={sharingMode}
          instanceId={selected.id}
          onClose={() => setSharingMode(null)}
          onNotice={setNotice}
          onImported={async (id) => {
            await invoke("select_instance", { id });
            await refreshInstances();
            setPage("Home");
          }}
        />
      )}
    </div>
  );
}
