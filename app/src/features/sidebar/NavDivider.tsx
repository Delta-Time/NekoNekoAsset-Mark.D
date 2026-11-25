import type { Layout } from '../shell/useLayout';
import type { UiPreferences } from '../preferences/useUiPreferences';

/** The divider that resizes the sidebar, by pointer or arrow keys. */
export default function NavDivider({
  layout,
  uiPrefs,
  q,
}: {
  layout: Layout;
  uiPrefs: UiPreferences;
  q: (key: string) => string;
}) {
  const { resize } = layout;
  const { setNavWidth } = uiPrefs;
  return (
    <div
      role="separator"
      aria-label={q('resizeNavigation')}
      aria-orientation="vertical"
      className="drag-divider left-divider"
      tabIndex={0}
      onPointerDown={(e) => resize('left', e)}
      onKeyDown={(e) => {
        if (['ArrowLeft', 'ArrowRight'].includes(e.key)) {
          e.preventDefault();
          setNavWidth((value) =>
            Math.max(132, Math.min(300, value + (e.key === 'ArrowRight' ? 16 : -16))),
          );
        }
      }}
    />
  );
}
