import { useRef, useState } from 'react';
import type { Locale } from '../../model';
import { errorCode } from '../../backend';
import { aiSettingsText } from '../../ai-settings-i18n';

/** Loading, busy, error and result state shared by the AI settings, and running one action at a
 * time. */
export function useSettingsRun({ locale }: { locale: Locale }) {
  const L = (key: string) => aiSettingsText(locale, key);
  const [loading, setLoading] = useState(true),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(''),
    [result, setResult] = useState(''),
    [testing, setTesting] = useState(false);
  const lock = useRef(false),
    mounted = useRef(true);
  const disabled = loading || busy;
  const run = async (action: () => Promise<void>) => {
    if (lock.current) return;
    lock.current = true;
    setBusy(true);
    setError('');
    setResult('');
    try {
      await action();
    } catch (value) {
      if (mounted.current) setError(errorCode(value));
    } finally {
      lock.current = false;
      if (mounted.current) setBusy(false);
    }
  };
  return {
    L,
    loading,
    setLoading,
    busy,
    error,
    setError,
    result,
    setResult,
    testing,
    setTesting,
    mounted,
    disabled,
    run,
  };
}
export type SettingsRun = ReturnType<typeof useSettingsRun>;
