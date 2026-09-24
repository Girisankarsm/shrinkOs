import React from 'react';
import { formatBytes } from '../lib/api';
import { useAppContext } from '../context/AppContext';

export default function Dashboard() {
  const { diskStats, vaultStats, managedApps, isLoading, error } = useAppContext();

  if (isLoading) {
    return (
      <div className="page-body" style={{ display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
        <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 'var(--space-4)' }}>
          <div className="spinner spinner-lg" />
          <span className="text-secondary text-sm">Loading AppVault…</span>
        </div>
      </div>
    );
  }

  const total = diskStats?.total_bytes ?? 0;
  const used = diskStats?.used_bytes ?? 0;
  const available = diskStats?.available_bytes ?? 0;
  const vaultBytes = diskStats?.appvault_bytes ?? 0;
  const originalBytes = vaultStats?.totals.total_original_bytes ?? 0;
  const storedBytes = vaultStats?.totals.total_stored_bytes ?? 0;
  const savedBytes = vaultStats?.totals.total_saved_bytes ?? 0;
  const managedCount = vaultStats?.totals.managed_app_count ?? 0;

  const usedFraction = total > 0 ? (used / total) : 0;
  const vaultFraction = total > 0 ? (vaultBytes / total) : 0;
  const displayBytes = (bytes: number) => error ? '—' : formatBytes(bytes);

  return (
    <>
      <header className="page-header">
        <div>
          <h1 className="page-title">Dashboard</h1>
          <div className="page-subtitle">Storage overview and managed applications</div>
        </div>
        <RefreshButton />
      </header>

      <div className="page-body">
        {error && (
          <div className="alert alert-error mb-6">
            <span>⚠</span>
            <div>
              <strong>AppVault data is unavailable</strong>
              <div>{error}</div>
              <div style={{ marginTop: 4 }}>
                Launch the native Tauri app with <strong>npm run desktop</strong>; the browser preview cannot access macOS disk or application bundles.
              </div>
            </div>
          </div>
        )}

        {/* Milestone banner */}
        <div className="milestone-banner">
          <span>⚡</span>
          <span>
            <strong>Milestone 1</strong> — Compression engine active · Chunking &amp; deduplication coming in M2
          </span>
        </div>

        {/* Stat cards */}
        <div className="stat-grid">
          <StatCard label="Total Disk" value={displayBytes(total)} />
          <StatCard label="Used" value={displayBytes(used)} sub={error ? 'Unavailable' : `${((usedFraction) * 100).toFixed(1)}% of total`} />
          <StatCard label="Available" value={displayBytes(available)} variant="accent" />
          <StatCard label="AppVault Size" value={displayBytes(vaultBytes)} />
          <StatCard label="Original Apps" value={displayBytes(originalBytes)} />
          <StatCard
            label="Space Saved"
            value={error ? '—' : savedBytes > 0 ? formatBytes(savedBytes) : '—'}
            variant="success"
            sub={error ? 'Unavailable' : savedBytes > 0 && originalBytes > 0 ? `${((savedBytes / originalBytes) * 100).toFixed(1)}% reduction` : 'Optimize apps to start saving'}
          />
        </div>

        {/* Storage bar */}
        <div className="card mb-6">
          <div className="card-title">Disk Usage</div>
          <div className="storage-bar-track">
            <div
              className="storage-bar-segment used"
              style={{ width: `${Math.min(usedFraction * 100, 100)}%` }}
            />
            <div
              className="storage-bar-segment vault"
              style={{ width: `${Math.min(vaultFraction * 100, 100)}%` }}
            />
          </div>
          <div className="storage-bar-legend">
            <div className="legend-item">
              <div className="legend-dot" style={{ background: 'hsl(220, 10%, 40%)' }} />
              Used — {displayBytes(used)}
            </div>
            <div className="legend-item">
              <div className="legend-dot" style={{ background: 'var(--color-accent-500)' }} />
              AppVault — {displayBytes(vaultBytes)}
            </div>
            <div className="legend-item">
              <div className="legend-dot" style={{ background: 'var(--color-bg-overlay)' }} />
              Free — {displayBytes(available)}
            </div>
          </div>
        </div>

        {/* Managed apps summary */}
        {managedCount === 0 ? (
          <div className="card">
            <div className="empty-state" style={{ padding: 'var(--space-10)' }}>
              <div className="empty-icon">◫</div>
              <div className="empty-title">No applications managed yet</div>
              <div className="empty-description">
                Go to <strong>Applications</strong> to discover and optimize installed apps.
                AppVault uses real Zstd compression — savings depend on the application's content.
              </div>
            </div>
          </div>
        ) : (
          <div className="app-table-container">
            <div style={{ padding: 'var(--space-5)', borderBottom: '1px solid var(--color-border-subtle)' }}>
              <div className="card-title" style={{ marginBottom: 0 }}>Managed Applications</div>
            </div>
            <table className="app-table">
              <thead>
                <tr>
                  <th>Application</th>
                  <th>Original</th>
                  <th>Stored</th>
                  <th>Saved</th>
                  <th>Ratio</th>
                  <th>Status</th>
                </tr>
              </thead>
              <tbody>
                {managedApps.map(app => (
                  <tr key={app.app_id}>
                    <td>
                      <div className="app-name-cell">
                        <div className="app-icon">📦</div>
                        <div>
                          <div className="app-name">{app.name}</div>
                          {app.version && <div className="app-version">v{app.version}</div>}
                        </div>
                      </div>
                    </td>
                    <td className="font-mono text-sm">{formatBytes(app.original_size)}</td>
                    <td className="font-mono text-sm">{formatBytes(app.stored_size)}</td>
                    <td className="font-mono text-sm text-success">{formatBytes(app.space_saved)}</td>
                    <td className="font-mono text-sm">
                      {app.compression_ratio < 1
                        ? `${(1 / app.compression_ratio).toFixed(2)}x`
                        : '—'}
                    </td>
                    <td>
                      <StatusBadge status={app.status} />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </>
  );
}

// ── Sub-components ─────────────────────────────────────────────────────────

function StatCard({
  label, value, sub, variant,
}: {
  label: string;
  value: string;
  sub?: string;
  variant?: 'accent' | 'success' | 'warning';
}) {
  return (
    <div className="stat-card">
      <div className="stat-label">{label}</div>
      <div className={`stat-value ${variant ?? ''}`}>{value}</div>
      {sub && <div className="stat-sub">{sub}</div>}
    </div>
  );
}

function StatusBadge({ status }: { status: string }) {
  const map: Record<string, { cls: string; label: string }> = {
    managed:    { cls: 'badge-managed',    label: 'Managed' },
    unmanaged:  { cls: '',                 label: 'Unmanaged' },
    optimizing: { cls: 'badge-caution',    label: 'Optimizing…' },
    analyzing:  { cls: 'badge-caution',    label: 'Analyzing…' },
    restoring:  { cls: 'badge-caution',    label: 'Restoring…' },
    error:      { cls: 'badge-unsupported',label: 'Error' },
  };
  const info = map[status] ?? { cls: '', label: status };
  return <span className={`badge ${info.cls}`}>{info.label}</span>;
}

function RefreshButton() {
  const { refresh, isLoading } = useAppContext();
  return (
    <button
      id="btn-refresh-dashboard"
      className="btn btn-secondary btn-sm"
      onClick={refresh}
      disabled={isLoading}
      title="Refresh statistics"
    >
      {isLoading ? <span className="spinner" style={{ width: 14, height: 14 }} /> : '↻'}
      Refresh
    </button>
  );
}
