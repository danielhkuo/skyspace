import {Navigate, Route, Routes} from 'react-router';

import {CatalogPage} from './catalog/CatalogPage';
import {PlaceholderPage} from './pages/PlaceholderPage';
import {PlanPage} from './plan/PlanPage';
import {Shell} from './shell/Shell';

export function App() {
  return (
    <Routes>
      <Route element={<Shell />}>
        <Route index element={<Navigate to="/catalog" replace />} />
        <Route path="/catalog" element={<CatalogPage />} />
        <Route
          path="/schedule"
          element={<PlaceholderPage title="Schedule" />}
        />
        <Route path="/plan" element={<PlanPage />} />
      </Route>
    </Routes>
  );
}
