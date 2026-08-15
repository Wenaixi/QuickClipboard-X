import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import '../../shared/i18n.js';
import App from './App.jsx';
import './screenshot.css';

createRoot(document.getElementById('root')).render(
  <StrictMode>
    <App />
  </StrictMode>
);
