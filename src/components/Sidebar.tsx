import React from 'react';
import { formatBytes } from '../lib/api';
import { useAppContext } from '../context/AppContext';

type Page = 'dashboard' | 'applications' | 'optimize' | 'settings';

interface SidebarProps {
  currentPage: Page;
  onNavigate: (page: Page) => void;
}

const NAV_ITEMS: { id: Page; label: string; icon: string }[] = [
  { id: 'dashboard',    label: 'Dashboard',    icon: '⊞' },
  { id: 'applications', label: 'Applications', icon: '◫' },
  { id: 'optimize',     label: 'Optimize',     icon: '◈' },
  { id: 'settings',     label: 'Settings',     icon: '◉' },
];

export default function Sidebar({ currentPage, onNavigate }: SidebarProps) {
  const { systemInfo, vaultStats, diskStats } = useAppContext();

  const savedBytes = vaultStats?.totals.total_saved_bytes ?? 0;
  const appCount = vaultStats?.totals.managed_app_count ?? 0;

  return (
    <aside className="sidebar">
      {/* Logo */}
      <div className="sidebar-logo">
        <div className="sidebar-logo-icon">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="white" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
            <path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"/>
            <polyline points="3.27 6.96 12 12.01 20.73 6.96"/>
            <line x1="12" y1="22.08" x2="12" y2="12"/>
          </svg>
        </div>
        <div>
          <div className="sidebar-logo-text">ShrinkOS</div>
          <div className="sidebar-logo-version">
            v{systemInfo?.app_version ?? '0.1.0'} M1
          </div>
        </div>
      </div>

      {/* Navigation */}
      <nav className="sidebar-nav">
        <span className="nav-section-label">Navigation</span>
        {NAV_ITEMS.map(item => (
          <button
            key={item.id}
            id={`nav-${item.id}`}
            className={`nav-item ${currentPage === item.id ? 'active' : ''}`}
            onClick={() => onNavigate(item.id)}
          >
            <span className="nav-item-icon" style={{ fontSize: '16px', lineHeight: 1 }}>
              {item.icon}
            </span>
            {item.label}
          </button>
        ))}
      </nav>

      {/* Footer Stats */}
      <div className="sidebar-footer">
        {appCount > 0 && (
          <div style={{
            background: 'var(--color-bg-elevated)',
            border: '1px solid var(--color-border)',
            borderRadius: 'var(--radius-md)',
            padding: 'var(--space-4)',
            marginBottom: 'var(--space-4)',
          }}>
            <div style={{ fontSize: 'var(--text-xs)', color: 'var(--color-text-muted)', marginBottom: 'var(--space-1)' }}>
              Space Saved
            </div>
            <div style={{ fontSize: 'var(--text-lg)', fontWeight: 700, color: 'var(--color-success)' }}>
              {formatBytes(savedBytes)}
            </div>
            <div style={{ fontSize: 'var(--text-xs)', color: 'var(--color-text-muted)', marginTop: '2px' }}>
              {appCount} app{appCount !== 1 ? 's' : ''} managed
            </div>
          </div>
        )}

        <div style={{ fontSize: 'var(--text-xs)', color: 'var(--color-text-muted)' }}>
          {systemInfo?.os_version}
          {systemInfo?.filesystem && (
            <span style={{ marginLeft: 4, color: 'var(--color-text-muted)' }}>
              · {systemInfo.filesystem}
            </span>
          )}
        </div>
      </div>
    </aside>
  );
}
