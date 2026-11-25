import type { Navigation } from '../shell/useNavigation';
import type { Library } from '../library/useLibrary';

/** Opening Settings in the details pane, and closing it back to the view, tab and pane it
 * replaced. */
export function useSettingsPane({ nav, library }: { nav: Navigation; library: Library }) {
  const {
    view,
    settingsReturn,
    inspector,
    tab,
    setSettingsRoute,
    setView,
    setInspector,
    setTab,
    settingsRoute,
  } = nav;
  const { aiEnabled } = library;
  const openSettings = (route: string) => {
    if (view !== 'settings')
      settingsReturn.current = {
        inspector,
        tab,
        view: view === 'activity' || view === 'ai-suggestions' ? view : 'library',
      };
    setSettingsRoute(route === 'ai-settings' && !aiEnabled ? 'features' : route);
    setView('settings');
    setInspector(true);
  };
  const toggleSettings = () => {
    if (view === 'settings') {
      setView(
        settingsReturn.current.view === 'ai-suggestions' && !aiEnabled
          ? 'library'
          : settingsReturn.current.view,
      );
      setTab(settingsReturn.current.tab);
      setInspector(settingsReturn.current.inspector);
    } else openSettings(settingsRoute);
  };
  return { openSettings, toggleSettings };
}
export type SettingsPane = ReturnType<typeof useSettingsPane>;
