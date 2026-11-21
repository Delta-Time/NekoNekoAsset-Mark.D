import type { Locale } from '../../model';
import { errorCode } from '../../backend';
import { cancelAiSuggestion, createAiChat, getAiChat, sendAiSuggestion } from '../../ai-backend';
import type { AiStoredTurn } from '../../ai-backend';
import type { UnconfirmedTurn } from './chat-parts';
import type { ChatRequest } from './useChatRequest';
import type { ChatWorkspace } from './useChatWorkspace';
import type { HistoryEdits } from './useHistoryEdits';
import type { Conversation } from './useConversation';
import type { Composer } from './useComposer';
import type { SearchOptions } from './useSearchOptions';

/** Sending a message (creating the chat first when needed), cancelling it, and settling a message
 * whose delivery is unknown: restore it, check history or discard it. */
export function useSendMessage({
  request,
  workspace,
  history,
  conversation,
  composer,
  search,
  locale,
  frozen,
  onUpdateBlockedChange,
}: {
  request: ChatRequest;
  workspace: ChatWorkspace;
  history: HistoryEdits;
  conversation: Conversation;
  composer: Composer;
  search: SearchOptions;
  locale: Locale;
  frozen: boolean;
  onUpdateBlockedChange: ((blocked: boolean) => void) | undefined;
}) {
  const {
    currentRequest,
    cancelRequested,
    requestSubmitted,
    canSend,
    setBusy,
    setUnconfirmed,
    unconfirmed,
    busy,
  } = request;
  const { setStatus, mounted, setError, updateSummary, entryId } = workspace;
  const { resetConfirmation } = history;
  const {
    setHistoryMode,
    chatId,
    presetId,
    setChatId,
    turns,
    setTurns,
    chatLoading,
    setChatLoading,
  } = conversation;
  const { prompt, images, setPrompt, setImages, setImageTrayOpen, imageInput } = composer;
  const { scope, hasOnline, stores, category, avatarId } = search;
  async function cancel() {
    const id = currentRequest.current;
    if (!id) return;
    cancelRequested.current = true;
    setStatus('ai_chat_cancelled');
    if (!requestSubmitted.current) return;
    try {
      await cancelAiSuggestion(id);
    } catch (value) {
      if (mounted.current) setError(errorCode(value));
    }
  }
  async function send() {
    if (!canSend || currentRequest.current) return;
    const requestId = crypto.randomUUID();
    currentRequest.current = requestId;
    cancelRequested.current = false;
    requestSubmitted.current = false;
    onUpdateBlockedChange?.(true);
    setBusy(true);
    setError('');
    setStatus('');
    resetConfirmation();
    setHistoryMode(true);
    const content = prompt.trim();
    const attached = [...images];
    const sentAt = new Date().toISOString();
    let sessionId = chatId;
    let draft: UnconfirmedTurn | null = null;
    try {
      if (!sessionId) {
        const detail = await createAiChat(presetId);
        sessionId = detail.chat.id;
        if (!mounted.current || currentRequest.current !== requestId) return;
        setChatId(sessionId);
        updateSummary(detail.chat);
      }
      if (cancelRequested.current) return;
      draft = {
        chatId: sessionId,
        requestId,
        content,
        images: attached,
        priorIds: turns.map((turn) => turn.id),
      };
      setUnconfirmed(draft);
      setTurns((previous) => [
        ...previous,
        {
          id: requestId,
          role: 'user',
          content,
          images: attached,
          createdAt: sentAt,
          status: 'pending',
        },
      ]);
      setPrompt('');
      setImages([]);
      setImageTrayOpen(false);
      if (imageInput.current) imageInput.current.value = '';
      requestSubmitted.current = true;
      await sendAiSuggestion({
        requestId,
        chatId: sessionId,
        entryId,
        appLanguage: locale,
        prompt: content,
        preset: 'chat',
        scope,
        stores: hasOnline ? [...stores] : [],
        ...(category ? { category } : {}),
        ...(avatarId ? { avatarId } : {}),
        images: attached,
      });
    } catch (value) {
      if (mounted.current && currentRequest.current === requestId) setError(errorCode(value));
    } finally {
      if (mounted.current && currentRequest.current === requestId) {
        if (sessionId) {
          try {
            const detail = await getAiChat(sessionId);
            if (mounted.current && currentRequest.current === requestId) {
              setTurns(detail.messages);
              updateSummary(detail.chat);
              if (draft) resolveDraft(draft, detail.messages);
            }
          } catch (value) {
            if (mounted.current) {
              setError(errorCode(value));
              setTurns((previous) =>
                previous.map((turn) =>
                  turn.id === requestId ? { ...turn, status: 'failed' } : turn,
                ),
              );
            }
          }
        }
        currentRequest.current = null;
        requestSubmitted.current = false;
        setBusy(false);
      }
    }
  }
  function resolveDraft(draft: UnconfirmedTurn, durable: AiStoredTurn[]) {
    const recorded = durable.some(
      (turn) =>
        turn.role === 'user' &&
        !draft.priorIds.includes(turn.id) &&
        turn.content === draft.content &&
        turn.images.length === draft.images.length,
    );
    if (!recorded) {
      setPrompt(draft.content);
      setImages(draft.images);
    }
    setUnconfirmed(null);
  }
  async function checkUnconfirmed() {
    const draft = unconfirmed;
    if (!draft || busy || chatLoading || frozen) return;
    setChatLoading(true);
    setError('');
    try {
      const detail = await getAiChat(draft.chatId);
      if (mounted.current) {
        setTurns(detail.messages);
        updateSummary(detail.chat);
        resolveDraft(draft, detail.messages);
      }
    } catch (value) {
      if (mounted.current) setError(errorCode(value));
    } finally {
      if (mounted.current) setChatLoading(false);
    }
  }
  function discardUnconfirmed() {
    if (!unconfirmed || busy || chatLoading || frozen) return;
    setTurns((previous) => previous.filter((turn) => turn.id !== unconfirmed.requestId));
    setUnconfirmed(null);
    setError('');
  }
  return { cancel, send, checkUnconfirmed, discardUnconfirmed };
}
export type SendMessage = ReturnType<typeof useSendMessage>;
