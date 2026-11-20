import React from 'react';
import { createRoot } from 'react-dom/client';
// Stylesheets in cascade order: later files win at equal specificity. Components import their own
// stylesheet too; listing them here keeps the order independent of how modules import each other.
import './inspector.css';
import './editor-styles.css';
import './toggle.css';
import './update.css';
import './settings-reset.css';
import './ai-suggestions.css';
import './licenses.css';
import './features.css';
import './SpendingDialog.css';
import './workflow-panels.css';
import './theme-picker.css';
import './AiSettings.css';
import './local-import.css';
import './UnityWorkflow.css';
import './ActivityHistory.css';
import App from './App';
import './fonts.css';
import './styles.css';
import './library-008.css';
import './calm-studio.css';
createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
