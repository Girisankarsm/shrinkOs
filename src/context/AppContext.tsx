import React, { createContext, useContext, useState, useEffect, useCallback, ReactNode } from 'react';
import { api, SystemInfo, DiskStats, AppManifest, DiscoveredApp, VaultStats } from '../lib/api';

// ── Types ─────────────────────────────────────────────────────────────────

interface AppContextValue {
  systemInfo: SystemInfo | null;
  diskStats: DiskStats | null;
  managedApps: AppManifest[];
  discoveredApps: DiscoveredApp[];
  setDiscoveredApps: (apps: DiscoveredApp[]) => void;
  vaultStats: VaultStats | null;
  isLoading: boolean;
  error: string | null;
  refresh: () => Promise<void>;
  theme: 'dark' | 'light';
  setTheme: (t: 'dark' | 'light') => void;
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
  const [vaultStats, setVaultStats] = useState<VaultStats | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [theme, setThemeState] = useState<'dark' | 'light'>('dark');

  const setTheme = useCallback((t: 'dark' | 'light') => {
    setThemeState(t);
    document.documentElement.setAttribute('data-theme', t);
    localStorage.setItem('appvault-theme', t);
  }, []);

  const setDiscoveredApps = useCallback((apps: DiscoveredApp[]) => {
    setDiscoveredAppsState(apps);
    localStorage.setItem('appvault-discovered-apps', JSON.stringify(apps));
  }, []);

  // Restore theme from localStorage
  useEffect(() => {
    const saved = localStorage.getItem('appvault-theme') as 'dark' | 'light' | null;
    if (saved) {
      setTheme(saved);
    }
  }, [setTheme]);

  const refresh = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const [info, disk, apps, vault] = await Promise.all([
        api.getSystemInfo(),
        api.getDiskStats(),
        api.getManagedApplications(),
        api.getVaultStats(),
      ]);
      setSystemInfo(info);
      setDiskStats(disk);
      setManagedApps(apps);
      setVaultStats(vault);
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

  return (
    <AppContext.Provider value={{
      systemInfo, diskStats, managedApps, vaultStats,
      discoveredApps, setDiscoveredApps,
      isLoading, error, refresh, theme, setTheme,
    }}>
      {children}
    </AppContext.Provider>
  );
}
