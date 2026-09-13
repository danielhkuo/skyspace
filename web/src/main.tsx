import {LinkProvider} from '@astryxdesign/core/Link';
import {Theme} from '@astryxdesign/core/theme';
import {neutralTheme} from '@astryxdesign/theme-neutral';
import {StrictMode} from 'react';
import {createRoot} from 'react-dom/client';
import {BrowserRouter} from 'react-router';

// Required by Astryx. Without both imports, components render unstyled.
import '@astryxdesign/core/reset.css';
import '@astryxdesign/core/astryx.css';
import './index.css';

import {App} from './App';
import {RouterLink} from './shell/RouterLink';

const container = document.getElementById('root');
if (container === null) {
  throw new Error('Missing #root element in index.html');
}

createRoot(container).render(
  <StrictMode>
    {/* One theme, light only: spec.md says no light/dark switch. */}
    <Theme theme={neutralTheme} mode="light">
      <BrowserRouter>
        <LinkProvider component={RouterLink}>
          <App />
        </LinkProvider>
      </BrowserRouter>
    </Theme>
  </StrictMode>,
);
