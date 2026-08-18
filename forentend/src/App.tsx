/**
 * @license
 * SPDX-License-Identifier: Apache-2.0
 */

import React from 'react';
import { AIQAProvider, useAIQA } from './context/AIQAContext';
import { Header } from './components/layout/Header';
import { Sidebar } from './components/layout/Sidebar';
import { CommandPalette } from './components/layout/CommandPalette';
import { RunTriggerModal } from './components/layout/RunTriggerModal';

// 12 Primary Navigation Pages
import { DashboardPage } from './components/pages/DashboardPage';
import { TestCaseGeneratorPage } from './components/pages/TestCaseGeneratorPage';
import { AIQAOrchestratorPage } from './components/pages/AIQAOrchestratorPage';
import { TestCaseValidatorPage } from './components/pages/TestCaseValidatorPage';
import { TestExecutionPage } from './components/pages/TestExecutionPage';
import { RBACTestingPage } from './components/pages/RBACTestingPage';
import { APITestingPage } from './components/pages/APITestingPage';
import { MobileTestingPage } from './components/pages/MobileTestingPage';
import { CustomTestingPage } from './components/pages/CustomTestingPage';
import { FeatureMapFlowPage } from './components/pages/FeatureMapFlowPage';
import { ReportsPage } from './components/pages/ReportsPage';
import { SettingsPage } from './components/pages/SettingsPage';

const MainContentArea: React.FC = () => {
  const { activePage } = useAIQA();

  const renderActivePage = () => {
    switch (activePage) {
      case 'dashboard':
        return <DashboardPage />;
      case 'generator':
        return <TestCaseGeneratorPage />;
      case 'orchestrator':
        return <AIQAOrchestratorPage />;
      case 'validator':
        return <TestCaseValidatorPage />;
      case 'execution':
        return <TestExecutionPage />;
      case 'rbac':
        return <RBACTestingPage />;
      case 'api-testing':
        return <APITestingPage />;
      case 'mobile-testing':
        return <MobileTestingPage />;
      case 'custom':
        return <CustomTestingPage />;
      case 'feature-map':
        return <FeatureMapFlowPage />;
      case 'reports':
        return <ReportsPage />;
      case 'settings':
        return <SettingsPage />;
      default:
        return <DashboardPage />;
    }
  };

  return (
    <main
      id="main-content-area"
      className="flex-1 overflow-y-auto bg-[#020617] p-3 sm:p-5 lg:p-6 min-h-0 text-slate-100"
    >
      <div className="max-w-7xl mx-auto w-full">
        {renderActivePage()}
      </div>
    </main>
  );
};

export default function App() {
  return (
    <AIQAProvider>
      <div className="flex flex-col h-screen w-screen overflow-hidden bg-[#020617] text-slate-100 font-sans">
        <Header />
        <div className="flex flex-1 overflow-hidden relative">
          <Sidebar />
          <MainContentArea />
        </div>
        <CommandPalette />
        <RunTriggerModal />
      </div>
    </AIQAProvider>
  );
}
