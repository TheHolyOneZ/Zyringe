import { useEffect, useState, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { save } from "@tauri-apps/plugin-dialog";
import { Copy, CheckCircle2, XCircle, X } from "lucide-react";
import type { InjectMode, MonoProcess, RunState } from "./types";
import TitleBar from "./components/TitleBar";
import ProcessList from "./components/ProcessList";
import InjectForm from "./components/InjectForm";
import StatusLog from "./components/StatusLog";
import MetadataBrowser from "./components/MetadataBrowser";
import InfoTip from "./components/InfoTip";
import ProcessContextMenu from "./components/ProcessContextMenu";
import LoaderPanel from "./components/LoaderPanel";
import SettingsModal from "./components/SettingsModal";
import ResizeHandles from "./components/ResizeHandles";
import { loadSettings, saveSettings, applyAccent, type Settings } from "./settings";

const clamp = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, v));


const numPref = (k: string, d: number) => {
  const v = Number(localStorage.getItem(k));
  return Number.isFinite(v) && v > 0 ? v : d;
};

interface Preset {
  dll: string | null;
  namespace: string;
  className: string;
  method: string;
  args?: string[];
}
const loadPresets = (): Record<string, Preset> => {
  try {
    return JSON.parse(localStorage.getItem("zyringe.presets") || "{}");
  } catch {
    return {};
  }
};
const savePreset = (key: string, p: Preset) => {
  if (!key) return;
  const all = loadPresets();
  all[key] = p;
  localStorage.setItem("zyringe.presets", JSON.stringify(all));
};

export default function App() {
  const [mode, setMode] = useState<InjectMode>("attach");
  const [selected, setSelected] = useState<MonoProcess | null>(null);
  const [exePath, setExePath] = useState("");
  const [dllPath, setDllPath] = useState<string | null>(null);
  const [namespace, setNamespace] = useState("");
  const [className, setClassName] = useState("");
  const [method, setMethod] = useState("");
  const [args, setArgs] = useState<string[]>([]);
  const [log, setLog] = useState<string[]>([]);
  const [runState, setRunState] = useState<RunState>("idle");
  const [elapsed, setElapsed] = useState(0);
  const [dragActive, setDragActive] = useState(false);
  const [browserOpen, setBrowserOpen] = useState(false);
  const [toast, setToast] = useState<{ kind: "ok" | "err" | "info"; msg: string } | null>(null);
  const [ctxMenu, setCtxMenu] = useState<{ x: number; y: number; proc: MonoProcess } | null>(null);
  const [settings, setSettings] = useState<Settings>(loadSettings);
  const [settingsOpen, setSettingsOpen] = useState(false);

  const [pin, setPin] = useState<MonoProcess | null>(null);
  const [pinLivePid, setPinLivePid] = useState<number | null>(null);
  const pinRef = useRef<MonoProcess | null>(null);
  pinRef.current = pin;
  const [showGuide, setShowGuide] = useState(
    () => localStorage.getItem("zyringe.guideDismissed") !== "1"
  );
  const dismissGuide = () => {
    setShowGuide(false);
    localStorage.setItem("zyringe.guideDismissed", "1");
  };
  const startedAt = useRef(0);


  useEffect(() => applyAccent(settings.accent), [settings.accent]);

  const updateSettings = (patch: Partial<Settings>) => {
    setSettings((prev) => {
      const next = { ...prev, ...patch };
      saveSettings(next);
      return next;
    });
  };


  const notify = (kind: "ok" | "err" | "info", msg: string) => setToast({ kind, msg });

  useEffect(() => {
    if (!toast) return;
    const id = setTimeout(() => setToast(null), toast.kind === "err" ? 7000 : 3500);
    return () => clearTimeout(id);
  }, [toast]);


  const [sidebarW, setSidebarW] = useState(() => numPref("zyringe.sidebarW", 290));
  const [consoleH, setConsoleH] = useState(() => numPref("zyringe.consoleH", 280));
  useEffect(() => localStorage.setItem("zyringe.sidebarW", String(sidebarW)), [sidebarW]);
  useEffect(() => localStorage.setItem("zyringe.consoleH", String(consoleH)), [consoleH]);

  const append = useCallback((line: string) => {
    setLog((prev) => [...prev, line]);
  }, []);

  useEffect(() => {
    const un = listen<string>("zyringe://log", (e) => append(e.payload));
    return () => {
      un.then((f) => f());
    };
  }, [append]);

  useEffect(() => {
    if (runState !== "running") return;
    const id = setInterval(() => setElapsed(Date.now() - startedAt.current), 100);
    return () => clearInterval(id);
  }, [runState]);


  useEffect(() => {
    setRunState((s) => (s === "running" || s === "idle" ? s : "idle"));
  }, [selected?.pid, dllPath]);


  useEffect(() => {
    const un = getCurrentWebview().onDragDropEvent((event) => {
      if (event.payload.type === "over" || event.payload.type === "enter") {
        setDragActive(true);
      } else if (event.payload.type === "leave") {
        setDragActive(false);
      } else if (event.payload.type === "drop") {
        setDragActive(false);
        const dll = event.payload.paths.find((p) => p.toLowerCase().endsWith(".dll"));
        if (dll) setDllPath(dll);
        else setToast({ kind: "err", msg: "That's not a .dll — drop a .NET mod assembly (.dll)." });
      }
    });
    return () => {
      un.then((f) => f());
    };
  }, []);


  const startDrag = (
    axis: "x" | "y",
    getStart: () => number,
    apply: (next: number) => void,
    sign: 1 | -1
  ) => (e: React.MouseEvent) => {
    e.preventDefault();
    const origin = axis === "x" ? e.clientX : e.clientY;
    const startVal = getStart();
    const move = (ev: MouseEvent) => {
      const delta = (axis === "x" ? ev.clientX : ev.clientY) - origin;
      apply(startVal + sign * delta);
    };
    const up = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
    document.body.style.cursor = axis === "x" ? "col-resize" : "row-resize";
    document.body.style.userSelect = "none";
  };

  const onSelect = useCallback((proc: MonoProcess) => {
    setSelected(proc);

    if (proc.engine === "IL2CPP") {
      setPin(proc);
      setPinLivePid(proc.pid);
    } else {
      setPin(null);
      setPinLivePid(null);
    }
    if (proc.exe_path) {
      setExePath(proc.exe_path);

      const preset = loadPresets()[proc.exe_path];
      if (preset) {
        setDllPath(preset.dll);
        setNamespace(preset.namespace);
        setClassName(preset.className);
        setMethod(preset.method);
        setArgs(preset.args ?? []);
      }
    }
  }, []);

  const onContext = useCallback(
    (
      proc: MonoProcess,
      e: {
        preventDefault: () => void;
        stopPropagation: () => void;
        clientX: number;
        clientY: number;
      }
    ) => {
      e.preventDefault();
      e.stopPropagation();
      setCtxMenu({ x: e.clientX, y: e.clientY, proc });
    },
    []
  );


  const onProcessList = useCallback((procs: MonoProcess[]) => {

    setSelected((cur) => {
      if (!cur) return cur;
      if (procs.some((p) => p.pid === cur.pid)) return cur;
      if (!cur.exe_path) return cur;
      const sameExe = procs.filter((p) => p.exe_path === cur.exe_path && p.pid !== cur.pid);
      return sameExe.find((p) => !p.duplicate) ?? sameExe[0] ?? cur;
    });

    const g = pinRef.current;
    if (g?.exe_path) {
      const same = procs.filter((p) => p.exe_path === g.exe_path);
      const live = same.find((p) => !p.duplicate) ?? same[0];
      setPinLivePid(live ? live.pid : null);
    }
  }, []);

  const copyText = async (text: string, label: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setToast({ kind: "info", msg: `Copied ${label}` });
    } catch {
      setToast({ kind: "err", msg: "Clipboard copy failed" });
    }
  };

  const busy = runState === "running";
  const canInject =
    !busy &&
    !!dllPath &&
    !!className.trim() &&
    !!method.trim() &&
    (mode === "attach" ? !!selected && selected.injectable : !!exePath.trim());

  const onInject = async () => {

    const presetKey = mode === "attach" ? selected?.exe_path ?? "" : exePath;
    savePreset(presetKey, { dll: dllPath, namespace, className, method, args });

    if (settings.clearOnInject) setLog([]);
    setRunState("running");
    startedAt.current = Date.now();
    setElapsed(0);
    try {
      if (mode === "attach") {
        if (!selected) return;
        await invoke("inject", {
          request: {
            pid: selected.pid,
            dll_path: dllPath,
            namespace,
            class_name: className,
            method,
            args,
            timeout_secs: settings.timeoutSecs,
          },
        });
      } else {
        await invoke("launch_with_preload", {
          request: {
            exe_path: exePath,
            dll_path: dllPath,
            namespace,
            class_name: className,
            method,
            args,
            working_dir: null,
          },
        });
      }
      setRunState("ok");
      setToast({ kind: "ok", msg: mode === "attach" ? "Injection succeeded" : "Launched with mod" });
    } catch (e) {
      append(`✗ ${e}`);
      setRunState("err");
      setToast({ kind: "err", msg: String(e) });
    } finally {
      setElapsed(Date.now() - startedAt.current);
    }
  };

  const copyPid = async () => {
    const pidVal = pin ? pinLivePid : selected?.pid;
    if (!pidVal) return;
    try {
      await navigator.clipboard.writeText(String(pidVal));
      setToast({ kind: "info", msg: `Copied PID ${pidVal}` });
    } catch {
      setToast({ kind: "err", msg: "Clipboard copy failed" });
    }
  };

  const copyLog = async () => {
    try {
      await navigator.clipboard.writeText(log.join("\n"));
      setToast({ kind: "info", msg: "Console copied" });
    } catch {
      setToast({ kind: "err", msg: "Clipboard copy failed" });
    }
  };
  const saveLog = async () => {
    try {
      const path = await save({
        defaultPath: "zyringe-log.txt",
        filters: [{ name: "Log", extensions: ["txt", "log"] }],
      });
      if (!path) return;
      await invoke("save_text", { path, content: log.join("\n") });
      setToast({ kind: "info", msg: "Log saved" });
    } catch (e) {
      setToast({ kind: "err", msg: String(e) });
    }
  };

  const onCancel = async () => {
    if (mode === "attach" && selected) {
      append("→ cancel requested…");
      try {
        await invoke("cancel_injection", { pid: selected.pid });
      } catch (e) {
        append(`✗ ${e}`);
      }
    }
  };

  const clearConsole = () => {
    setLog([]);
    setRunState("idle");
  };

  const openPlayerLog = async () => {
    try {
      const p = await invoke<string>("open_player_log");
      append(`→ opened ${p}`);
    } catch (e) {
      append(`✗ ${e}`);
    }
  };
  const openHelperLog = async () => {
    if (!selected) return;
    try {
      const p = await invoke<string>("open_helper_log", { pid: selected.pid });
      append(`→ opened ${p}`);
    } catch (e) {
      append(`✗ ${e}`);
    }
  };


  const kb = useRef({ inject: onInject, cancel: onCancel, clear: clearConsole, canInject, busy });
  kb.current = { inject: onInject, cancel: onCancel, clear: clearConsole, canInject, busy };
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const meta = e.ctrlKey || e.metaKey;
      if (meta && e.key === "Enter") {
        e.preventDefault();
        if (kb.current.canInject) kb.current.inject();
      } else if (e.key === "Escape" && kb.current.busy) {
        e.preventDefault();
        kb.current.cancel();
      } else if (meta && e.key.toLowerCase() === "l") {
        e.preventDefault();
        kb.current.clear();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <div className="app">
      <ResizeHandles />
      {toast && (
        <div className={`toast ${toast.kind}`}>
          {toast.kind === "ok" && <CheckCircle2 size={16} />}
          {toast.kind === "err" && <XCircle size={16} />}
          <span>{toast.msg}</span>
        </div>
      )}
      {dragActive && (
        <div className="drop-overlay">
          <div className="drop-card">Drop a .dll to load it</div>
        </div>
      )}
      {ctxMenu && (
        <ProcessContextMenu
          proc={ctxMenu.proc}
          x={ctxMenu.x}
          y={ctxMenu.y}
          onClose={() => setCtxMenu(null)}
          onSelect={onSelect}
          onCopy={copyText}
          onOpenPlayerLog={openPlayerLog}
        />
      )}
      {browserOpen && dllPath && (
        <MetadataBrowser
          dllPath={dllPath}
          onClose={() => setBrowserOpen(false)}
          onPick={(ep) => {
            setNamespace(ep.namespace);
            setClassName(ep.class);
            setMethod(ep.method);
            setArgs(ep.param_count > 0 ? Array(ep.param_count).fill("") : []);
            setBrowserOpen(false);
          }}
        />
      )}
      <TitleBar onSettings={() => setSettingsOpen(true)} />
      {settingsOpen && (
        <SettingsModal
          settings={settings}
          onChange={updateSettings}
          onClose={() => setSettingsOpen(false)}
        />
      )}
      <div className="workspace">
        <aside className="sidebar" style={{ width: sidebarW }}>
          <ProcessList
            selectedPid={pin ? pinLivePid : selected?.pid ?? null}
            onSelect={onSelect}
            onContext={onContext}
            onList={onProcessList}
            refreshMs={settings.refreshMs}
          />
        </aside>
        <div
          className="resize-x"
          onMouseDown={startDrag("x", () => sidebarW, (v) => setSidebarW(clamp(v, 210, 520)), 1)}
        />

        <div className="main">
          <div className="context-bar">
            <div className="ctx-target">
              <span
                className={`ctx-dot ${
                  pin
                    ? pinLivePid
                      ? "on"
                      : "warn"
                    : selected
                    ? selected.injectable
                      ? "on"
                      : "warn"
                    : ""
                }`}
              />
              {pin ? (
                <div className="ctx-target-text">
                  <span className="ctx-name">{pin.name}</span>
                  <span className="ctx-meta">
                    {pinLivePid ? `PID ${pinLivePid}` : "not running"} · IL2CPP
                    {pin.app_id ? ` · Steam ${pin.app_id}` : ""}
                  </span>
                </div>
              ) : selected ? (
                <div className="ctx-target-text">
                  <span className="ctx-name">{selected.name}</span>
                  <span className="ctx-meta">
                    PID {selected.pid} · {selected.flavor} · {selected.thread_count} threads
                  </span>
                </div>
              ) : mode === "launch" ? (
                <div className="ctx-target-text">
                  <span className="ctx-name muted">Launch mode</span>
                  <span className="ctx-meta">Set the game executable in the form</span>
                </div>
              ) : (
                <div className="ctx-target-text">
                  <span className="ctx-name muted">No target selected</span>
                  <span className="ctx-meta">Pick a process from the left</span>
                </div>
              )}
              {(pin || selected) && (
                <button className="ctx-copy" onClick={copyPid} title="Copy PID">
                  <Copy size={14} />
                </button>
              )}
            </div>

            {!pin && (
              <div className="mode-wrap">
                <InfoTip text="Attach injects into a running game (select it on the left). Launch starts the game yourself with the mod preloaded — use it when attach can't reach the process." />
                <div className="mode-toggle">
                  <button className={mode === "attach" ? "active" : ""} onClick={() => setMode("attach")}>
                    Attach
                  </button>
                  <button className={mode === "launch" ? "active" : ""} onClick={() => setMode("launch")}>
                    Launch
                  </button>
                </div>
              </div>
            )}
          </div>

          <div className="config">
            {!pin && !selected && log.length === 0 && showGuide && (
              <div className="guide-card">
                <div className="guide-head">
                  <span>Getting started</span>
                  <button className="icon-btn" onClick={dismissGuide} title="Dismiss">
                    <X size={14} />
                  </button>
                </div>
                <ol className="guide-steps">
                  <li>
                    <strong>Pick a target</strong> on the left — Mono games are injectable; IL2CPP
                    games open a mod-loader panel. No game running? Switch to <strong>Launch</strong>{" "}
                    and point Zyringe at the game's executable.
                  </li>
                  <li>
                    <strong>Choose your mod DLL</strong> and its entry point
                    (Namespace.Class.Method) — <strong>Browse methods</strong> lists them straight
                    from the DLL.
                  </li>
                  <li>
                    <strong>Inject</strong> — progress and results appear in the console below.
                  </li>
                </ol>
              </div>
            )}
            {pin ? (
              <LoaderPanel game={pin} onToast={notify} />
            ) : (
              <InjectForm
              mode={mode}
              selected={selected}
              exePath={exePath}
              onExePathChange={setExePath}
              dllPath={dllPath}
              onDllPick={setDllPath}
              modFolder={settings.modFolder}
              namespace={namespace}
              onNamespaceChange={setNamespace}
              className={className}
              onClassChange={setClassName}
              method={method}
              onMethodChange={setMethod}
              args={args}
              onArgsChange={setArgs}
              busy={busy}
              injected={runState === "ok"}
              canInject={canInject}
              onInject={onInject}
              onCancel={onCancel}
              onBrowse={() => setBrowserOpen(true)}
              />
            )}
          </div>

          {/* The console only carries injection output. IL2CPP loaders (Melon/
              BepInEx) log to their own files, not to us — so hide it and give the
              loader panel the full height. */}
          {!pin && (
            <>
              <div
                className="resize-y"
                onMouseDown={startDrag("y", () => consoleH, (v) => setConsoleH(clamp(v, 140, 560)), -1)}
              />
              <div className="console-region" style={{ height: consoleH }}>
                <StatusLog
                  lines={log}
                  onClear={clearConsole}
                  runState={runState}
                  elapsed={elapsed}
                  runningVerb={mode === "launch" ? "Launching" : "Injecting"}
                  onOpenPlayerLog={openPlayerLog}
                  onOpenHelperLog={openHelperLog}
                  onCopyLog={copyLog}
                  onSaveLog={saveLog}
                  hasTarget={!!selected}
                />
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
