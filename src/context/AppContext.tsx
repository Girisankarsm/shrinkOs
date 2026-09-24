import React, { createContext, useContext, useState, useEffect, useCallback, ReactNode } from 'react';
import { api, SystemInfo, DiskStats, AppManifest, DiscoveredApp, VaultStats, PermissionStatus } from '../lib/api';

// ── Types ─────────────────────────────────────────────────────────────────

interface AppContextValue {
  systemInfo: SystemInfo | null;
  diskStats: DiskStats | null;
  managedApps: AppManifest[];
  discoveredApps: DiscoveredApp[];
  setDiscoveredApps: (apps: DiscoveredApp[]) => void;
  hasScanned: boolean;
  setHasScanned: (value: boolean) => void;
  vaultStats: VaultStats | null;
  permissionStatus: PermissionStatus | null;
  isLoading: boolean;
  error: string | null;
  refresh: (silent?: boolean) => Promise<void>;
}

// ── Context ───────────────────────────────────────────────────────────────

const AppContext = createContext<AppContextValue | null>(null);

export function useAppContext(): AppContextValue {
  const ctx = useContext(AppContext);
  if (!ctx) throw new Error('useAppContext must be used within AppProvider');
  return ctx;
}

// ── Provider ──────────────────────────────────────────────────────────────

export function AppProvider({ children }: { children: ReactNode }) {
  const [systemInfo, setSystemInfo] = useState<SystemInfo | null>(null);
  const [diskStats, setDiskStats] = useState<DiskStats | null>(null);
  const [managedApps, setManagedApps] = useState<AppManifest[]>([]);
  const [discoveredApps, setDiscoveredAppsState] = useState<DiscoveredApp[]>(() => {
    try {
      const saved = localStorage.getItem('appvault-discovered-apps');
      return saved ? JSON.parse(saved) as DiscoveredApp[] : [];
    } catch {
      return [];
    }
  });
  const [hasScanned, setHasScannedState] = useState<boolean>(() => {
    return localStorage.getItem('shrinkos-has-scanned') === '1';
  });
  const [vaultStats, setVaultStats] = useState<VaultStats | null>(null);
  const [permissionStatus, setPermissionStatus] = useState<PermissionStatus | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const setDiscoveredApps = useCallback((apps: DiscoveredApp[]) => {
    setDiscoveredAppsState(apps);
    localStorage.setItem('appvault-discovered-apps', JSON.stringify(apps));
  }, []);

  const setHasScanned = useCallback((value: boolean) => {
    setHasScannedState(value);
    localStorage.setItem('shrinkos-has-scanned', value ? '1' : '0');
  }, []);

  const refresh = useCallback(async (silent = false) => {
    if (!silent) setIsLoading(true);
    setError(null);
    try {
      const [info, disk, apps, vault, permissions] = await Promise.all([
        api.getSystemInfo(),
        api.getDiskStats(),
        api.getManagedApplications(),
        api.getVaultStats(),
        api.getPermissionStatus(),
      ]);
      setSystemInfo(info);
      setDiskStats(disk);
      setManagedApps(apps);
      setVaultStats(vault);
      setPermissionStatus(permissions);
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      setError(`Failed to load ShrinkOS data: ${msg}`);
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  useEffect(() => {
    const refreshWhenVisible = () => {
      if (document.visibilityState === 'visible') void refresh(true);
    };
    const interval = window.setInterval(refreshWhenVisible, 5000);
    document.addEventListener('visibilitychange', refreshWhenVisible);
    window.addEventListener('focus', refreshWhenVisible);

    return () => {
      window.clearInterval(interval);
      document.removeEventListener('visibilitychange', refreshWhenVisible);
      window.removeEventListener('focus', refreshWhenVisible);
    };
  }, [refresh]);

  return (
    <AppContext.Provider value={{
      systemInfo, diskStats, managedApps, vaultStats, permissionStatus,
      discoveredApps, setDiscoveredApps, hasScanned, setHasScanned,
      isLoading, error, refresh,
    }}>
      {children}
    </AppContext.Provider>
  );
}
