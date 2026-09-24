import React, { useEffect, useState } from 'react';
import './index.css';
import { AppProvider } from './context/AppContext';
import Sidebar from './components/Sidebar';
import Dashboard from './pages/Dashboard';
import ApplicationsPage from './pages/Applications';
import OptimizePage from './pages/Optimize';
import SettingsPage from './pages/Settings';
import { api } from './lib/api';
import { useAppContext } from './context/AppContext';

type Page = 'dashboard' | 'applications' | 'optimize' | 'restore' | 'settings';

export default function App() {
  const [currentPage, setCurrentPage] = useState<Page>('dashboard');

  return (
    <AppProvider>
      <PermissionPrompt />
      <div className="app-layout">
        <Sidebar currentPage={currentPage} onNavigate={setCurrentPage} />
        <main className="main-content">
          {currentPage === 'dashboard'    && <Dashboard />}
          {currentPage === 'applications' && <ApplicationsPage />}
          {currentPage === 'optimize'     && <ApplicationsPage />}
          {currentPage === 'restore'      && <OptimizePage />}
          {currentPage === 'settings'     && <SettingsPage />}
        </main>
      </div>
    </AppProvider>
  );
}

function PermissionPrompt() {
  const { permissionStatus } = useAppContext();

  useEffect(() => {
    if (!permissionStatus?.requires_approval || localStorage.getItem('shrinkos-permission-prompted') === '1') {
      return;
    }

    localStorage.setItem('shrinkos-permission-prompted', '1');
    if (window.confirm(`${permissionStatus.message}\n\nOpen System Settings now?`)) {
      void api.openPermissionSettings();
    }
  }, [permissionStatus]);

  return null;
}
