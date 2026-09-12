import {StrictMode} from 'react';
import {createRoot} from 'react-dom/client';

// Required by Astryx. Without both imports, components render unstyled.
import '@astryxdesign/core/reset.css';
import '@astryxdesign/core/astryx.css';
import './index.css';

import {App} from './App';

const container = document.getElementById('root');
if (container === null) {
  throw new Error('Missing #root element in index.html');
}

createRoot(container).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
