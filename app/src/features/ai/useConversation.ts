import { useRef, useState } from 'react';
import type { AiChatSummary, AiStoredTurn } from '../../ai-backend';
import type { ChatWorkspace } from './useChatWorkspace';

/** The open chat: its preset, id and turns, whether the sidebar lists history or presets, and
 * the elements that scroll. */
export function useConversation({ workspace }: { workspace: ChatWorkspace }) {
  const { presets, L } = workspace;
  const [presetId, setPresetId] = useState('chat');
  const [historyMode, setHistoryMode] = useState(false);
  const [chatId, setChatId] = useState<string | null>(null);
  const [turns, setTurns] = useState<AiStoredTurn[]>([]);
  const [chatLoading, setChatLoading] = useState(false);
  const navigation = useRef(0);
  const main = useRef<HTMLDivElement>(null);
  const messages = useRef<HTMLDivElement>(null);
  const selectedPreset = presets.find((preset) => preset.id === presetId) ?? {
    title: L('customPreset'),
    description: L('customPresetDescription'),
  };
  const chatTitle = (chat: AiChatSummary) =>
    chat.title === chat.presetId
      ? (presets.find((preset) => preset.id === chat.presetId && preset.builtin)?.title ??
        chat.title)
      : chat.title || L('newChat');
  return {
    presetId,
    setPresetId,
    historyMode,
    setHistoryMode,
    chatId,
    setChatId,
    turns,
    setTurns,
    chatLoading,
    setChatLoading,
    navigation,
    main,
    messages,
    selectedPreset,
    chatTitle,
  };
}
export type Conversation = ReturnType<typeof useConversation>;
