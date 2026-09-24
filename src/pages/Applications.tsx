import React, { useState, useCallback } from 'react';
import { api, DiscoveredApp, ApplicationAnalysis, OptimizationProgress, formatBytes, formatApiError } from '../lib/api';
import { useAppContext } from '../context/AppContext';

type Phase = 'list' | 'analyzing' | 'confirm' | 'progress' | 'done' | 'error';

export default function ApplicationsPage() {
  const { managedApps, discoveredApps, setDiscoveredApps, refresh } = useAppContext();
  const [scanning, setScanning] = useState(false);
  const [hasScanned, setHasScanned] = useState(false);
  const [selectedApp, setSelectedApp] = useState<DiscoveredApp | null>(null);
  const [analysis, setAnalysis] = useState<ApplicationAnalysis | null>(null);
  const [phase, setPhase] = useState<Phase>('list');
  const [progress, setProgress] = useState<OptimizationProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [activeAppId, setActiveAppId] = useState<string | null>(null);

  // ── Discover ──────────────────────────────────────────────────────
  const handleScan = useCallback(async () => {
    setScanning(true);
    setError(null);
    try {
      const apps = await api.discoverApplications();
      setDiscoveredApps(apps.filter(a => a.compatibility !== 'UNSUPPORTED'));
      setHasScanned(true);
    } catch (e: unknown) {
      setError(formatApiError(e));
    } finally {
      setScanning(false);
    }
  }, []);

  // ── Analyze ───────────────────────────────────────────────────────
  const handleAnalyze = useCallback(async (app: DiscoveredApp) => {
    if (app.is_running) {
      setError(`${app.name} is currently running. Quit it before optimizing.`);
      return;
    }
    setSelectedApp(app);
    setPhase('analyzing');
    setError(null);
    try {
      const result = await api.analyzeApplication(app.path);
      setAnalysis(result);
      setPhase('confirm');
    } catch (e: unknown) {
      setError(formatApiError(e));
      setPhase('error');
    }
  }, []);

  // ── Optimize ──────────────────────────────────────────────────────
  const handleOptimize = useCallback(async () => {
    if (!selectedApp) return;
    setPhase('progress');
    setError(null);
    try {
      const appId = await api.startOptimization(selectedApp.path);
      setActiveAppId(appId);

      // Poll progress every 500 ms.
      const interval = setInterval(async () => {
        const prog = await api.getOptimizationProgress(appId);
        if (prog) {
          setProgress(prog);
          if (prog.finished) {
            clearInterval(interval);
            setPhase(prog.phase === 'done' ? 'done' : 'error');
            if (prog.error) setError(prog.error);
            await refresh();
          }
        }
      }, 500);
    } catch (e: unknown) {
      setError(formatApiError(e));
      setPhase('error');
    }
  }, [selectedApp, refresh]);

  const handleCancel = useCallback(async () => {
    if (activeAppId) {
      await api.cancelOptimization(activeAppId);
    }
  }, [activeAppId]);

  const handleReset = useCallback(() => {
    setPhase('list');
    setSelectedApp(null);
    setAnalysis(null);
    setProgress(null);
    setError(null);
    setActiveAppId(null);
  }, []);

  // ── Render ────────────────────────────────────────────────────────
  return (
    <>
      <header className="page-header">
        <div>
          <h1 className="page-title">Applications</h1>
          <div className="page-subtitle">Discover and optimize installed applications</div>
        </div>
        {phase === 'list' && (
          <button
            id="btn-scan-apps"
            className="btn btn-primary"
            onClick={handleScan}
            disabled={scanning}
          >
            {scanning
              ? <><span className="spinner" style={{ width: 14, height: 14 }} /> Scanning…</>
              : '⊕ Scan for Applications'
            }
          </button>
        )}
        {phase !== 'list' && (
          <button className="btn btn-ghost" onClick={handleReset}>
            ← Back
          </button>
        )}
      </header>

      <div className="page-body">
        {error && (
          <div className="alert alert-error mb-6">
            <span>⚠</span>
            <div>
              <strong>Error</strong>
              <div>{error}</div>
            </div>
          </div>
        )}

        {/* Phase: List */}
        {phase === 'list' && (
          <ApplicationList
            discovered={discoveredApps}
            managedApps={managedApps}
            onAnalyze={handleAnalyze}
            hasScanned={hasScanned}
            scanning={scanning}
          />
        )}

        {/* Phase: Analyzing */}
        {phase === 'analyzing' && selectedApp && (
          <div className="card" style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 'var(--space-6)', padding: 'var(--space-12)' }}>
            <div className="spinner spinner-lg" />
            <div>
              <div className="text-lg font-semibold" style={{ marginBottom: 'var(--space-2)', textAlign: 'center' }}>
                Analyzing {selectedApp.name}
              </div>
              <div className="text-sm text-secondary" style={{ textAlign: 'center' }}>
                Walking file tree, classifying content types, estimating compression savings…
              </div>
            </div>
          </div>
        )}

        {/* Phase: Confirm */}
        {phase === 'confirm' && selectedApp && analysis && (
          <AnalysisConfirm
            app={selectedApp}
            analysis={analysis}
            onConfirm={handleOptimize}
            onCancel={handleReset}
          />
        )}

        {/* Phase: Progress */}
        {phase === 'progress' && selectedApp && (
          <OptimizationProgressCard
            app={selectedApp}
            progress={progress}
            onCancel={handleCancel}
          />
        )}

        {/* Phase: Done */}
        {phase === 'done' && progress && selectedApp && (
          <DoneCard
            appName={selectedApp.name}
            progress={progress}
            onBack={handleReset}
          />
        )}
      </div>
    </>
  );
}

// ── Sub-components ─────────────────────────────────────────────────────────

function ApplicationList({
  discovered, managedApps, onAnalyze, hasScanned, scanning,
}: {
  discovered: DiscoveredApp[];
  managedApps: ReturnType<typeof useAppContext>['managedApps'];
  onAnalyze: (app: DiscoveredApp) => void;
  hasScanned: boolean;
  scanning: boolean;
}) {
  const restoredPaths = new Set(
    managedApps
      .filter(app => app.status === 'unmanaged')
      .map(app => app.original_path),
  );
  const managedPaths = new Set(
    managedApps
      .filter(app => app.status === 'managed' && !restoredPaths.has(app.original_path))
      .map(app => app.original_path),
  );

  if (scanning && discovered.length === 0) {
    return (
      <div className="empty-state scan-state" aria-live="polite">
        <div className="scan-orbit" aria-hidden="true">
          <div className="scan-orbit-dot" />
        </div>
        <div className="empty-title">Scanning applications</div>
        <div className="empty-description">
          Looking through standard macOS application locations…
        </div>
      </div>
    );
  }

  if (!hasScanned) {
    return (
      <div className="empty-state">
        <div className="empty-icon">◫</div>
        <div className="empty-title">No applications scanned</div>
        <div className="empty-description">
          Click <strong>Scan for Applications</strong> to discover installed apps
          in standard system locations. ShrinkOS will never modify any app without your explicit confirmation.
        </div>
      </div>
    );
  }

  if (discovered.length === 0) {
    return (
      <div className="empty-state">
        <div className="empty-icon">◫</div>
        <div className="empty-title">No applications found</div>
        <div className="empty-description">
          No supported applications were found in standard locations.
        </div>
      </div>
    );
  }

  return (
    <div className="app-table-container">
      <div style={{ padding: 'var(--space-5)', borderBottom: '1px solid var(--color-border-subtle)', display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <div className="card-title" style={{ marginBottom: 0 }}>
          Discovered Applications ({discovered.length})
        </div>
        <div className="text-xs text-muted">
          Only SAFE-rated apps are shown
        </div>
      </div>
      <table className="app-table">
        <thead>
          <tr>
            <th>Application</th>
            <th>Size</th>
            <th>Compatibility</th>
            <th>Status</th>
            <th>Action</th>
          </tr>
        </thead>
        <tbody>
          {discovered.map(app => {
            const isManaged = managedPaths.has(app.path);
            return (
              <tr key={app.path}>
                <td>
                  <div className="app-name-cell">
                    <div className="app-icon">
                      {app.is_running ? '▶' : '📦'}
                    </div>
                    <div>
                      <div className="app-name">{app.name}</div>
                      {app.version && <div className="app-version">v{app.version}</div>}
                    </div>
                  </div>
                </td>
                <td className="font-mono text-sm">{formatBytes(app.size_bytes)}</td>
                <td>
                  <CompatBadge compat={app.compatibility} />
                </td>
                <td>
                  {app.is_running && <span className="badge badge-running">● Running</span>}
                  {isManaged && <span className="badge badge-managed">✓ Managed</span>}
                </td>
                <td>
                  <button
                    id={`btn-optimize-${app.name.replace(/\s+/g, '-').toLowerCase()}`}
                    className="btn btn-primary btn-sm"
                    disabled={app.is_running || isManaged}
                    onClick={() => onAnalyze(app)}
                    title={
                      app.is_running ? 'Quit the app first' :
                      isManaged ? 'Already managed' :
                      'Analyze and optimize'
                    }
                  >
                    {isManaged ? 'Managed' : 'Optimize →'}
                  </button>
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

function CompatBadge({ compat }: { compat: string }) {
  if (compat === 'SAFE') return <span className="badge badge-safe">✓ Safe</span>;
  if (compat === 'CAUTION') return <span className="badge badge-caution">⚠ Caution</span>;
  return <span className="badge badge-unsupported">✗ Unsupported</span>;
}

function AnalysisConfirm({
  app, analysis, onConfirm, onCancel,
}: {
  app: DiscoveredApp;
  analysis: ApplicationAnalysis;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const ratio = analysis.compressibility_ratio;
  const estimatedSaved = analysis.estimated_compressible_bytes * 0.5;

  return (
    <div className="card">
      <div style={{ marginBottom: 'var(--space-6)' }}>
        <div className="text-xl font-bold" style={{ marginBottom: 'var(--space-2)' }}>
          Analysis Complete: {app.name}
        </div>
        <div className="text-sm text-secondary">
          Review the analysis before proceeding. Actual savings may differ from estimates.
        </div>
      </div>

      {/* Key stats */}
      <div className="stat-grid" style={{ marginBottom: 'var(--space-6)' }}>
        <div className="stat-card">
          <div className="stat-label">Total Size</div>
          <div className="stat-value">{formatBytes(analysis.total_size_bytes)}</div>
        </div>
        <div className="stat-card">
          <div className="stat-label">Files</div>
          <div className="stat-value">{analysis.file_count.toLocaleString()}</div>
        </div>
        <div className="stat-card">
          <div className="stat-label">Compressible</div>
          <div className="stat-value accent">{(ratio * 100).toFixed(0)}%</div>
          <div className="stat-sub">of files benefit from compression</div>
        </div>
        <div className="stat-card">
          <div className="stat-label">Est. Savings</div>
          <div className="stat-value success">
            {estimatedSaved > 0 ? `~${formatBytes(estimatedSaved)}` : 'Minimal'}
          </div>
          <div className="stat-sub">actual savings measured during compression</div>
        </div>
      </div>

      {/* Honest disclaimer */}
      {ratio < 0.3 && (
        <div className="alert alert-warning" style={{ marginBottom: 'var(--space-6)' }}>
          <span>⚠</span>
          <div>
            <strong>Low compressibility detected</strong>
            <div style={{ marginTop: 4 }}>
              {(ratio * 100).toFixed(0)}% of this application's files are already compressed
              (images, videos, archives, encrypted data). Compression savings may be minimal or negligible.
              ShrinkOS will still store the accurate result after measuring.
            </div>
          </div>
        </div>
      )}

      <div className="alert alert-info" style={{ marginBottom: 'var(--space-6)' }}>
        <span>ℹ</span>
        <div>
          <strong>Safety guarantee</strong>
          <div style={{ marginTop: 4 }}>
            ShrinkOS will compress files into its vault storage. Your original application
            remains fully intact. You can restore it at any time from the Applications page.
          </div>
        </div>
      </div>

      <div className="modal-footer" style={{ padding: 0, border: 'none' }}>
        <button id="btn-cancel-optimize" className="btn btn-secondary" onClick={onCancel}>
          Cancel
        </button>
        <button id="btn-confirm-optimize" className="btn btn-primary" onClick={onConfirm}>
          ⚡ Start Optimization
        </button>
      </div>
    </div>
  );
}

function OptimizationProgressCard({
  app, progress, onCancel,
}: {
  app: DiscoveredApp;
  progress: OptimizationProgress | null;
  onCancel: () => void;
}) {
  const pct = progress?.percent ?? 0;
  const phase = progress?.phase ?? 'analyzing';

  const phaseLabel: Record<string, string> = {
    analyzing:       'Analyzing application…',
    checking_space:  'Checking available storage…',
    checking_running:'Verifying application is not running…',
    compressing:     'Compressing files…',
    verifying:       'Verifying integrity…',
    done:            'Complete',
    failed:          'Failed',
    cancelled:       'Cancelled',
  };

  return (
    <div className="progress-container">
      <div className="progress-title">
        <div className="spinner" />
        Optimizing {app.name}
      </div>

      <div className="text-sm text-secondary" style={{ marginBottom: 'var(--space-4)' }}>
        {phaseLabel[phase] ?? phase}
      </div>

      <div className="progress-bar-track">
        <div className="progress-bar-fill" style={{ width: `${pct}%` }} />
      </div>

      <div style={{ display: 'flex', justifyContent: 'space-between', marginTop: 'var(--space-2)' }}>
        <span className="text-xs text-muted font-mono">{pct.toFixed(1)}%</span>
        {progress && (
          <span className="text-xs text-muted font-mono">
            {progress.files_done.toLocaleString()} / {progress.files_total.toLocaleString()} files
          </span>
        )}
      </div>

      {progress && (
        <div className="progress-stats">
          <div>
            <div className="progress-stat-label">Bytes Processed</div>
            <div className="progress-stat-value">{formatBytes(progress.bytes_done)}</div>
          </div>
          <div>
            <div className="progress-stat-label">Saved So Far</div>
            <div className="progress-stat-value text-success">{formatBytes(progress.bytes_saved)}</div>
          </div>
          <div>
            <div className="progress-stat-label">Total Size</div>
            <div className="progress-stat-value">{formatBytes(progress.bytes_total)}</div>
          </div>
        </div>
      )}

      {progress?.current_file && (
        <div className="progress-current-file">
          ↳ {progress.current_file}
        </div>
      )}

      <div style={{ marginTop: 'var(--space-6)', display: 'flex', justifyContent: 'flex-end' }}>
        <button
          id="btn-cancel-running"
          className="btn btn-secondary btn-sm"
          onClick={onCancel}
        >
          Cancel
        </button>
      </div>
    </div>
  );
}

function DoneCard({
  appName, progress, onBack,
}: {
  appName: string;
  progress: OptimizationProgress;
  onBack: () => void;
}) {
  return (
    <div className="card" style={{ textAlign: 'center', padding: 'var(--space-12)' }}>
      <div style={{ fontSize: 48, marginBottom: 'var(--space-4)' }}>✅</div>
      <div className="text-xl font-bold" style={{ marginBottom: 'var(--space-2)' }}>
        {appName} optimized successfully
      </div>
      <div className="text-sm text-secondary" style={{ marginBottom: 'var(--space-8)' }}>
        {progress.files_done.toLocaleString()} files compressed ·{' '}
        <span className="text-success font-semibold">{formatBytes(progress.bytes_saved)} saved</span>
      </div>
      <div style={{ display: 'flex', gap: 'var(--space-4)', justifyContent: 'center' }}>
        <button id="btn-done-back" className="btn btn-primary" onClick={onBack}>
          ← Back to Applications
        </button>
      </div>
    </div>
  );
}
