import { useEffect, useRef, useState } from 'react';
import type { Locale } from '../../model';
import { errorCode, listAiEntries, listAiWorkspace, openExternal } from '../../backend';
import type { AiChatSummary, AiCustomPreset, AiEntry, AiPersonalization } from '../../backend';
import { getBuiltinAiPresets } from '../../ai-presets';
import { aiSuggestionsText } from '../../ai-suggestions-i18n';
import { sortChats } from './chat-parts';

/** What the AI view loads while it is shown: chats, custom presets, personalization and API
 * entries; the chosen API and what it supports; the view's error and status line; labels. */
export function useChatWorkspace({ locale, active }: { locale: Locale; active: boolean }) {
  const L = (key: string) => aiSuggestionsText(locale, key);
  const [chats, setChats] = useState<AiChatSummary[]>([]);
  const [customPresets, setCustomPresets] = useState<AiCustomPreset[]>([]);
  const [personalization, setPersonalization] = useState<AiPersonalization>({
    avatarDataUrl: null,
    userName: '',
    replyLanguage: 'app',
    personality: 'basic',
  });
  const [editor, setEditor] = useState<{ preset: AiCustomPreset | null } | null>(null);
  const [entries, setEntries] = useState<AiEntry[]>([]);
  const [entryId, setEntryId] = useState('');
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');
  const [status, setStatus] = useState('');
  const mounted = useRef(true);
  const presets = [
    ...getBuiltinAiPresets(locale),
    ...customPresets.map((preset) => ({ ...preset, builtin: false })),
  ];
  const provider = entries.find((entry) => entry.id === entryId);
  const vision = provider?.visionSupported === true;
  const keyMissing = !!provider && provider.keyRequired && !provider.keyPresent;
  useEffect(() => {
    if (!active) return;
    let current = true;
    setLoading(true);
    void Promise.all([listAiWorkspace(), listAiEntries()])
      .then(([workspace, providers]) => {
        if (!current) return;
        setChats(sortChats(workspace.chats));
        setCustomPresets(workspace.presets);
        setPersonalization(workspace.personalization);
        setEntries(providers.entries);
        setEntryId((previous) =>
          providers.entries.some((entry) => entry.id === previous)
            ? previous
            : (providers.activeEntryId ?? providers.entries[0]?.id ?? ''),
        );
      })
      .catch((value) => {
        if (current) setError(errorCode(value));
      })
      .finally(() => {
        if (current) setLoading(false);
      });
    return () => {
      current = false;
    };
  }, [active]);
  function updateSummary(chat: AiChatSummary) {
    setChats((previous) => sortChats([...previous.filter((value) => value.id !== chat.id), chat]));
  }
  async function openPage(url: string) {
    try {
      await openExternal(url);
    } catch (value) {
      if (mounted.current) setError(errorCode(value));
    }
  }
  const storeLabel = (store: string) =>
    store === 'booth'
      ? 'BOOTH'
      : store === 'gumroad'
        ? 'Gumroad'
        : store === 'jinxxy'
          ? 'Jinxxy'
          : store === 'local'
            ? L('library')
            : store;
  const apiLabel = (entry: AiEntry) => {
    const label = `${entry.provider} · ${entry.model}`;
    if (
      entries.filter((value) => value.provider === entry.provider && value.model === entry.model)
        .length < 2
    )
      return label;
    let host = '';
    try {
      host = new URL(entry.endpoint).host;
    } catch {}
    return `${label} · ${host || entry.id.slice(0, 6)}${entries.filter((value) => value.provider === entry.provider && value.model === entry.model && value.endpoint === entry.endpoint).length > 1 ? ' · ' + entry.id.slice(0, 6) : ''}`;
  };
  const date = (value: string) => {
    const parsed = new Date(value);
    return Number.isNaN(parsed.getTime())
      ? ''
      : new Intl.DateTimeFormat(locale, {
          month: 'short',
          day: 'numeric',
          hour: '2-digit',
          minute: '2-digit',
        }).format(parsed);
  };
  return {
    L,
    chats,
    setChats,
    setCustomPresets,
    personalization,
    editor,
    setEditor,
    entries,
    entryId,
    setEntryId,
    loading,
    error,
    setError,
    status,
    setStatus,
    mounted,
    presets,
    provider,
    vision,
    keyMissing,
    updateSummary,
    openPage,
    storeLabel,
    apiLabel,
    date,
  };
}
export type ChatWorkspace = ReturnType<typeof useChatWorkspace>;
