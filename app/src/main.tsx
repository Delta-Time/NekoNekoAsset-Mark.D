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
import './ai-chat.css';
import './ai-suggestions-responsive.css';
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
import './styles/base.css';
import './styles/tokens.css';
import './styles/controls.css';
import './styles/motion.css';
import './styles/topbar.css';
import './styles/sidebar.css';
import './styles/toolbar.css';
import './styles/results.css';
import './styles/cards.css';
import './styles/list.css';
import './styles/footer.css';
import './styles/menus.css';
import './styles/inspector.css';
import './styles/asset-inspector.css';
import './styles/inspector-files.css';
import './styles/settings.css';
import './styles/workflows.css';
import './styles/dialogs.css';
import './styles/activity.css';
import './styles/ai.css';
import './styles/statusbar.css';
createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
