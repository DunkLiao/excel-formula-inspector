import React from 'react';
import ReactDOM from 'react-dom/client';

import App from './App';
import './styles/app.css';

const container = document.getElementById('root');
if (!container) {
  throw new Error('找不到 #root 容器');
}

ReactDOM.createRoot(container).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
