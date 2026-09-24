import React, { useState, useEffect, useCallback } from 'react';
import { api, Config, formatBytes } from '../lib/api';
import { useAppContext } from '../context/AppContext';

export default function SettingsPage() {
  const { theme, setTheme } = useAppContext();
  const [config, setConfig] = useState<Config | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api.getSettings().then(c => {
      setConfig(c);
      setLoading(false);
    }).catch(e => {
      setError(String(e));
      setLoading(false);
    });
  }, []);

  const handleSave = useCallback(async () => {
    if (!config) return;
    setSaving(true);
    setError(null);
    try {
      await api.updateSettings(config);
      setSaved(true);
      setTimeout(() => setSaved(false), 3000);
    } catch (e: unknown) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }, [config]);

  if (loading) return <div className="page-body"><div className="spinner spinner-lg" style={{ margin: '100px auto' }} /></div>;

  return (
    <>
      <header className="page-header">
        <div>
          <h1 className="page-title">Settings</h1>
          <div className="page-subtitle">Configure ShrinkOS behavior</div>
        </div>
        <button
          id="btn-save-settings"
          className="btn btn-primary"
          onClick={handleSave}
          disabled={saving || !config}
        >
          {saving ? <><span className="spinner" style={{ width: 14, height: 14 }} /> Saving…</> : '✓ Save Changes'}
        </button>
      </header>

      <div className="page-body">
        {error && <div className="alert alert-error mb-6"><span>⚠</span><div>{error}</div></div>}
        {saved && <div className="alert alert-success mb-6"><span>✓</span><div>Settings saved.</div></div>}

        {config && (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-5)', maxWidth: 600 }}>

            {/* Appearance */}
            <SettingSection title="Appearance">
              <SettingRow
                label="Theme"
                description="Choose between dark and light interface"
              >
                <div style={{ display: 'flex', gap: 'var(--space-2)' }}>
                  <button
                    className={`btn btn-sm ${theme === 'dark' ? 'btn-primary' : 'btn-secondary'}`}
                    onClick={() => setTheme('dark')}
                  >
                    ◐ Dark
                  </button>
                  <button
                    className={`btn btn-sm ${theme === 'light' ? 'btn-primary' : 'btn-secondary'}`}
                    onClick={() => setTheme('light')}
                  >
                    ○ Light
                  </button>
                </div>
              </SettingRow>
            </SettingSection>

            {/* Compression */}
            <SettingSection title="Compression">
              <SettingRow
                label="Compression Level"
                description={`Zstd level ${config.compression_level} · Higher = better ratio but slower compression`}
              >
                <div style={{ display: 'flex', alignItems: 'center', gap: 'var(--space-3)' }}>
                  <input
                    id="input-compression-level"
                    type="range"
                    min={1}
                    max={22}
                    value={config.compression_level}
                    onChange={e => setConfig({ ...config, compression_level: Number(e.target.value) })}
                    style={{ width: 140 }}
                  />
                  <span className="font-mono text-sm" style={{ minWidth: 20, textAlign: 'right' }}>
                    {config.compression_level}
                  </span>
                </div>
              </SettingRow>
              <SettingRow
                label="Parallel Tasks"
                description="Number of files compressed simultaneously"
              >
                <div style={{ display: 'flex', alignItems: 'center', gap: 'var(--space-3)' }}>
                  <input
                    id="input-parallel-tasks"
                    type="range"
                    min={1}
                    max={8}
                    value={config.parallel_tasks}
                    onChange={e => setConfig({ ...config, parallel_tasks: Number(e.target.value) })}
                    style={{ width: 100 }}
                  />
                  <span className="font-mono text-sm">{config.parallel_tasks}</span>
                </div>
              </SettingRow>
            </SettingSection>

            {/* Cache */}
            <SettingSection title="Cache">
              <SettingRow
                label="Maximum Cache Size"
                description={`Currently: ${formatBytes(config.max_cache_bytes)}`}
              >
                <select
                  id="select-cache-size"
                  className="btn btn-secondary btn-sm"
                  value={config.max_cache_bytes}
                  onChange={e => setConfig({ ...config, max_cache_bytes: Number(e.target.value) })}
                  style={{ cursor: 'pointer' }}
                >
                  <option value={512 * 1024 * 1024}>512 MB</option>
                  <option value={1 * 1024 * 1024 * 1024}>1 GB</option>
                  <option value={2 * 1024 * 1024 * 1024}>2 GB</option>
                  <option value={4 * 1024 * 1024 * 1024}>4 GB</option>
                  <option value={8 * 1024 * 1024 * 1024}>8 GB</option>
                </select>
              </SettingRow>
            </SettingSection>

            {/* Automation */}
            <SettingSection title="Automation">
              <SettingRow
                label="Auto-detect new applications"
                description="Monitor standard app directories for newly installed software"
              >
                <Toggle
                  id="toggle-auto-detect"
                  value={config.auto_detect}
                  onChange={v => setConfig({ ...config, auto_detect: v })}
                />
              </SettingRow>
              <SettingRow
                label="Show notifications"
                description="Notify when new apps are detected or optimization completes"
              >
                <Toggle
                  id="toggle-notifications"
                  value={config.show_notifications}
                  onChange={v => setConfig({ ...config, show_notifications: v })}
                />
              </SettingRow>
            </SettingSection>

          </div>
        )}
      </div>
    </>
  );
}

// ── Utility components ─────────────────────────────────────────────────────

function SettingSection({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="card">
      <div className="card-title">{title}</div>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-5)' }}>
        {children}
      </div>
    </div>
  );
}

function SettingRow({ label, description, children }: {
  label: string;
  description?: string;
  children: React.ReactNode;
}) {
  return (
    <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', gap: 'var(--space-4)' }}>
      <div>
        <div className="text-sm font-medium">{label}</div>
        {description && <div className="text-xs text-muted" style={{ marginTop: 2 }}>{description}</div>}
      </div>
      {children}
    </div>
  );
}

function Toggle({ id, value, onChange }: { id: string; value: boolean; onChange: (v: boolean) => void }) {
  return (
    <button
      id={id}
      onClick={() => onChange(!value)}
      style={{
        width: 44,
        height: 24,
        borderRadius: 12,
        border: 'none',
        cursor: 'pointer',
        background: value ? 'var(--color-accent-500)' : 'var(--color-bg-overlay)',
        position: 'relative',
        transition: 'background 200ms',
        flexShrink: 0,
      }}
      role="switch"
      aria-checked={value}
    >
      <div style={{
        position: 'absolute',
        top: 2,
        left: value ? 22 : 2,
        width: 20,
        height: 20,
        borderRadius: '50%',
        background: 'white',
        transition: 'left 200ms',
        boxShadow: '0 1px 3px rgba(0,0,0,0.3)',
      }} />
    </button>
  );
}
