export type Page = 'overview' | 'growth' | 'explore' | 'settings';

export type Snapshot = {
  file: string;
  taken_at_utc: number;
  scan_root: string;
  alloc_bytes: number;
  volume_free: number;
  unreadable_dirs: number;
};

export type Hotspot = {
  path: string;
  name: string;
  delta_bytes: number;
  old_bytes: number;
  new_bytes: number;
  label: string | null;
  hint: string | null;
};

export type Diff = {
  from_utc: number;
  to_utc: number;
  growth_bytes: number;
  free_delta_bytes: number;
  hotspots: Hotspot[];
};

export type Settings = {
  scanRoot: string;
  theme: 'system' | 'dark' | 'light';
  retention: number;
  exclusions: string[];
  schedule: boolean;
};

export type ExploreChild = {
  path: string;
  name: string;
  bytes: number;
  files: number;
  delta: number | null;
};

export type BigFile = { path: string; name: string; bytes: number };

export type ScanProgress = { files: number; bytes: number; directories: number; current_dir: string };
export type ScanDone = { file: string | null; error: string | null; unreadableDirs: number };
