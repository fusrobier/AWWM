import { useEffect, useMemo, useRef, useState } from 'react';
import { Sidebar } from './components/Sidebar';
import { ToastProvider } from './components/Toasts';
import { initialPage, PAGE_TITLES, pageFromHash } from './lib/navigation';
import type { Page } from './lib/types';
import { AboutPage } from './pages/AboutPage';
import { DashboardPage } from './pages/DashboardPage';
import { GeneralPage } from './pages/GeneralPage';
import { MousePage } from './pages/MousePage';
import { RulesPage } from './pages/RulesPage';
import { AppStateProvider } from './state/AppStateProvider';
import { NavigationContext, useAppState, useNavigation, type Navigation } from './state/appStateContext';

export function App() {
  // The backend opens the window at index.html#<page>; an open window gets
  // awwm://navigate events instead (handled by AppStateProvider).
  const [page, setPage] = useState<Page>(() => initialPage(window.location.hash));
  const navigation = useMemo<Navigation>(() => ({ page, navigate: setPage }), [page]);

  useEffect(() => {
    const onHashChange = (): void => {
      const requested = pageFromHash(window.location.hash);
      if (requested) setPage(requested);
    };
    window.addEventListener('hashchange', onHashChange);
    return () => window.removeEventListener('hashchange', onHashChange);
  }, []);

  return (
    <ToastProvider>
      <AppStateProvider onNavigate={setPage}>
        <NavigationContext.Provider value={navigation}>
          <Shell />
        </NavigationContext.Provider>
      </AppStateProvider>
    </ToastProvider>
  );
}

function Shell() {
  const { page, navigate } = useNavigation();
  const { snapshot, dismissedWarning } = useAppState();
  const mainRef = useRef<HTMLElement>(null);

  useEffect(() => {
    mainRef.current?.scrollTo({ top: 0 });
    document.title = `${PAGE_TITLES[page]} · AWWM`;
  }, [page]);

  const attention = useMemo(() => {
    const pages = new Set<Page>();
    if (snapshot.settingsWarning !== null && snapshot.settingsWarning !== dismissedWarning) pages.add('general');
    return pages;
  }, [snapshot.settingsWarning, dismissedWarning]);

  return (
    <div className="app-shell">
      <Sidebar
        page={page}
        onNavigate={navigate}
        managementEnabled={snapshot.managementEnabled}
        attention={attention}
      />
      <main ref={mainRef} id="main-content" className="app-main" tabIndex={-1} aria-labelledby="page-title">
        <div className="page" key={page}>
          <PageView page={page} />
        </div>
      </main>
    </div>
  );
}

function PageView({ page }: { page: Page }) {
  switch (page) {
    case 'dashboard':
      return <DashboardPage />;
    case 'rules':
      return <RulesPage />;
    case 'mouse':
      return <MousePage />;
    case 'general':
      return <GeneralPage />;
    case 'about':
      return <AboutPage />;
  }
}
