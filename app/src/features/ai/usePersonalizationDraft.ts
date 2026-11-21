import { useRef, useState } from 'react';
import { saveAiPersonalization } from '../../backend';
import { DEFAULT_AI_PERSONALIZATION, normalizeAiAvatar } from '../../ai-personalization';
import type { AiPersonalization } from '../../ai-personalization';
import type { SettingsRun } from './useSettingsRun';

/** The chat personalization being edited: name, avatar, reply language and personality. */
export function usePersonalizationDraft({ runner }: { runner: SettingsRun }) {
  const { setResult, run, mounted } = runner;
  const [personalization, setPersonalization] = useState<AiPersonalization>({
      ...DEFAULT_AI_PERSONALIZATION,
    }),
    [savedPersonalization, setSavedPersonalization] = useState<AiPersonalization>({
      ...DEFAULT_AI_PERSONALIZATION,
    });
  const [personalizationLoaded, setPersonalizationLoaded] = useState(false);
  const avatarInput = useRef<HTMLInputElement>(null);
  const personalizationDirty =
    JSON.stringify(personalization) !== JSON.stringify(savedPersonalization);
  const updatePersonalization = (patch: Partial<AiPersonalization>) => {
    setPersonalization((value) => ({ ...value, ...patch }));
    setResult('');
  };
  const chooseAvatar = (file: File) =>
    void run(async () => {
      const dataUrl = await normalizeAiAvatar(file);
      if (mounted.current) setPersonalization((value) => ({ ...value, avatarDataUrl: dataUrl }));
    });
  const savePersonalization = () =>
    void run(async () => {
      const value = await saveAiPersonalization({
        ...personalization,
        userName: personalization.userName.trim(),
      });
      if (mounted.current) {
        setPersonalization(value);
        setSavedPersonalization(value);
        setResult('personalizationSaved');
      }
    });
  return {
    personalization,
    setPersonalization,
    setSavedPersonalization,
    personalizationLoaded,
    setPersonalizationLoaded,
    avatarInput,
    personalizationDirty,
    updatePersonalization,
    chooseAvatar,
    savePersonalization,
  };
}
export type PersonalizationDraft = ReturnType<typeof usePersonalizationDraft>;
