import { useEffect, useRef, useState } from 'react';
import { cancelAiSuggestion } from '../../ai-backend';
import type { UnconfirmedTurn } from './chat-parts';
import type { ChatWorkspace } from './useChatWorkspace';
import type { Conversation } from './useConversation';
import type { HistoryEdits } from './useHistoryEdits';
import type { Composer } from './useComposer';
import type { SearchOptions } from './useSearchOptions';

/** The reply in flight and a sent message not yet confirmed, what they lock, and whether Send is
 * possible. Leaving the view cancels the request. */
export function useChatRequest({
  workspace,
  conversation,
  history,
  composer,
  search,
  active,
  frozen,
  onUpdateBlockedChange,
}: {
  workspace: ChatWorkspace;
  conversation: Conversation;
  history: HistoryEdits;
  composer: Composer;
  search: SearchOptions;
  active: boolean;
  frozen: boolean;
  onUpdateBlockedChange: ((blocked: boolean) => void) | undefined;
}) {
  const { loading, provider, keyMissing, vision, mounted } = workspace;
  const { chatLoading, navigation } = conversation;
  const { historyBusy, deleteTimer } = history;
  const { imageBusy, promptTooLong, images, prompt, conversion } = composer;
  const { hasOnline, stores } = search;
  const [busy, setBusy] = useState(false);
  const [unconfirmed, setUnconfirmed] = useState<UnconfirmedTurn | null>(null);
  // Only a reply in flight holds up an App update; unsent drafts never do.
  useEffect(() => {
    onUpdateBlockedChange?.(busy);
  }, [busy, onUpdateBlockedChange]);
  useEffect(
    () => () => {
      onUpdateBlockedChange?.(false);
    },
    [onUpdateBlockedChange],
  );
  const currentRequest = useRef<string | null>(null);
  const cancelRequested = useRef(false);
  const requestSubmitted = useRef(false);
  const locked = loading || busy || chatLoading || historyBusy || !!unconfirmed;
  const canSend =
    active &&
    !frozen &&
    !loading &&
    !!provider &&
    !locked &&
    !imageBusy &&
    !keyMissing &&
    !promptTooLong &&
    (!images.length || vision) &&
    (!!prompt.trim() || images.length > 0) &&
    (!hasOnline || stores.length > 0);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      conversion.current++;
      navigation.current++;
      if (deleteTimer.current) clearTimeout(deleteTimer.current);
      const id = currentRequest.current;
      currentRequest.current = null;
      if (id) void cancelAiSuggestion(id).catch(() => {});
    };
  }, []);
  return {
    busy,
    setBusy,
    unconfirmed,
    setUnconfirmed,
    currentRequest,
    cancelRequested,
    requestSubmitted,
    locked,
    canSend,
  };
}
export type ChatRequest = ReturnType<typeof useChatRequest>;
