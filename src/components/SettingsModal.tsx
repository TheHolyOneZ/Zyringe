import { useEffect } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Settings2, X, FolderOpen, RotateCcw, Check } from "lucide-react";
import { ACCENTS, DEFAULT_SETTINGS, type Settings } from "../settings";
import InfoTip from "./InfoTip";

interface Props {
  settings: Settings;
  onChange: (patch: Partial<Settings>) => void;
  onClose: () => void;
}

const REFRESH_OPTIONS = [
  { label: "1s", ms: 1000 },
  { label: "2s", ms: 2000 },
  { label: "5s", ms: 5000 },
  { label: "Off", ms: 0 },
];

export default function SettingsModal({ settings, onChange, onClose }: Props) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const pickFolder = async () => {
    const dir = await open({ directory: true, multiple: false });
    if (typeof dir === "string") onChange({ modFolder: dir });
  };

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal settings" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <div className="modal-title">
            <Settings2 size={16} />
            <div className="modal-title-main">Settings</div>
          </div>
          <button className="icon-btn" onClick={onClose} title="Close (Esc)">
            <X size={15} />
          </button>
        </div>

        <div className="settings-body">

          <div className="setting">
            <div className="setting-label">
              Accent color
              <InfoTip text="Recolors the whole app — buttons, highlights, selection bars and glows. Applies instantly, no restart." />
            </div>
            <div className="swatches">
              {ACCENTS.map((a) => {
                const active = settings.accent[0] === a.colors[0];
                return (
                  <button
                    key={a.name}
                    className={`swatch${active ? " active" : ""}`}
                    title={a.name}
                    style={{ background: `linear-gradient(135deg, ${a.colors[0]}, ${a.colors[1]})` }}
                    onClick={() => onChange({ accent: a.colors })}
                  >
                    {active && <Check size={14} />}
                  </button>
                );
              })}
            </div>
          </div>


          <div className="setting">
            <div className="setting-label">
              Process refresh
              <InfoTip text="How often the sidebar re-scans for running Unity/Mono games. 'Off' stops the auto-scan — use the ↻ button to refresh manually." />
              <span className="setting-hint">how often the target list updates</span>
            </div>
            <div className="seg-choice">
              {REFRESH_OPTIONS.map((o) => (
                <button
                  key={o.label}
                  className={settings.refreshMs === o.ms ? "active" : ""}
                  onClick={() => onChange({ refreshMs: o.ms })}
                >
                  {o.label}
                </button>
              ))}
            </div>
          </div>


          <div className="setting">
            <div className="setting-label">
              DLL picker start folder
              <InfoTip text="Purely a shortcut: the folder the 'Choose .dll' file browser opens in, so you don't navigate from scratch each time. It does NOT auto-pick or guess a DLL — you always choose the file yourself for the game you're modding." />
              <span className="setting-hint">where the file browser opens — you still pick the .dll yourself</span>
            </div>
            <div className="setting-row">
              <button className="secondary-btn" onClick={pickFolder}>
                <FolderOpen size={14} /> Choose…
              </button>
              <span className="setting-path" title={settings.modFolder ?? ""}>
                {settings.modFolder ?? "System default"}
              </span>
              {settings.modFolder && (
                <button className="icon-btn" title="Clear" onClick={() => onChange({ modFolder: null })}>
                  <X size={13} />
                </button>
              )}
            </div>
          </div>


          <div className="setting">
            <div className="setting-label">
              Injection timeout
              <InfoTip text="How long to wait for your mod's entry method to finish before giving up. Raise it for slow-loading mods; lower it to fail faster on a wrong/dead target." />
              <span className="setting-hint">max wait for the mod's entry point</span>
            </div>
            <div className="setting-row">
              <input
                type="number"
                min={5}
                max={300}
                value={settings.timeoutSecs}
                onChange={(e) =>
                  onChange({ timeoutSecs: Math.max(5, Math.min(300, Number(e.target.value) || 40)) })
                }
              />
              <span className="setting-unit">seconds</span>
            </div>
          </div>

          {/* Clear on inject */}
          <div className="setting setting-inline">
            <div className="setting-label">
              Clear console on each inject
              <InfoTip text="Wipe the console at the start of every injection for a clean log. Turn off to keep the full history across runs." />
              <span className="setting-hint">start every run with a clean log</span>
            </div>
            <button
              className={`toggle${settings.clearOnInject ? " on" : ""}`}
              onClick={() => onChange({ clearOnInject: !settings.clearOnInject })}
              role="switch"
              aria-checked={settings.clearOnInject}
            >
              <span className="toggle-knob" />
            </button>
          </div>
          {/* About / status */}
          <div className="setting about">
            <div className="about-head">
              <span className="about-name">Zyringe</span>
              <span className="beta-pill">BETA</span>
            </div>
            <p className="about-body">
              Mono injection (Attach &amp; Launch) and IL2CPP mod loaders (MelonLoader &amp; BepInEx)
              are all implemented and were tested working end-to-end. It's BETA only because it
              hasn't been battle-hardened across every distro, game and edge case yet — expect the
              occasional rough edge.
            </p>
            <dl className="about-tested">
              <div>
                <dt>Verified</dt>
                <dd>Mono Attach · Mono Launch · IL2CPP MelonLoader · IL2CPP BepInEx</dd>
              </div>
              <div>
                <dt>Tested on</dt>
                <dd>EndeavourOS · Linux 6.18 LTS · x86_64 · KDE / X11 · glibc 2.43</dd>
              </div>
            </dl>
          </div>
        </div>

        <div className="settings-foot">
          <button className="text-btn" onClick={() => onChange({ ...DEFAULT_SETTINGS })}>
            <RotateCcw size={13} /> Reset to defaults
          </button>
        </div>
      </div>
    </div>
  );
}
