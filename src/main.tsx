import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './App';
import { applyInitialTheme } from './hooks/useDocumentTheme';
import './styles/index.css';

applyInitialTheme();

if (import.meta.env.PROD) {
  // A native utility has no browser context menu (Back, Refresh, Print …). Text fields
  // and selectable text (paths, executable names) keep it for cut, copy and paste.
  document.addEventListener('contextmenu', (event) => {
    const target = event.target;
    const textual =
      target instanceof HTMLElement &&
      (target.isContentEditable || target.closest('input, textarea, code, .selectable') !== null);
    if (!textual) event.preventDefault();
  });
}

const container = document.getElementById('root');
if (!container) throw new Error('index.html is missing the #root element.');

createRoot(container).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
