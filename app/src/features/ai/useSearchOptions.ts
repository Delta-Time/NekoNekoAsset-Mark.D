import { useState } from 'react';
import type { SearchScope, Store } from './chat-parts';

/** Where the AI looks: the library, stores or both, which stores, and a category and avatar. */
export function useSearchOptions({ initialAvatarId }: { initialAvatarId: string }) {
  const [scope, setScope] = useState<SearchScope>('library');
  const [stores, setStores] = useState<Store[]>(['booth', 'gumroad', 'jinxxy']);
  const [category, setCategory] = useState('');
  const [avatarId, setAvatarId] = useState(initialAvatarId);
  const hasOnline = scope !== 'library';
  return {
    scope,
    setScope,
    stores,
    setStores,
    category,
    setCategory,
    avatarId,
    setAvatarId,
    hasOnline,
  };
}
export type SearchOptions = ReturnType<typeof useSearchOptions>;
