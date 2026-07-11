export interface MonoProcess {
  pid: number;
  name: string;
  cmdline: string;
  flavor: string;
  mono_so_path: string;
  data_dir: string | null;
  game_dir: string | null;
  exe_path: string | null;
  thread_count: number;
  suspect: boolean;
  engine: string;
  injectable: boolean;
  duplicate: boolean;
  app_id: string | null;
  proton: boolean;
}

export interface EntryPoint {
  namespace: string;
  class: string;
  method: string;
  is_static: boolean;
  param_count: number;
  returns_void: boolean;
}

export type InjectMode = "attach" | "launch";

export type RunState = "idle" | "running" | "ok" | "err";

export type LoaderKind = "MelonLoader" | "BepInEx";

export interface LoaderStatus {
  installed: boolean;
  plugins_dir: string;
  plugins: string[];
  run_script: string | null;
  windows_proxy: boolean;
}

export interface LoaderAsset {
  name: string;
  url: string;
  size: number;
  tag: string;
  prerelease: boolean;
}
