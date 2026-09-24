import React, { useState, useCallback } from 'react';
import { api, AppManifest, OptimizationProgress, formatBytes, formatApiError } from '../lib/api';
import { useAppContext } from '../context/AppContext';

export default function OptimizePage() {
  const { managedApps, refresh } = useAppContext();
  const activeApps = managedApps.filter(app => app.status === 'managed');
  const restoredApps = managedApps.filter(app => app.status === 'unmanaged');
  const [restoring, setRestoring] = useState<string | null>(null);
  const [progress, setProgress] = useState<OptimizationProgress | null>(null);
  const [restored, setRestored] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const handleRestore = useCallback(async (app: AppManifest) => {
    setRestoring(app.app_id);
    setError(null);
    setProgress(null);
    setRestored(null);

    try {
      // Start restore
      await api.restoreApplication(app.app_id);

      // Poll progress
      const interval = setInterval(async () => {
        const prog = await api.getRestoreProgress(app.app_id);
        if (prog) {
          setProgress(prog);
          if (prog.finished) {
            clearInterval(interval);
            setRestoring(null);
            setRestored(app.name);
            await refresh();
          }
        }
      }, 500);
    } catch (e: unknown) {
      setError(formatApiError(e));
      setRestoring(null);
    }
  }, [refresh]);

  return (
    <>
      <header className="page-header">
        <div>
          <h1 className="page-title">Restore</h1>
          <div className="page-subtitle">Restore managed applications to their original state</div>
        </div>
      </header>

      <div className="page-body">
        {error && (
          <div className="alert alert-error mb-6">
            <span>⚠</span>
            <div>{error}</div>
          </div>
        )}

        {restored && (
          <div className="alert alert-success mb-6">
            <span>✓</span>
            <div><strong>{restored}</strong> has been fully restored to its original location.</div>
          </div>
        )}

        {restoring && progress && (
          <div className="progress-container mb-6">
            <div className="progress-title">
              <div className="spinner" />
              Restoring…
            </div>
            <div className="progress-bar-track">
              <div className="progress-bar-fill" style={{ width: `${progress.percent}%` }} />
            </div>
            <div style={{ display: 'flex', justifyContent: 'space-between', marginTop: 8 }}>
              <span className="text-xs text-muted font-mono">{progress.percent.toFixed(1)}%</span>
              <span className="text-xs text-muted font-mono">
                {progress.files_done} / {progress.files_total} files
              </span>
            </div>
          </div>
        )}

        {activeApps.length === 0 ? (
          <div className="empty-state">
            <div className="empty-icon">◈</div>
            <div className="empty-title">No managed applications</div>
            <div className="empty-description">
              Once you optimize applications, they will appear here and can be restored at any time.
            </div>
          </div>
        ) : (
          <div className="app-table-container">
            <div style={{ padding: 'var(--space-5)', borderBottom: '1px solid var(--color-border-subtle)' }}>
              <div className="card-title" style={{ marginBottom: 0 }}>
                Managed Applications ({activeApps.length})
              </div>
            </div>
            <table className="app-table">
              <thead>
                <tr>
                  <th>Application</th>
                  <th>Original Size</th>
                  <th>Stored Size</th>
                  <th>Space Saved</th>
                  <th>Ratio</th>
                  <th>Action</th>
                </tr>
              </thead>
              <tbody>
                {activeApps.map(app => (
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
                      {app.compression_ratio < 1 ? `${(1 / app.compression_ratio).toFixed(2)}x` : '—'}
                    </td>
                    <td>
                      <button
                        id={`btn-restore-${app.app_id}`}
                        className="btn btn-secondary btn-sm"
                        disabled={restoring !== null}
                        onClick={() => handleRestore(app)}
                      >
                        {restoring === app.app_id
                          ? <><span className="spinner" style={{ width: 12, height: 12 }} /> Restoring…</>
                          : '↩ Restore'
                        }
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}

        {restoredApps.length > 0 && (
          <div className="app-table-container" style={{ marginTop: 'var(--space-6)' }}>
            <div style={{ padding: 'var(--space-5)', borderBottom: '1px solid var(--color-border-subtle)' }}>
              <div className="card-title" style={{ marginBottom: 0 }}>
                Restored Applications ({restoredApps.length})
              </div>
            </div>
            <table className="app-table">
              <thead>
                <tr>
                  <th>Application</th>
                  <th>Original Size</th>
                  <th>Restored Location</th>
                  <th>Status</th>
                </tr>
              </thead>
              <tbody>
                {restoredApps.map(app => (
                  <tr key={`restored-${app.app_id}`}>
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
                    <td className="font-mono text-xs">{app.original_path}</td>
                    <td><span className="badge badge-safe">✓ Restored</span></td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}

        {/* Integrity check */}
        <IntegrityCheck />
      </div>
    </>
  );
}

function IntegrityCheck() {
  const [checking, setChecking] = useState(false);
  const [result, setResult] = useState<string[] | null>(null);

  const handleCheck = async () => {
    setChecking(true);
    setResult(null);
    try {
      const failed = await api.verifyVaultIntegrity();
      setResult(failed);
    } finally {
      setChecking(false);
    }
  };

  return (
    <div className="card" style={{ marginTop: 'var(--space-6)' }}>
      <div className="card-title">Vault Integrity</div>
      <div className="text-sm text-secondary" style={{ marginBottom: 'var(--space-4)' }}>
        Verify that all stored files pass their Blake3 hash checks. This re-decompresses
        every stored file and compares it to the recorded hash — no data is modified.
      </div>
      <button
        id="btn-verify-integrity"
        className="btn btn-secondary"
        onClick={handleCheck}
        disabled={checking}
      >
        {checking ? <><span className="spinner" style={{ width: 14, height: 14 }} /> Verifying…</> : '⊕ Verify Integrity'}
      </button>

      {result !== null && (
        <div style={{ marginTop: 'var(--space-4)' }}>
          {result.length === 0 ? (
            <div className="alert alert-success">
              <span>✓</span>
              <div>All stored files passed integrity verification. No corruption detected.</div>
            </div>
          ) : (
            <div className="alert alert-error">
              <span>⚠</span>
              <div>
                <strong>{result.length} application(s) failed integrity check:</strong>
                <ul style={{ marginTop: 8, paddingLeft: 16 }}>
                  {result.map(id => <li key={id} className="font-mono">{id}</li>)}
                </ul>
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
