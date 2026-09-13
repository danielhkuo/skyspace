import {Navigate, Route, Routes} from 'react-router';

import {CatalogPage} from './catalog/CatalogPage';
import {AccountPage} from './pages/AccountPage';
import {OnboardingPage} from './pages/OnboardingPage';
import {SignInPage} from './pages/SignInPage';
import {PlanPage} from './plan/PlanPage';
import {SchedulePage} from './schedule/SchedulePage';
import {Shell} from './shell/Shell';

export function App() {
  return (
    <Routes>
      <Route element={<Shell />}>
        <Route index element={<Navigate to="/catalog" replace />} />
        <Route path="/catalog" element={<CatalogPage />} />
        <Route path="/schedule" element={<SchedulePage />} />
        <Route path="/plan" element={<PlanPage />} />
        <Route path="/plan/new" element={<OnboardingPage />} />
        <Route path="/sign-in" element={<SignInPage />} />
        <Route path="/account" element={<AccountPage />} />
      </Route>
    </Routes>
  );
}
