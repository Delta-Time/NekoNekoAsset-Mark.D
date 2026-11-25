import type { Locale } from '../../model';
import { categoryKeys } from '../../model';
import { t } from '../../i18n';
import type { SearchScope } from './chat-parts';
import type { ChatWorkspace } from './useChatWorkspace';
import type { SearchOptions } from './useSearchOptions';
import type { ChatRequest } from './useChatRequest';

/** Where to search, which stores, and the category and avatar to suggest for. */
export default function ChatOptions({
  workspace,
  search,
  request,
  locale,
  avatarOptions,
}: {
  workspace: ChatWorkspace;
  search: SearchOptions;
  request: ChatRequest;
  locale: Locale;
  avatarOptions: { id: string; label: string }[];
}) {
  const { L, storeLabel } = workspace;
  const {
    scope,
    setScope,
    hasOnline,
    stores,
    setStores,
    category,
    setCategory,
    avatarId,
    setAvatarId,
  } = search;
  const { locked } = request;
  return (
    <div className="ai-chat-options">
      <label>
        {L('scope')}
        <select
          value={scope}
          title={L(scope)}
          disabled={locked}
          onChange={(event) => setScope(event.target.value as SearchScope)}
        >
          {(['library', 'online', 'both'] as const).map((value) => (
            <option value={value} key={value}>
              {L(value)}
            </option>
          ))}
        </select>
      </label>
      {hasOnline && (
        <div className="ai-chat-stores" role="group" aria-label={L('stores')}>
          {(['booth', 'gumroad', 'jinxxy'] as const).map((store) => (
            <label key={store}>
              <input
                type="checkbox"
                checked={stores.includes(store)}
                disabled={locked}
                onChange={(event) =>
                  setStores((previous) =>
                    event.target.checked
                      ? [...previous, store]
                      : previous.filter((value) => value !== store),
                  )
                }
              />
              {storeLabel(store)}
            </label>
          ))}
        </div>
      )}
      <label>
        {L('category')}
        <select
          value={category}
          disabled={locked}
          onChange={(event) => setCategory(event.target.value)}
        >
          <option value="">{L('anyCategory')}</option>
          {categoryKeys.map((value) => (
            <option value={value} key={value}>
              {t(locale, value)}
            </option>
          ))}
        </select>
      </label>
      {avatarOptions.length > 0 && (
        <label>
          {L('avatar')}
          <select
            value={avatarId}
            disabled={locked}
            onChange={(event) => setAvatarId(event.target.value)}
          >
            <option value="">{L('anyAvatar')}</option>
            {avatarOptions.map((avatar) => (
              <option value={avatar.id} key={avatar.id}>
                {avatar.label}
              </option>
            ))}
          </select>
        </label>
      )}
    </div>
  );
}
