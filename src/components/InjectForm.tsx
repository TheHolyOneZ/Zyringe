import { Ban, ListTree, Plus, X } from "lucide-react";
import type { InjectMode, MonoProcess } from "../types";
import DllPicker from "./DllPicker";
import InfoTip from "./InfoTip";

interface Props {
  mode: InjectMode;
  selected: MonoProcess | null;
  exePath: string;
  onExePathChange: (v: string) => void;
  dllPath: string | null;
  onDllPick: (p: string) => void;
  modFolder: string | null;
  namespace: string;
  onNamespaceChange: (v: string) => void;
  className: string;
  onClassChange: (v: string) => void;
  method: string;
  onMethodChange: (v: string) => void;
  args: string[];
  onArgsChange: (v: string[]) => void;
  busy: boolean;
  injected: boolean;
  canInject: boolean;
  onInject: () => void;
  onCancel: () => void;
  onBrowse: () => void;
}

export default function InjectForm(p: Props) {
  const notInjectable = p.mode === "attach" && !!p.selected && !p.selected.injectable;


  const missingReason = (() => {
    if (p.canInject || p.busy) return null;
    if (p.mode === "attach") {
      if (!p.selected) return "Select a target on the left";
      if (!p.selected.injectable) return null;
    } else if (!p.exePath.trim()) {
      return "Set the game executable above";
    }
    if (!p.dllPath) return "Choose a mod DLL";
    if (!p.className.trim()) return "Enter the class name";
    if (!p.method.trim()) return "Enter the method name";
    return null;
  })();

  return (
    <div className="form">
      {notInjectable && (
        <div className="inject-banner">
          <Ban size={14} />
          <span>
            {p.selected!.proton ? (
              <>
                <strong>{p.selected!.name}</strong> runs through Proton/Wine — its Mono runtime is a
                Windows DLL, which Zyringe's Linux injector can't attach to. Attach &amp; Launch work
                on native-Linux Mono games.
              </>
            ) : (
              <>
                <strong>{p.selected!.name}</strong> has no live Mono runtime to inject into — it may
                be a launcher, bootstrap, or duplicate process. Pick another target on the left.
              </>
            )}
          </span>
        </div>
      )}
      {p.mode === "launch" && (
        <div className="field">
          <label>Game executable</label>
          <input
            type="text"
            placeholder="/path/to/Game.x86_64"
            value={p.exePath}
            onChange={(e) => p.onExePathChange(e.target.value)}
          />
        </div>
      )}

      <DllPicker dllPath={p.dllPath} onPick={p.onDllPick} defaultFolder={p.modFolder} />

      <div className="field">
        <div className="field-head">
          <label>
            Entry point
            <InfoTip text="The static, void method Zyringe calls right after loading your DLL — e.g. HasteMod.Loader.Init. Use Browse methods to pick it straight from the assembly." />
          </label>
          <button
            className="text-btn"
            disabled={!p.dllPath}
            onClick={p.onBrowse}
            title={p.dllPath ? "Browse methods in the DLL" : "Pick a DLL first"}
          >
            <ListTree size={13} /> Browse methods
          </button>
        </div>
        <div className="entry-group">
          <input
            className="seg"
            type="text"
            placeholder="Namespace"
            value={p.namespace}
            onChange={(e) => p.onNamespaceChange(e.target.value)}
          />
          <span className="seg-div" />
          <input
            className="seg"
            type="text"
            placeholder="Class"
            value={p.className}
            onChange={(e) => p.onClassChange(e.target.value)}
          />
          <span className="seg-div" />
          <input
            className="seg"
            type="text"
            placeholder="Method"
            value={p.method}
            onChange={(e) => p.onMethodChange(e.target.value)}
          />
        </div>
        <span className="field-note">Namespace can be blank for top-level (global-namespace) types.</span>
      </div>

      <div className="field">
        <div className="field-head">
          <label>
            Arguments
            <InfoTip text="Optional. Each is passed to the entry method as a System.String, in order. Leave empty to call a parameterless method. Picking a method in Browse pre-fills the right number of slots." />
            <span className="label-hint">optional · passed as strings</span>
          </label>
          <button className="text-btn" onClick={() => p.onArgsChange([...p.args, ""])}>
            <Plus size={13} /> Add
          </button>
        </div>
        {p.args.length === 0 ? (
          <div className="args-empty">No arguments — calls a parameterless method.</div>
        ) : (
          <div className="args-list">
            {p.args.map((a, i) => (
              <div className="arg-row" key={i}>
                <span className="arg-idx">{i}</span>
                <input
                  type="text"
                  placeholder={`argument ${i}`}
                  value={a}
                  onChange={(e) =>
                    p.onArgsChange(p.args.map((x, j) => (j === i ? e.target.value : x)))
                  }
                />
                <button
                  className="icon-btn"
                  title="Remove"
                  onClick={() => p.onArgsChange(p.args.filter((_, j) => j !== i))}
                >
                  <X size={13} />
                </button>
              </div>
            ))}
          </div>
        )}
      </div>

      <div className="action-row">
        <button className="inject-btn" disabled={!p.canInject} onClick={p.onInject}>
          {p.mode === "attach"
            ? p.busy
              ? "Injecting…"
              : p.injected
              ? "Re-inject"
              : "Inject"
            : p.busy
            ? "Launching…"
            : "Launch with Mod"}
        </button>
        {p.busy && p.mode === "attach" && (
          <button className="cancel-btn" onClick={p.onCancel}>
            <Ban size={15} /> Cancel
          </button>
        )}
        {p.busy && p.mode === "launch" && (
          <span className="inject-note">
            Launch can't be cancelled — close the game window to stop it.
          </span>
        )}
        {missingReason && <span className="inject-note muted">{missingReason}</span>}
      </div>
    </div>
  );
}
