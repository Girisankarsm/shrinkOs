import React, { useState } from 'react';
import './index.css';
import { AppProvider } from './context/AppContext';
import Sidebar from './components/Sidebar';
import Dashboard from './pages/Dashboard';
import ApplicationsPage from './pages/Applications';
import OptimizePage from './pages/Optimize';
import SettingsPage from './pages/Settings';

type Page = 'dashboard' | 'applications' | 'optimize' | 'restore' | 'settings';

export default function App() {
  const [currentPage, setCurrentPage] = useState<Page>('dashboard');

  return (
    <AppProvider>
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
