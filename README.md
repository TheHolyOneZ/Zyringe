<div align="center">

<img src="src/assets/logo.png" width="128" alt="Zyringe logo" />

# Zyringe

**A Linux-native GUI for injecting .NET/C# mods into Unity & Mono games — and for setting up IL2CPP mod loaders — without touching a terminal or a launch script.**

<p>
  <img alt="platform" src="https://img.shields.io/badge/platform-Linux%20x86__64-1f6feb" />
  <img alt="license" src="https://img.shields.io/badge/license-GPL--3.0--only-3fb950" />
  <img alt="status" src="https://img.shields.io/badge/status-BETA-f0b74a" />
  <img alt="frontend" src="https://img.shields.io/badge/UI-React%20%2B%20TypeScript-61dafb" />
  <img alt="backend" src="https://img.shields.io/badge/core-Rust%20%2B%20Tauri%20v2-dea584" />
</p>

<sub>Made by <b>TheHolyOneZ</b> · <a href="https://github.com/TheHolyOneZ/Zyringe">Repo</a> · <a href="https://zlogic.eu/zyringe/">Download</a> · <a href="https://zsync.eu/">More projects</a> · <a href="https://zlogic.eu/mods/">Game mods</a></sub>

</div>

---

Zyringe is the Linux-native equivalent of [SharpMonoInjector](https://github.com/warbler/SharpMonoInjector): pick a running Unity/Mono game, pick a `.dll`, name an entry point (`Namespace.Class.Method`), and inject it — in-memory, no disk write, no launch wrapper. For **IL2CPP** games (which have no Mono runtime to inject into), it installs and manages **MelonLoader** or **BepInEx** for you, sets the Steam launch option automatically, and launches the game — so modding an IL2CPP title is a few clicks instead of a wiki crawl.

> [!WARNING]
> **BETA.** Mono injection (Attach & Launch) and IL2CPP loaders (MelonLoader & BepInEx) are all implemented and tested working end-to-end. It is marked BETA only because it hasn't been battle-hardened across every distro, game and edge case yet. Expect the occasional rough edge.

---

## Table of contents

- [Who it's for](#who-its-for)
- [Feature overview](#feature-overview)
- [Target discovery](#target-discovery)
- [Mono injection](#mono-injection)
- [Entry-point browser &amp; arguments](#entry-point-browser--arguments)
- [IL2CPP mod loaders](#il2cpp-mod-loaders)
- [Interface &amp; UX](#interface--ux)
- [Security model](#security-model)
- [Architecture](#architecture)
- [Tech stack](#tech-stack)
- [Platform support](#platform-support)
- [Requirements](#requirements)
- [Build &amp; run](#build--run-development)
- [Packaging](#packaging-appimage--deb)
- [Usage walkthroughs](#usage-walkthroughs)
- [Keyboard shortcuts](#keyboard-shortcuts)
- [Troubleshooting / FAQ](#troubleshooting--faq)
- [Project layout](#project-layout)
- [Contributing](#contributing)
- [License](#license)
- [Disclaimer](#disclaimer)

---

## Who it's for

<table>
<tr><td width="33%" valign="top">

### 🎮 Modders
Run existing C#/.NET mods in Unity games on Linux — the thing that used to require Windows or fragile scripts.

</td><td width="33%" valign="top">

### 🧪 Reverse-engineers
Attach into a live game, load an assembly in memory, and call arbitrary static entry points with a real UI and a live log.

</td><td width="33%" valign="top">

### 🛠️ Tinkerers
IL2CPP games too — pick a loader, click install, click launch. No hand-editing Steam launch options.

</td></tr>
</table>

---

## Feature overview

<table>
<tr>
<td width="50%" valign="top">

**Mono injection (the tested core)**
- Attach into a running game via `ptrace`
- Launch a game with the mod pre-loaded (`LD_PRELOAD`)
- In-memory assembly load — never written to disk
- Call a `static void` entry point with string args
- Capture the return value & managed exceptions
- Entry-point browser reads the DLL's metadata
- Per-game presets, drag-and-drop, cancel & timeout

</td>
<td width="50%" valign="top">

**IL2CPP mod loaders (BETA)**
- Detects IL2CPP games automatically
- Install MelonLoader / BepInEx from GitHub or a zip
- Guards against wrong-platform / wrong-variant builds
- Manage plugin DLLs in-app
- Auto-writes the Steam launch option & launches
- Works with Proton, Wine, cracked & non-Steam layouts

</td>
</tr>
<tr>
<td width="50%" valign="top">

**Interface**
- Custom-titlebar dashboard, fully resizable
- Live accent theming, settings, tooltips
- Right-click context menus everywhere
- Live console with staged progress + log export

</td>
<td width="50%" valign="top">

**Security**
- GUI runs **unprivileged**
- Only a tiny helper is elevated, per-injection, via `pkexec`
- polkit rule pins the exact privileged binary path

</td>
</tr>
</table>

---

## Target discovery

Zyringe scans `/proc/*/maps` for running Unity/Mono games — it never needs to have launched them.

<table>
<tr><th align="left">What it detects</th><th align="left">How</th></tr>
<tr><td><b>Mono runtime</b> + flavor</td><td>Matches <code>libmonobdwgc-2.0</code> (BleedingEdge), <code>libmonosgen-2.0</code> (SGen), <code>libmono-2.0</code>, Boehm — reported per target</td></tr>
<tr><td><b>IL2CPP</b> games</td><td>Detects <code>GameAssembly</code> / <code>libil2cpp</code> — surfaced but routed to the loader flow, not injection</td></tr>
<tr><td><b>Windows-Mono under Proton</b></td><td>Detects <code>mono-2.0-bdwgc.dll</code> inside Wine → listed as <b>can't attach</b> (a Linux injector can't reach a Windows-DLL runtime)</td></tr>
<tr><td><b>Proton / Wine</b></td><td>From <code>STEAM_COMPAT_DATA_PATH</code> / <code>WINEPREFIX</code> / <code>PROTON_VERSION</code> in the process environment</td></tr>
<tr><td><b>Steam AppID</b></td><td>Read from <code>/proc/&lt;pid&gt;/environ</code> — enables the launch-option automation</td></tr>
<tr><td><b>Duplicate PIDs</b></td><td>A game shown under several PIDs (Steam runtime bootstrap/reaper) — the real one (most threads) is primary, the rest badged <code>dup</code>; nothing is hidden</td></tr>
</table>

Each row shows engine, flavor, thread count and injectability; a **right-click menu** gives the full breakdown (name, PID, engine, runtime, threads, injectable, exe path, data dir, Mono `.so`, cmdline) plus quick actions. The list auto-refreshes on a configurable interval (1s / 2s / 5s / Off), and it's **container-aware** — it resolves symbols through `/proc/<pid>/root` so it works inside Steam's pressure-vessel.

---

## Mono injection

Two modes share one self-contained C helper payload (`libzyringe.so`, links only libc, resolves every Mono symbol at runtime via `dlsym`).

### Attach mode <sub>(primary)</sub>

Injects into an **already-running** game:

1. A small privileged binary (`zyringe-inject`) is elevated **per-injection** via `pkexec` (polkit). The GUI stays unprivileged.
2. It `ptrace`-attaches, picks a safe **syscall-parked** thread (scored by state / `wchan`, avoiding the loader lock), and remote-`dlopen`s the helper.
3. The helper does an **SMI-grade in-memory load** — `mono_image_open_from_data_with_name` (the assembly is *never* written to disk, and `Assembly.Location` is set to the real path), on a **fresh attached worker thread**, then `mono_runtime_invoke`.

<details>
<summary><b>Details & hardening</b></summary>

- **String arguments** marshalled via `mono_string_new`, passed to your entry method in order.
- **Return value** captured via `mono_object_to_string`; **managed exceptions** captured and reported.
- **Cooperative cancel** — a cancel flag in your private `$XDG_RUNTIME_DIR` (not world-writable `/tmp`; TOCTOU-safe) lets you abort a stuck injection without a second password prompt.
- **Configurable timeout** on the invoke.
- **32-bit reject** — reads the target's ELF class and refuses cleanly.
- **noexec `/tmp` fallback** — parses the target's `mountinfo` and stages the helper into `/tmp` → `/dev/shm` → the game dir as needed.
- **mmap-leak fix** — the helper `munmap`s its scratch page after a short grace so injection leaves no residue.
- **Container-safe** symbol resolution via ELF `.dynsym` parsing under `/proc/<pid>/root`.
</details>

### Launch mode <sub>(fallback)</sub>

Starts the game yourself with the helper `LD_PRELOAD`ed and the config passed by env — no elevation required. Crucially, Zyringe knows the injection is done the moment the helper reports success **mid-startup** (it watches the helper's markers and also polls the helper's own log file in case the game redirects stderr), so the UI resolves to *Success* while the game keeps running — instead of hanging until the game exits.

---

## Entry-point browser &amp; arguments

No more guessing method names. **Browse methods** parses the DLL's .NET metadata directly:

- Reads the PE → CLI header → `#~` metadata tables (`TypeDef` / `MethodDef`), the `#Strings` heap, and signature blobs.
- Lists every candidate as `Namespace.Class.Method`, grouped by type, searchable.
- Badges each method **static**, **void / non-void**, and its **argument count**.
- A **"Compatible only"** filter shows just the valid entry points (static + void — arguments are fine, they're passed as strings).
- Picking a method auto-fills the entry-point fields and the right number of argument slots.

The **Arguments** editor lets you add/remove ordered string arguments passed to the entry method (each as a `System.String`).

---

## IL2CPP mod loaders

IL2CPP compiles C# to native code — there is no Mono runtime to inject into. So instead of injecting, Zyringe **provisions and manages an external loader** the game runs at startup. Fully supported: **MelonLoader** and **BepInEx** (IL2CPP builds).

### Install
- **Browse &amp; install a build** straight from the loader's GitHub releases (filtered to `.zip` assets), or **Install from a local zip**.
- **Smart guards** so you can't install the wrong thing:
  - Proton games hide **Linux** and **macOS** builds (a Linux loader crashes a Wine game).
  - BepInEx hides non-**IL2CPP** builds (a Mono/BepInEx-5 build silently no-ops on an IL2CPP game).
- **Validated downloads** — PK-magic check, zip-slip-safe extraction, exec bits preserved.
- **Post-install verification** — if the archive didn't actually produce a working loader (incomplete download, wrong asset, wrong variant, wrong platform), it **fails loudly with a specific fix** instead of a false "installed".

### Manage & launch
- Add / remove plugin DLLs and open the plugins folder in your file manager.
- **Correct game-folder resolution** even for Proton (where `/proc/<pid>/exe` is Wine, not the game): uses `STEAM_COMPAT_INSTALL_PATH`, then translates the Windows exe path from the game's argv (`Z:\…` → `/…`, other drives → the Wine prefix), then falls back to the process cwd — so it lands in the *game* folder for Steam, Proton, cracked/OnlineFix and non-Steam layouts alike.
- **Steam auto-setup** — writes the correct launch option into Steam's own config (a *targeted*, backed-up edit of `localconfig.vdf`; refuses while Steam is running so it can't be clobbered), then launches via `steam://run/<appid>`. No pasting.
- **Non-Steam / Wine** games get explicit, copyable guidance (the `WINEDLLOVERRIDES` to set in Lutris / Bottles / Heroic or your own script).
- A red banner warns if a Proton game has a loader installed but is missing its Windows proxy DLL (i.e. a Linux build slipped in).

---

## Interface &amp; UX

<table>
<tr><td width="50%" valign="top">

- **Custom titlebar** (minimize / maximize / close, drag-to-move)
- **Frameless resize** — grabbable edges & corners on all sides
- **Live accent theming** — 6 presets, applied instantly via CSS variables
- **Settings** — accent, refresh interval, DLL-picker start folder, injection timeout, clear-console-on-inject, and an About/status panel
- **Tooltips** (the `?` chips) explaining every non-obvious control

</td><td width="50%" valign="top">

- **Right-click menus** — process rows, console lines, the DLL chip
- **Live console** with a staged progress stepper, plus one-click **Player.log** and in-target **helper log**, and copy / save
- **Per-game presets** — remembers your DLL, entry point and args per game
- **Drag-and-drop** a `.dll` anywhere onto the window
- **First-run guide**, mode-aware hints, and clear "why is this disabled" messages

</td></tr>
</table>

---

## Security model

<table>
<tr><td valign="top">

```
┌─────────────────────────────┐
│ Zyringe GUI  (uid 1000)     │  ← unprivileged, always
│  scan · pick · configure    │
└───────────────┬─────────────┘
                │ pkexec (polkit, per-injection)
                ▼
┌─────────────────────────────┐
│ zyringe-inject  (root)      │  ← elevated ONLY to inject,
│  ptrace → dlopen → detach   │     then exits
└─────────────────────────────┘
```

</td><td valign="top">

- The **GUI never runs as root.** Only the tiny `zyringe-inject` binary is elevated, once per injection, and it exits immediately after.
- The polkit rule **pins the exact path** `/usr/lib/zyringe/zyringe-inject`, installed root-owned and not user-writable — so a local user can't swap the binary that runs as root. That pinning is the whole point of the install step.
- The cancel flag lives in your private runtime dir, not world-writable `/tmp`.

</td></tr>
</table>

---

## Architecture

```
React/TS frontend (Vite)                    ← unprivileged
   │  Tauri v2 IPC (invoke + events)
src-tauri/ Rust backend                     ← unprivileged
   ├── scanner.rs    find Unity/Mono games in /proc/*/maps
   ├── injector.rs   orchestrate: pkexec helper / LD_PRELOAD launch
   ├── metadata.rs   read a DLL's entry points (PE→CLI metadata)
   ├── loader.rs     install & manage MelonLoader / BepInEx
   ├── steam.rs      edit Steam launch options, launch via steam://
   └── mono.rs       locate the bundled privileged assets
   │  pkexec (polkit)
   ▼
zyringe-inject  (privileged Rust bin)       ← elevated only for injection
   ├── ptrace_engine.rs  ATTACH → remote dlopen(helper.so) → restore → DETACH
   ├── elf.rs / maps.rs  resolve remote symbols via /proc + ELF .dynsym
   └── main.rs           arg parsing, staging, cancel/timeout
   │  dlopen
   ▼
helper/libzyringe.so  (C, libc-only)        ← runs inside the game
   └── resolve Mono syms → attach thread → assembly_open → runtime_invoke
```

---

## Tech stack

<table>
<tr><th align="left">Layer</th><th align="left">Technology</th></tr>
<tr><td>Frontend</td><td>React 18 · TypeScript · Vite · lucide-react</td></tr>
<tr><td>App backend</td><td>Rust · Tauri v2 (webkit2gtk)</td></tr>
<tr><td>Privileged injector</td><td>Standalone Rust binary — <code>ptrace</code> / <code>process_vm_writev</code>, elevated via <code>pkexec</code> + polkit</td></tr>
<tr><td>In-target payload</td><td>C shared object — links only libc, resolves Mono symbols at runtime via <code>dlsym</code></td></tr>
<tr><td>Loader downloads</td><td><code>curl</code> + the <code>zip</code> crate (no bundled TLS stack)</td></tr>
<tr><td>Metadata parsing</td><td>Hand-rolled ECMA-335 (PE → CLI → <code>#~</code> tables) reader in Rust</td></tr>
</table>

---

## Platform support

<table>
<tr><th align="left">Target</th><th>Injection</th><th>Modding</th><th>Notes</th></tr>
<tr><td><b>Native-Linux Mono</b> (Unity)</td><td>✅ Attach + Launch</td><td>✅</td><td>The tested core path</td></tr>
<tr><td><b>Native-Linux IL2CPP</b></td><td>—</td><td>✅ MelonLoader / BepInEx</td><td>Loader-based</td></tr>
<tr><td><b>Proton/Windows IL2CPP</b></td><td>—</td><td>✅ Windows loader + Steam auto-setup</td><td>Uses the Windows loader build</td></tr>
<tr><td><b>Proton/Windows Mono</b></td><td>❌ can't attach</td><td>—</td><td>Its Mono runtime is a Windows DLL inside Wine; clearly flagged</td></tr>
<tr><td>Architecture</td><td colspan="3">x86_64 only</td></tr>
</table>

Verified pipelines: **Mono Attach · Mono Launch · IL2CPP MelonLoader · IL2CPP BepInEx**.
Tested on: **EndeavourOS · Linux 6.18 LTS · x86_64 · KDE / X11 · glibc 2.43**.

---

## Requirements

- **x86_64 Linux** with `pkexec` (polkit), `curl`, and a desktop portal (`xdg-open`).
- Build toolchain: **Node + pnpm**, **Rust** (stable), a **C compiler**, and the **Tauri v2** system deps (`webkit2gtk-4.1`, `gtk3`, `libsoup-3.0`, `openssl`).

---

## Build &amp; run (development)

```bash
# 0. one-time: generate the app icons from your logo
pnpm tauri icon path/to/logo.png

# 1. build everything and launch  (icons → helper .so → zyringe-inject → deps → app)
bash scripts/dev.sh
#    or build only, don't launch:
bash scripts/dev.sh --build

# 2. install the privileged bits ONCE (needed for Attach/Launch)
bash scripts/build-resources.sh     # build helper + zyringe-inject (as you)
sudo bash scripts/install.sh        # install to /usr/lib/zyringe + polkit rule (as root)
```

`install.sh` puts `zyringe-inject` + `libzyringe.so` under `/usr/lib/zyringe` (root-owned) and a polkit rule that pins that exact path. Remove with:

```bash
sudo rm -rf /usr/lib/zyringe /etc/polkit-1/rules.d/49-zyringe.rules
```

---

## Packaging (AppImage + .deb)

```bash
bash scripts/package.sh
```

Builds the helper + privileged injector, runs `tauri build`, and collects everything into **`./dist/`**:

```
dist/
├── Zyringe_<ver>_amd64.AppImage    portable GUI
├── Zyringe_<ver>_amd64.deb         GUI as a Debian/Ubuntu package
├── zyringe-inject                  privileged ptrace binary
├── libzyringe.so                   in-target helper payload
├── install.sh   49-zyringe.rules   privileged-bits installer + polkit rule
└── INSTALL.txt                     post-install steps
```

The GUI bundle is unprivileged; run `sudo bash install.sh` from `./dist/` once to install the polkit-pinned injection bits.

---

## Usage walkthroughs

<details>
<summary><b>Inject a mod into a Mono game (Attach)</b></summary>

1. Launch your Unity/Mono game; it appears in the left **Targets** list.
2. Click it (Mono games are injectable).
3. Choose your **Mod DLL** and its **entry point** — use **Browse methods** to pick a `static void` method straight from the assembly.
4. (Optional) add string **Arguments**.
5. Click **Inject** → approve the `pkexec` prompt → watch the console. On success your entry point has run; the game keeps playing.
</details>

<details>
<summary><b>Inject at launch (LD_PRELOAD)</b></summary>

1. Switch the mode toggle to **Launch**.
2. Set the **Game executable** path.
3. Choose your DLL + entry point (+ args).
4. Click **Launch with Mod** — Zyringe starts the game with the helper preloaded and reports success the moment your entry point runs.
</details>

<details>
<summary><b>Mod an IL2CPP game (MelonLoader / BepInEx)</b></summary>

1. Launch the IL2CPP game; select it — it opens the **loader panel** (not the inject form).
2. Pick a loader, then **Browse &amp; install a build** (Zyringe only shows builds that fit your game/platform) or **Install from zip**.
3. **Add plugin** → your mod `.dll`.
4. For a Steam game: close Steam, then **Set up &amp; launch via Steam** — Zyringe writes the launch option and starts the game. For non-Steam/Wine, copy the shown `WINEDLLOVERRIDES` into your launcher.
5. Verify from the loader's own log (e.g. `BepInEx/LogOutput.log`).
</details>

---

## Keyboard shortcuts

<table>
<tr><td><kbd>Ctrl</kbd> + <kbd>Enter</kbd></td><td>Inject / Launch</td></tr>
<tr><td><kbd>Esc</kbd></td><td>Cancel an in-flight injection</td></tr>
<tr><td><kbd>Ctrl</kbd> + <kbd>L</kbd></td><td>Clear the console</td></tr>
</table>

---

## Troubleshooting / FAQ

<details>
<summary><b>The Inject button is greyed out.</b></summary>
The hint next to it tells you the first missing requirement (no target selected, no DLL, missing class/method, etc.). Fill it in.
</details>

<details>
<summary><b>My IL2CPP mod didn't load.</b></summary>
Almost always the wrong loader build. For a Proton game you need a <b>Windows IL2CPP</b> build (not Linux, not the Mono/BepInEx-5 build). Zyringe now hides the wrong ones and rejects a mismatched install — reinstall with a build whose name contains <code>IL2CPP</code> and <code>win-x64</code>.
</details>

<details>
<summary><b>"Steam is running" when setting the launch option.</b></summary>
Steam rewrites its config on exit, so Zyringe refuses to edit it while Steam is open. Fully quit Steam (Steam → Exit), then try again.
</details>

<details>
<summary><b>A Windows game under Proton shows "can't attach".</b></summary>
Its Mono runtime is a Windows DLL living inside Wine — a Linux injector can't reach it. Attach/Launch work on native-Linux Mono games; use the loader flow for Proton/Windows games.
</details>

<details>
<summary><b>The game crashes on launch even without any mod.</b></summary>
That's the game, not Zyringe — check its <code>Player.log</code>. (Common culprit on old builds: expired TLS certs breaking the game's own telemetry.)
</details>

---

## Project layout

```
src/                 React + TypeScript frontend
  components/         UI (process list, inject form, loader panel, …)
src-tauri/src/        Rust app backend (scanner, injector, loader, steam, metadata, mono)
zyringe-inject/       Standalone privileged ptrace binary
helper/               C in-target payload (libzyringe.so) + Makefile
scripts/              dev / build / install / package
packaging/            polkit rule
```

---

## Contributing

Issues and PRs welcome. This is BETA — bug reports with the game, distro, loader/build, and the relevant `Player.log` / loader log are especially useful. By contributing you agree your contributions are licensed under GPL-3.0-only.

---

## Links

<table>
<tr><td>📦 <b>Source (GitHub)</b></td><td><a href="https://github.com/TheHolyOneZ/Zyringe">github.com/TheHolyOneZ/Zyringe</a></td></tr>
<tr><td>⬇️ <b>Download</b> (pre-compiled)</td><td><a href="https://zlogic.eu/zyringe/">zlogic.eu/zyringe</a> — the landing &amp; download page for ready-to-run builds (GitHub is source only)</td></tr>
<tr><td>👤 <b>Author</b></td><td><a href="https://github.com/TheHolyOneZ">@TheHolyOneZ</a></td></tr>
<tr><td>🧰 <b>More projects</b></td><td><a href="https://zsync.eu/">zsync.eu</a></td></tr>
<tr><td>🎮 <b>Game mod menus</b></td><td><a href="https://zlogic.eu/mods/">zlogic.eu/mods</a></td></tr>
</table>

## License

**GPL-3.0-only** © 2026 TheHolyOneZ. See [LICENSE](LICENSE).

---

## Disclaimer

Zyringe uses `ptrace` / `process_vm_writev` to load code into other processes — the same capability as any debugger or SharpMonoInjector. It requires admin authorization per injection and is intended for **modding games you own** and for development. x86_64 Linux, Mono/Unity targets only.
