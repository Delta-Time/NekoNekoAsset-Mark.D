import { useEffect, useRef, useState } from 'react';
import {
  getAiProviders,
  listAiEntries,
  listAiWorkspace,
  removeAiEntry,
  saveAiEntry,
  selectAiEntry,
  testAiDraft,
  testAiEntry,
  errorCode,
} from '../../backend';
import type { AiEntryList, AiProviderPreset } from '../../backend';
import type { SettingsRun } from './useSettingsRun';
import type { PersonalizationDraft } from './usePersonalizationDraft';
import type { AiJobs } from './useAiJobs';

/** The saved API entries and the editor for a new one: a draft is tested before it can be saved,
 * and any edit to it needs a new test. Loads the entries, provider presets and personalization. */
export function useAiEntries({
  runner,
  personal,
  aiJobs,
}: {
  runner: SettingsRun;
  personal: PersonalizationDraft;
  aiJobs: AiJobs;
}) {
  const { setResult, mounted, setError, setLoading, run, setTesting, disabled } = runner;
  const { setPersonalization, setSavedPersonalization, setPersonalizationLoaded } = personal;
  const { jobsActive } = aiJobs;
  const [list, setList] = useState<AiEntryList>({ entries: [], activeEntryId: null }),
    [presets, setPresets] = useState<AiProviderPreset[]>([]);
  const [editing, setEditing] = useState(false),
    [provider, setProvider] = useState('openai'),
    [endpoint, setEndpoint] = useState('https://api.openai.com/v1'),
    [model, setModel] = useState('gpt-4o-mini'),
    [hasKey, setHasKey] = useState(false),
    [proof, setProof] = useState('');
  const keyInput = useRef<HTMLInputElement>(null),
    draftRevision = useRef(0);
  const selected = list.entries.find((entry) => entry.id === list.activeEntryId),
    ready = !!selected && (!selected.keyRequired || selected.keyPresent);
  const invalidate = () => {
    draftRevision.current++;
    setProof('');
    setResult('');
  };
  const clearKey = () => {
    if (keyInput.current) keyInput.current.value = '';
    setHasKey(false);
    invalidate();
  };
  useEffect(() => {
    mounted.current = true;
    void Promise.all([listAiEntries(), getAiProviders(), listAiWorkspace()])
      .then(([entries, providers, workspace]) => {
        if (mounted.current) {
          setList(entries);
          setPresets(providers.presets);
          setPersonalization(workspace.personalization);
          setSavedPersonalization(workspace.personalization);
          setPersonalizationLoaded(true);
        }
      })
      .catch((value) => {
        if (mounted.current) setError(errorCode(value));
      })
      .finally(() => {
        if (mounted.current) setLoading(false);
      });
    return () => {
      mounted.current = false;
      draftRevision.current++;
      if (keyInput.current) keyInput.current.value = '';
    };
  }, []);
  useEffect(() => {
    const input = keyInput.current;
    return () => {
      if (input) input.value = '';
    };
  }, [editing]);
  const accept = (entries: AiEntryList) => {
    if (mounted.current) {
      setList(entries);
      invalidate();
    }
  };
  const openEditor = () => {
    const preset = presets.find((value) => value.id === 'openai') ?? presets[0];
    setProvider(preset?.id ?? 'openai');
    setEndpoint(preset?.endpoint ?? 'https://api.openai.com/v1');
    setModel(preset?.model ?? 'gpt-4o-mini');
    clearKey();
    setError('');
    setEditing(true);
  };
  const cancelEditor = () => {
    clearKey();
    setEditing(false);
    setError('');
  };
  const chooseProvider = (id: string) => {
    const preset = presets.find((value) => value.id === id);
    if (!preset) return;
    setProvider(id);
    setEndpoint(preset.endpoint);
    setModel(preset.model);
    clearKey();
  };
  const draft = () => ({ provider, endpoint, model, key: keyInput.current?.value ?? '' });
  const keyRequired = presets.find((value) => value.id === provider)?.keyRequired !== false;
  const canTest = !!endpoint.trim() && !!model.trim() && (!keyRequired || hasKey);
  const testDraft = () =>
    void run(async () => {
      setTesting(true);
      const revision = draftRevision.current;
      try {
        const value = await testAiDraft(draft());
        if (mounted.current && revision === draftRevision.current) {
          setProof(value.testToken);
          setResult('draftTested');
        }
      } finally {
        if (mounted.current) setTesting(false);
      }
    });
  const save = () => {
    if (!proof || !canTest) return;
    void run(async () => {
      try {
        const value = await saveAiEntry({ ...draft(), testToken: proof });
        accept(value);
        if (mounted.current) setEditing(false);
      } finally {
        if (mounted.current) clearKey();
        else if (keyInput.current) keyInput.current.value = '';
      }
    });
  };
  const select = (id: string) => {
    if (disabled || editing || jobsActive || id === list.activeEntryId) return;
    void run(async () => accept(await selectAiEntry(id)));
  };
  const remove = () => {
    if (!selected || disabled || editing || jobsActive) return;
    void run(async () => {
      try {
        accept(await removeAiEntry(selected.id));
      } catch (value) {
        try {
          accept(await listAiEntries());
        } catch {}
        throw value;
      }
    });
  };
  const testSaved = () => {
    if (!selected || !ready || disabled || editing) return;
    void run(async () => {
      setTesting(true);
      try {
        await testAiEntry(selected.id);
        if (mounted.current) setResult('aiConnected');
      } finally {
        if (mounted.current) setTesting(false);
      }
    });
  };
  return {
    list,
    presets,
    editing,
    provider,
    endpoint,
    setEndpoint,
    model,
    setModel,
    setHasKey,
    proof,
    keyInput,
    selected,
    ready,
    invalidate,
    openEditor,
    cancelEditor,
    chooseProvider,
    canTest,
    testDraft,
    save,
    select,
    remove,
    testSaved,
  };
}
export type AiEntries = ReturnType<typeof useAiEntries>;
