import AiSuggestions from '../../AiSuggestions';
import type { Library } from '../library/useLibrary';
import type { Navigation } from '../shell/useNavigation';
import type { Updates } from '../updates/useUpdates';
import type { Appearance } from '../appearance/useAppearance';
import type { ProductEdits } from '../products/useProductEdits';
import type { UiPreferences } from '../preferences/useUiPreferences';
import type { Avatars } from '../avatars/useAvatars';
import type { Filters } from '../filters/useFilters';

/** The AI suggestions view, kept mounted while AI is on so a chat survives switching views. */
export default function AiPane({
  library,
  nav,
  updates,
  appearance,
  edits,
  uiPrefs,
  avatars,
  filters,
}: {
  library: Library;
  nav: Navigation;
  updates: Updates;
  appearance: Appearance;
  edits: ProductEdits;
  uiPrefs: UiPreferences;
  avatars: Avatars;
  filters: Filters;
}) {
  const { aiEnabled, libraryOpening } = library;
  const { view, flow, dialog } = nav;
  const { updateOpen, reportAiChatUpdateBlocked } = updates;
  const { themeDeleteTarget } = appearance;
  const { deleteLocalSelection } = edits;
  const { locale } = uiPrefs;
  const { avatarChoices, avatarLabel } = avatars;
  const { openProduct } = filters;
  return (
    aiEnabled && (
      <div
        style={{
          display: view === 'ai-suggestions' ? 'flex' : 'none',
          flex: 1,
          minHeight: 0,
        }}
      >
        <AiSuggestions
          active={view === 'ai-suggestions'}
          frozen={
            updateOpen ||
            !!themeDeleteTarget ||
            !!flow ||
            !!dialog ||
            !!deleteLocalSelection ||
            libraryOpening
          }
          onUpdateBlockedChange={reportAiChatUpdateBlocked}
          locale={locale}
          avatarOptions={avatarChoices.map((id) => ({ id, label: avatarLabel(id) }))}
          onOpenProduct={openProduct}
        />
      </div>
    )
  );
}
