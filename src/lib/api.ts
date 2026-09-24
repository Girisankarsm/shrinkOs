// AppVault TypeScript API layer
// All communication with the Rust backend goes through this module.
// Do NOT call invoke() directly from React components.

import { invoke } from "@tauri-apps/api/core";

// ── Types ─────────────────────────────────────────────────────────────────

export interface SystemInfo {
  os: string;
  os_version: string;
  filesystem: string;
  vault_path: string;
  app_version: string;
  milestone: number;
}

export interface DiskStats {
  total_bytes: number;
  used_bytes: number;
  available_bytes: number;
  appvault_bytes: number;
}

export interface DiscoveredApp {
  path: string;
  name: string;
  version: string | null;
  size_bytes: number;
  compatibility: "SAFE" | "CAUTION" | "UNSUPPORTED";
  is_running: boolean;
}

export interface ApplicationAnalysis {
  app_path: string;
  total_size_bytes: number;
  file_count: number;
  estimated_compressible_bytes: number;
  compressibility_ratio: number;
  files: FileEntry[];
}

export interface FileEntry {
  relative_path: string;
  size_bytes: number;
  compressibility: "high" | "medium" | "low";
  content_type: string;
}

export interface AppManifest {
  schema_version: number;
  app_id: string;
  name: string;
  version: string | null;
  platform: string;
  original_path: string;
  vault_path: string;
  original_size: number;
  stored_size: number;
  cached_size: number;
  space_saved: number;
  compression_ratio: number;
  compression: string;
  file_count: number;
  files: ManifestFile[];
  compatibility: "SAFE" | "CAUTION" | "UNSUPPORTED";
  status: AppStatus;
  created_at: string;
  last_accessed: string;
  optimized_at: string | null;
  notes: string[];
}

export interface ManifestFile {
  relative_path: string;
  original_size: number;
  stored_size: number;
  original_hash: string;
  compression: string;
  compression_level: number;
}

export type AppStatus =
  | "unmanaged"
  | "analyzing"
  | "optimizing"
  | "managed"
  | "restoring"
  | "error";

export type OptimizationPhase =
  | "analyzing"
  | "checking_space"
  | "checking_running"
  | "compressing"
  | "verifying"
  | "done"
  | "failed"
  | "cancelled";

export interface OptimizationProgress {
  app_id: string;
  phase: OptimizationPhase;
  files_total: number;
  files_done: number;
  bytes_total: number;
  bytes_done: number;
  bytes_saved: number;
  percent: number;
  current_file: string | null;
  error: string | null;
  finished: boolean;
}

export interface VaultTotals {
  managed_app_count: number;
  total_original_bytes: number;
  total_stored_bytes: number;
  total_saved_bytes: number;
}

export interface VaultStats {
  totals: VaultTotals;
  vault_path: string;
  vault_size_bytes: number;
}

export interface Config {
  compression_level: number;
  max_cache_bytes: number;
  auto_detect: boolean;
  dark_mode: boolean | null;
  show_notifications: boolean;
  parallel_tasks: number;
}

export interface AppError {
  kind: string;
  detail: string;
}

// ── API functions ──────────────────────────────────────────────────────────

export const api = {
  // System
  getSystemInfo: (): Promise<SystemInfo> => invoke("get_system_info"),
  getDiskStats: (): Promise<DiskStats> => invoke("get_disk_stats"),

  // Applications
  discoverApplications: (): Promise<DiscoveredApp[]> =>
    invoke("discover_applications"),
  analyzeApplication: (appPath: string): Promise<ApplicationAnalysis> =>
    invoke("analyze_application", { appPath }),
  getManagedApplications: (): Promise<AppManifest[]> =>
    invoke("get_managed_applications"),
  getApplicationDetail: (appId: string): Promise<AppManifest | null> =>
    invoke("get_application_detail", { appId }),

  // Optimization
  startOptimization: (appPath: string): Promise<string> =>
    invoke("start_optimization", { appPath }),
  getOptimizationProgress: (appId: string): Promise<OptimizationProgress | null> =>
    invoke("get_optimization_progress", { appId }),
  cancelOptimization: (appId: string): Promise<void> =>
    invoke("cancel_optimization", { appId }),

  // Restore
  restoreApplication: (appId: string): Promise<void> =>
    invoke("restore_application", { appId }),
  getRestoreProgress: (appId: string): Promise<OptimizationProgress | null> =>
    invoke("get_restore_progress", { appId }),

  // Settings
  getSettings: (): Promise<Config> => invoke("get_settings"),
  updateSettings: (newConfig: Config): Promise<void> =>
    invoke("update_settings", { newConfig }),

  // Vault
  getVaultStats: (): Promise<VaultStats> => invoke("get_vault_stats"),
  verifyVaultIntegrity: (): Promise<string[]> =>
    invoke("verify_vault_integrity"),
};

// ── Utility functions ─────────────────────────────────────────────────────

export function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  return `${(bytes / Math.pow(1024, i)).toFixed(2)} ${units[i]}`;
}

export function formatPercent(value: number, decimals = 1): string {
  return `${value.toFixed(decimals)}%`;
}

export function formatRatio(ratio: number): string {
  // ratio = stored / original; e.g. 0.6 → "1.67x"
  if (ratio <= 0 || ratio >= 1) return "—";
  return `${(1 / ratio).toFixed(2)}x`;
}
