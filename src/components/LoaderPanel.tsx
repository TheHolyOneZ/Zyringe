import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import {
  CheckCircle2,
  AlertTriangle,
  Plus,
  Trash2,
  FolderOpen,
  ExternalLink,
  Copy,
  Download,
  Loader2,
} from "lucide-react";
import type { MonoProcess, LoaderKind, LoaderStatus, LoaderAsset } from "../types";
import InfoTip from "./InfoTip";

interface Props {
  game: MonoProcess;
  onToast: (kind: "ok" | "err" | "info", msg: string) => void;
}

const LOADERS: Record<LoaderKind, { releases: string; blurb: string; pluginsNote: string }> = {
  MelonLoader: {
    releases: "https://github.com/LavaGang/MelonLoader/releases",
    blurb: "One unified loader. Common on Windows/Proton; native-Linux support is newer.",
    pluginsNote: "the game's Mods/ folder",
  },
  BepInEx: {
    releases: "https://github.com/BepInEx/BepInEx/releases",
    blurb: "Huge plugin ecosystem, best native-Linux support. Use a BepInEx 6 IL2CPP build (match Proton vs Linux to your game).",
    pluginsNote: "BepInEx/plugins/",
  },
};

const dirname = (p: string) => p.split("/").slice(0, -1).join("/") || "/";
const mb = (n: number) => `${(n / 1048576).toFixed(1)} MB`;

export default function LoaderPanel({ game, onToast }: Props) {
  const [loader, setLoader] = useState<LoaderKind>("BepInEx");
  const [status, setStatus] = useState<LoaderStatus | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [confirmRemove, setConfirmRemove] = useState(false);

  const [showBuilds, setShowBuilds] = useState(false);
  const [assets, setAssets] = useState<LoaderAsset[] | null>(null);
  const [filter, setFilter] = useState(game.proton ? "" : "linux");
  const [steamUp, setSteamUp] = useState(false);


  const gameDir =
    game.game_dir ??
    (game.data_dir ? dirname(game.data_dir) : game.exe_path ? dirname(game.exe_path) : null);
  const meta = LOADERS[loader];

  const refresh = useCallback(async () => {
    if (!gameDir) return;
    try {
      setStatus(await invoke<LoaderStatus>("loader_status", { gameDir, loader }));
    } catch (e) {
      onToast("err", String(e));
    }
  }, [gameDir, loader, onToast]);

  useEffect(() => {
    refresh();
    setShowBuilds(false);
    setAssets(null);
    setConfirmRemove(false);
  }, [refresh]);

  const openReleases = () => invoke("reveal_path", { path: meta.releases }).catch(() => {});
  const revealFolder = () =>
    status && invoke("reveal_path", { path: status.plugins_dir }).catch(() => {});

  const browseBuilds = async () => {
    setShowBuilds(true);
    if (assets) return;
    setBusy("Fetching builds…");
    try {
      setAssets(await invoke<LoaderAsset[]>("loader_fetch_assets", { loader }));
    } catch (e) {
      onToast("err", String(e));
      setShowBuilds(false);
    } finally {
      setBusy(null);
    }
  };

  const installUrl = async (a: LoaderAsset) => {
    if (!gameDir) return;
    setBusy(`Installing ${a.name}…`);
    try {
      const msg = await invoke<string>("loader_install_url", { gameDir, url: a.url, loader });
      onToast("ok", `${loader} installed — ${msg}`);
      setShowBuilds(false);
      refresh();
    } catch (e) {
      onToast("err", String(e));
    } finally {
      setBusy(null);
    }
  };

  const installZip = async () => {
    if (!gameDir) return;
    const zip = await open({ multiple: false, filters: [{ name: "Zip", extensions: ["zip"] }] });
    if (typeof zip !== "string") return;
    setBusy("Installing…");
    try {
      const msg = await invoke<string>("loader_install_zip", { gameDir, zipPath: zip, loader });
      onToast("ok", `${loader} installed — ${msg}`);
      refresh();
    } catch (e) {
      onToast("err", String(e));
    } finally {
      setBusy(null);
    }
  };

  const removeLoader = async () => {
    if (!gameDir) return;
    setBusy(`Removing ${loader}…`);
    try {
      const msg = await invoke<string>("loader_remove", { gameDir, loader });
      onToast("ok", msg);
      setConfirmRemove(false);
      refresh();
    } catch (e) {
      onToast("err", String(e));
    } finally {
      setBusy(null);
    }
  };

  const addPlugin = async () => {
    if (!status) return;
    const dll = await open({
      multiple: false,
      filters: [{ name: ".NET Assembly", extensions: ["dll"] }],
    });
    if (typeof dll !== "string") return;
    try {
      const name = await invoke<string>("loader_add_plugin", {
        pluginsDir: status.plugins_dir,
        dllPath: dll,
      });
      onToast("info", `Added ${name}`);
      refresh();
    } catch (e) {
      onToast("err", String(e));
    }
  };

  const removePlugin = async (name: string) => {
    if (!status) return;
    try {
      await invoke("loader_remove_plugin", { pluginsDir: status.plugins_dir, name });
      refresh();
    } catch (e) {
      onToast("err", String(e));
    }
  };

  const copy = (text: string, label: string) =>
    navigator.clipboard.writeText(text).then(
      () => onToast("info", `${label} copied`),
      () => onToast("err", "Copy failed")
    );


  const checkSteam = useCallback(async () => {
    if (!game.app_id) return;
    try {
      setSteamUp(await invoke<boolean>("steam_running"));
    } catch {

    }
  }, [game.app_id]);
  useEffect(() => {
    checkSteam();
  }, [checkSteam, status?.installed]);


  const setupSteam = async (thenLaunch: boolean) => {
    if (!launchOption || !game.app_id) return;
    setBusy("Checking Steam…");
    try {
      const up = await invoke<boolean>("steam_running");
      setSteamUp(up);
      if (up) {
        onToast("err", "Steam is running — fully close it (Steam → Exit), then try again.");
        return;
      }
      setBusy("Setting the launch option in Steam…");
      await invoke<string>("steam_set_launch_option", {
        appId: game.app_id,
        option: launchOption,
      });
      if (thenLaunch) {
        setBusy("Launching via Steam…");
        await invoke<string>("steam_run", { appId: game.app_id });
        onToast("ok", `Set up in Steam & launching ${game.name}. Your mods load automatically.`);
      } else {
        onToast(
          "ok",
          `Launch option set in Steam. Just launch ${game.name} normally — mods load automatically.`
        );
      }
    } catch (e) {
      onToast("err", String(e));
    } finally {
      setBusy(null);
    }
  };


  const launchOption = game.proton
    ? loader === "BepInEx"
      ? `WINEDLLOVERRIDES="winhttp=n,b" %command%`
      : `WINEDLLOVERRIDES="version=n,b" %command%`
    : status?.run_script
    ? `"${status.run_script}" %command%`
    : null;

  const shownAssets = (assets ?? []).filter((a) => {
    const n = a.name.toLowerCase();


    if (n.includes("mac") || n.includes("osx")) return false;
    if (game.proton && n.includes("linux")) return false;


    if (loader === "BepInEx" && !n.includes("il2cpp")) return false;
    return filter.trim() ? n.includes(filter.trim().toLowerCase()) : true;
  });

  return (
    <div className="form loader-panel">
      <div className="beta-banner info">
        <span>
          IL2CPP games have no Mono runtime to inject into, so Zyringe installs &amp; manages a
          <strong> mod loader</strong> (MelonLoader / BepInEx) — the game is launched by Steam and
          the loader hooks at startup. Verified working with both loaders.
        </span>
      </div>


      <div className="field">
        <div className="field-head">
          <label>
            Mod loader{" "}
            <span className="plat-tag">{game.proton ? "Proton (Windows)" : "native Linux"}</span>
            <InfoTip text="IL2CPP games compile C# to native code — there's no Mono runtime to inject into. A loader (MelonLoader/BepInEx) hosts a managed runtime and loads your plugins when the game starts. Proton games run the Windows loader via Wine; native games use the Linux build." />
          </label>
          <button className="text-btn" onClick={openReleases}>
            <ExternalLink size={13} /> Get {loader}
          </button>
        </div>
        <div className="mode-toggle">
          {(Object.keys(LOADERS) as LoaderKind[]).map((k) => (
            <button key={k} className={loader === k ? "active" : ""} onClick={() => setLoader(k)}>
              {k}
            </button>
          ))}
        </div>
        <p className="loader-blurb">{meta.blurb}</p>
        <div className="loader-hint">
          <InfoTip text="Loaders aren't interchangeable — a BepInEx mod won't run under MelonLoader. Use whatever the mod's page says. Also match the build to your game: a Proton game needs the Windows loader build, a native game the Linux build." />
          <span>Use whichever your mod's page says. Match the build to {game.proton ? "Proton / Windows" : "native Linux"}.</span>
        </div>
      </div>

      {!gameDir && (
        <div className="loader-status err">
          <AlertTriangle size={15} /> Couldn't determine this game's folder.
        </div>
      )}

      {/* Install status */}
      {gameDir && status?.installed && (
        <div className="loader-status ok">
          <CheckCircle2 size={15} />
          <span>{loader} is installed.</span>
          <div className="status-actions">
            {confirmRemove ? (
              <>
                <span className="confirm-q">Remove {loader}?</span>
                <button className="text-btn danger" onClick={removeLoader} disabled={!!busy}>
                  Yes, remove
                </button>
                <button className="text-btn" onClick={() => setConfirmRemove(false)}>
                  Cancel
                </button>
              </>
            ) : (
              <button className="text-btn danger" onClick={() => setConfirmRemove(true)}>
                <Trash2 size={13} /> Remove {loader}
              </button>
            )}
          </div>
        </div>
      )}

      {gameDir && status?.installed && game.proton && !status.windows_proxy && (
        <div className="loader-status err">
          <AlertTriangle size={15} />
          <span>
            This looks like a <strong>Linux</strong> build, but {game.name} runs through Proton and
            needs a <strong>Windows</strong> build — a Linux loader crashes the game on launch.
            Remove it above, then install a <strong>win-x64</strong> build.
          </span>
        </div>
      )}

      {gameDir && status && !status.installed && (
        <div className="install-card">
          <div className="install-head">{loader} isn't set up in this game yet</div>
          <p className="install-body">
            Install a {game.proton ? "Windows" : "Linux"} {loader} build into the game folder, or
            drop in a zip you already downloaded.
          </p>
          <div className="install-actions">
            <button className="secondary-btn" onClick={browseBuilds} disabled={!!busy}>
              <Download size={14} /> Browse &amp; install a build
            </button>
            <button className="secondary-btn" onClick={installZip} disabled={!!busy}>
              <FolderOpen size={14} /> Install from zip…
            </button>
          </div>

          {showBuilds && (
            <div className="builds">
              <div className="builds-filter">
                <input
                  type="text"
                  placeholder="Filter builds — e.g. il2cpp, win, x64, linux"
                  value={filter}
                  onChange={(e) => setFilter(e.target.value)}
                />
                <InfoTip
                  text={
                    game.proton
                      ? "Proton game → pick a Windows IL2CPP build (e.g. 'BepInEx-Unity.IL2CPP-win-x64'). NOT a linux build."
                      : "Native Linux game → pick a Linux IL2CPP build (e.g. 'BepInEx-Unity.IL2CPP-linux-x64')."
                  }
                />
              </div>
              {!assets ? (
                <div className="args-empty">Loading releases…</div>
              ) : shownAssets.length === 0 ? (
                <div className="args-empty">No assets match “{filter}”. Clear the filter to see all.</div>
              ) : (
                <div className="builds-list">
                  {shownAssets.slice(0, 40).map((a) => (
                    <button
                      key={a.url}
                      className="build-row"
                      disabled={!!busy}
                      onClick={() => installUrl(a)}
                    >
                      <div className="build-main">
                        <span className="build-name">{a.name}</span>
                        <span className="build-meta">
                          {a.tag} · {mb(a.size)}
                          {a.prerelease && <span className="build-pre">pre-release</span>}
                        </span>
                      </div>
                      <span className="build-install">Install</span>
                    </button>
                  ))}
                </div>
              )}
            </div>
          )}
        </div>
      )}


      {status?.installed && (
        <div className="field">
          <div className="field-head">
            <label>
              Plugins
              <InfoTip text={`Your mod DLLs live in ${meta.pluginsNote}. "Add" copies a .dll there; the loader picks it up next launch. Some mods ship a folder — use "Open folder" and copy them in.`} />
            </label>
            <div className="loader-actions">
              <button className="text-btn" onClick={revealFolder} title="Open the folder">
                <FolderOpen size={13} /> Folder
              </button>
              <button className="text-btn" onClick={addPlugin}>
                <Plus size={13} /> Add plugin
              </button>
            </div>
          </div>
          {status.plugins.length === 0 ? (
            <div className="args-empty">No plugins yet — Add one, or open the folder and copy them in.</div>
          ) : (
            <div className="plugin-list">
              {status.plugins.map((p) => (
                <div className="plugin-row" key={p}>
                  <span className="plugin-name">{p}</span>
                  <button className="icon-btn" title="Remove" onClick={() => removePlugin(p)}>
                    <Trash2 size={13} />
                  </button>
                </div>
              ))}
            </div>
          )}
        </div>
      )}


      {status?.installed && (
        <div className="field">
          <div className="field-head">
            <label>
              How to launch with mods
              <InfoTip text="Loaders hook when the game starts, via a proxy the game loads at launch — nothing injects at runtime. So you set a launch option once in Steam, then just play normally and mods load. (This is how BepInEx/MelonLoader are designed to work; it's why r2modman etc. do the same.)" />
            </label>
          </div>
          <div className="launch-block">
          {launchOption && game.app_id ? (
            <>
              <div className="loader-hint">
                <span>
                  Zyringe sets the loader up in Steam for you, then launches the game — nothing to
                  paste. Steam must be <strong>fully closed</strong> while its config is edited.
                </span>
              </div>
              {steamUp && (
                <div className="loader-status warn">
                  <AlertTriangle size={14} />
                  <span>
                    Steam is running — close it fully (Steam → Exit) first, then click below.
                  </span>
                  <button className="text-btn" onClick={checkSteam}>
                    Re-check
                  </button>
                </div>
              )}
              <div className="install-actions">
                <button className="inject-btn" onClick={() => setupSteam(true)} disabled={!!busy}>
                  Set up &amp; launch via Steam
                </button>
                <button
                  className="secondary-btn"
                  onClick={() => setupSteam(false)}
                  disabled={!!busy}
                >
                  Just set the option
                </button>
              </div>
              <div className="loader-hint">
                <span>
                  {game.proton
                    ? `Proton (Windows) game — Zyringe sets WINEDLLOVERRIDES so Wine loads the loader's proxy DLL.${
                        loader === "MelonLoader"
                          ? " MelonLoader on Proton can need extra steps; check its wiki if mods don't load."
                          : ""
                      }`
                    : "Native Linux — the launch runs the game through the loader's wrapper."}
                </span>
              </div>

              <details className="launch-alt">
                <summary>Or set it manually</summary>
                <div className="launch-step">
                  <span className="step-n">1</span>
                  <span>
                    In Steam: right-click <strong>{game.name}</strong> → <strong>Properties</strong>{" "}
                    → <strong>Launch Options</strong>, and paste:
                  </span>
                </div>
                <div className="launch-opt">
                  <code>{launchOption}</code>
                  <button
                    className="icon-btn"
                    title="Copy"
                    onClick={() => copy(launchOption, "Launch option")}
                  >
                    <Copy size={13} />
                  </button>
                </div>
                <div className="launch-step">
                  <span className="step-n">2</span>
                  <span>Launch the game normally from Steam. Plugins load automatically.</span>
                </div>
              </details>
            </>
          ) : launchOption && game.proton ? (
            <>
              <div className="launch-step">
                <span className="step-n">1</span>
                <span>
                  Non-Steam Proton/Wine game — add this environment variable wherever the game is
                  launched (Lutris → game/runner options → Environment variables, Bottles, Heroic,
                  or your own launch script):
                </span>
              </div>
              <div className="launch-opt">
                <code>{launchOption.replace(" %command%", "")}</code>
                <button
                  className="icon-btn"
                  title="Copy"
                  onClick={() => copy(launchOption.replace(" %command%", ""), "Override")}
                >
                  <Copy size={13} />
                </button>
              </div>
              <div className="launch-step">
                <span className="step-n">2</span>
                <span>
                  Launch the game as usual. It must run from{" "}
                  <code>{gameDir}</code> so Wine loads the loader's proxy DLL — then your plugins
                  load automatically.
                </span>
              </div>
              <div className="loader-hint">
                <span>
                  Script form: <code>{launchOption.replace(" %command%", "")} wine "YourGame.exe"</code>
                  {loader === "MelonLoader"
                    ? " — MelonLoader on Proton can need extra steps; check its wiki if mods don't load."
                    : ""}
                </span>
              </div>
            </>
          ) : launchOption ? (
            <>
              <div className="loader-hint">
                <span>Non-Steam native Linux game — launch it through the loader's wrapper:</span>
              </div>
              <div className="launch-opt">
                <code>{`"${status?.run_script ?? "run_bepinex.sh"}" "${game.exe_path ?? "<game>"}"`}</code>
                <button
                  className="icon-btn"
                  title="Copy"
                  onClick={() =>
                    copy(
                      `"${status?.run_script ?? "run_bepinex.sh"}" "${game.exe_path ?? "<game>"}"`,
                      "Command"
                    )
                  }
                >
                  <Copy size={13} />
                </button>
              </div>
            </>
          ) : (
            <div className="loader-hint">
              <AlertTriangle size={13} />
              <span>
                {loader === "BepInEx"
                  ? "No run_bepinex.sh found — finish installing a Linux BepInEx build, then the launch command appears here."
                  : "MelonLoader on native Linux: see its wiki for the launch command."}
              </span>
            </div>
          )}
          </div>
        </div>
      )}

      {busy && (
        <div className="loader-busy">
          <Loader2 size={15} className="spin" /> {busy}
        </div>
      )}
    </div>
  );
}
