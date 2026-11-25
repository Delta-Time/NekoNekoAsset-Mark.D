import { useEffect } from 'react';
import { errorCode } from '../../backend';
import { deleteAiChat, getAiChat, renameAiChat } from '../../ai-backend';
import type { AiChatSummary } from '../../ai-backend';
import type { Conversation } from './useConversation';
import type { ChatRequest } from './useChatRequest';
import type { HistoryEdits } from './useHistoryEdits';
import type { ChatWorkspace } from './useChatWorkspace';

/** Starting a chat, opening, renaming and deleting chats, and keeping the conversation scrolled
 * to its end. */
export function useChatActions({
  conversation,
  request,
  history,
  workspace,
}: {
  conversation: Conversation;
  request: ChatRequest;
  history: HistoryEdits;
  workspace: ChatWorkspace;
}) {
  const {
    messages,
    turns,
    chatLoading,
    main,
    presetId,
    navigation,
    setPresetId,
    setChatId,
    setTurns,
    setHistoryMode,
    setChatLoading,
    chatId,
  } = conversation;
  const { busy, locked } = request;
  const {
    resetConfirmation,
    renameDraft,
    historyBusy,
    setHistoryBusy,
    setRenaming,
    deleting,
    setDeleting,
    deleteTimer,
  } = history;
  const { presets, setError, setStatus, mounted, updateSummary, setChats } = workspace;
  useEffect(() => {
    const chatMessages = messages.current;
    chatMessages?.scrollTo({ top: turns.length || busy ? chatMessages.scrollHeight : 0 });
    if (
      !turns.length &&
      !busy &&
      !chatLoading &&
      window.matchMedia('(max-height:350px)').matches &&
      main.current &&
      chatMessages
    )
      main.current.scrollTo({ top: chatMessages.offsetTop - main.current.offsetTop });
  }, [turns, busy, chatLoading]);
  function newChat(nextPreset = presetId) {
    if (locked) return;
    main.current?.scrollTo({ top: 0 });
    navigation.current++;
    resetConfirmation();
    setPresetId(presets.some((preset) => preset.id === nextPreset) ? nextPreset : 'chat');
    setChatId(null);
    setTurns([]);
    setError('');
    setStatus('');
    setHistoryMode(true);
  }
  async function openChat(id: string) {
    if (locked) return;
    main.current?.scrollTo({ top: 0 });
    resetConfirmation();
    setChatLoading(true);
    setError('');
    setStatus('');
    const epoch = ++navigation.current;
    try {
      const value = await getAiChat(id);
      if (mounted.current && epoch === navigation.current) {
        setChatId(value.chat.id);
        setPresetId(value.chat.presetId);
        setTurns(value.messages);
        setHistoryMode(true);
      }
    } catch (value) {
      if (mounted.current && epoch === navigation.current) setError(errorCode(value));
    } finally {
      if (mounted.current && epoch === navigation.current) setChatLoading(false);
    }
  }
  async function renameChat(chat: AiChatSummary) {
    const title = renameDraft.trim();
    if (historyBusy || !title) return;
    setHistoryBusy(true);
    setError('');
    try {
      const value = await renameAiChat(chat.id, title);
      if (mounted.current) {
        updateSummary(value);
        setRenaming(null);
      }
    } catch (value) {
      if (mounted.current) setError(errorCode(value));
    } finally {
      if (mounted.current) setHistoryBusy(false);
    }
  }
  async function deleteChat(chat: AiChatSummary) {
    if (locked) return;
    if (deleting?.id !== chat.id) {
      resetConfirmation();
      setDeleting({ id: chat.id, ready: false });
      deleteTimer.current = setTimeout(() => {
        if (mounted.current)
          setDeleting((value) => (value?.id === chat.id ? { id: chat.id, ready: true } : value));
        deleteTimer.current = null;
      }, 600);
      return;
    }
    if (!deleting.ready) return;
    setHistoryBusy(true);
    setError('');
    try {
      await deleteAiChat(chat.id);
      if (mounted.current) {
        setChats((previous) => previous.filter((value) => value.id !== chat.id));
        if (chatId === chat.id) {
          setChatId(null);
          setTurns([]);
        }
        resetConfirmation();
      }
    } catch (value) {
      if (mounted.current) setError(errorCode(value));
    } finally {
      if (mounted.current) setHistoryBusy(false);
    }
  }
  return { newChat, openChat, renameChat, deleteChat };
}
export type ChatActions = ReturnType<typeof useChatActions>;
