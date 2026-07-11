

export interface Phase {
  key: string;
  label: string;

  match: string[];
}

export const PHASES: Phase[] = [
  { key: "elevate", label: "Elevate", match: ["requesting elevation", "pkexec"] },
  { key: "attach", label: "Attach", match: ["stopping target", "stopped ", "hijacking"] },
  { key: "alloc", label: "Allocate", match: ["resolving libc", "allocated"] },
  { key: "load", label: "Load", match: ["loading helper", "helper armed", "loaded assembly", "loaded "] },
  { key: "invoke", label: "Invoke", match: ["worker thread waiting", "waiting for managed", "managed entry point executed"] },
  { key: "done", label: "Done", match: ["injection succeeded", "✓"] },
];


export function reachedPhase(lines: string[]): number {
  let reached = -1;
  const hay = lines.map((l) => l.toLowerCase());
  PHASES.forEach((p, i) => {
    if (hay.some((line) => p.match.some((m) => line.includes(m)))) {
      reached = Math.max(reached, i);
    }
  });
  return reached;
}
