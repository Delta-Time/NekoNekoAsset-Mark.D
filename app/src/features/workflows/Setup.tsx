import {
  type Ctx,
  useAction,
  validPath,
  savePreferences,
  Panel,
  Field,
  displayWindowsPath,
  choose,
  Status,
} from './parts';
import { useState, useEffect } from 'react';
import { nativeWorkflowText } from '../../native-workflow-i18n';
import { isStoreJob, isActive } from './jobs';
import type { Locale } from '../../model';
import { locales, localeNames } from '../../i18n';
import ThemePicker, { defaultThemeOptions } from '../../ThemePicker';
import { signIn, AccountRows } from './Accounts';
import Toggle from '../../Toggle';

export function Setup(c: Ctx) {
  const task = useAction();
  const [path, setPath] = useState(c.nativeState?.roots[0]?.path ?? c.defaultLibraryPath ?? '');
  const [auto, setAuto] = useState(
    typeof c.nativeState?.preferences.autoDownloadAll === 'boolean'
      ? c.nativeState.preferences.autoDownloadAll
      : (c.autoDownloadAll ?? false),
  );
  const ready = c.libraryReady || c.setupReady;
  const [locale, setLocale] = useState(c.locale);
  const [theme, setTheme] = useState(c.themeId ?? 'dark');
  const [provider, setProvider] = useState('BOOTH');
  const [name, setName] = useState('');
  const L = (key: string) => nativeWorkflowText(locale, key);
  useEffect(() => setLocale(c.locale), [c.locale]);
  const actualPath =
    c.nativeState?.roots.find((root) => root.id === c.nativeState?.preferences.libraryRootId)
      ?.path ??
    c.nativeState?.roots[0]?.path ??
    c.defaultLibraryPath;
  useEffect(() => {
    if (actualPath && (c.libraryReady || c.defaultLibraryPath)) setPath(actualPath);
  }, [c.libraryReady, actualPath, c.defaultLibraryPath]);
  const loginPending = (c.nativeState?.jobs ?? []).some(
    (job) => isStoreJob(job.type) && /login/i.test(job.type) && isActive(job),
  );
  const openLocation = async (location: string) => {
    try {
      await c.onLibraryReady?.(location, undefined, true);
    } catch (error) {
      if (ready && actualPath) setPath(actualPath);
      throw error;
    }
  };
  const open = () =>
    task.run(async () => {
      if (validPath(path)) await openLocation(path.trim());
    });
  const finish = () =>
    task.run(async () => {
      if (!ready) return;
      await savePreferences(c, {
        locale,
        theme: c.themeId ?? theme,
        autoDownloadAll: auto,
        setupComplete: true,
      });
      c.onPolicyChange?.(auto);
      c.onClose();
    });
  return (
    <Panel
      actions={
        <button className="primary" disabled={task.busy || loginPending || !ready} onClick={finish}>
          {L('finishSetup')}
        </button>
      }
    >
      <Field label={L('language')}>
        <select
          aria-label={L('language')}
          value={locale}
          onChange={(e) => {
            const next = e.target.value as Locale;
            setLocale(next);
            c.onLocaleChange?.(next);
          }}
        >
          {locales.map((key) => (
            <option key={key} value={key}>
              {localeNames[key]}
            </option>
          ))}
        </select>
      </Field>
      <Field label={L('libraryLocation')}>
        <span className="wf-location-choice">
          <input
            disabled={task.busy}
            readOnly={ready}
            aria-label={L('libraryLocation')}
            value={displayWindowsPath(path)}
            title={displayWindowsPath(path)}
            onChange={(e) => setPath(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && !e.nativeEvent.isComposing && !ready) {
                e.preventDefault();
                void open();
              }
            }}
          />
          <button
            type="button"
            disabled={task.busy}
            onClick={() =>
              task.run(async () => {
                const selected = await choose(c, 'folder', { defaultPath: path || undefined });
                if (selected) {
                  setPath(selected);
                  await openLocation(selected);
                }
              })
            }
          >
            {L('chooseLocation')}
          </button>
          {!ready && (
            <button type="button" disabled={task.busy || !validPath(path)} onClick={open}>
              {L('openLibrary')}
            </button>
          )}
        </span>
      </Field>
      <p className="wf-location-warning" role="status">
        {L('setupLocationRequired')}
      </p>
      {path && !validPath(path) && <Status value="invalidPath" L={L} error />}
      <div className="wf-setup-themes">
        <span className="wf-field-label">{L('theme')}</span>
        <ThemePicker
          locale={locale}
          options={c.themeOptions ?? defaultThemeOptions}
          value={c.themeId ?? theme}
          onChange={(id) => {
            setTheme(id);
            c.onThemeChange?.(id);
          }}
        />
      </div>
      <div className="wf-setup-stores">
        <div className="wf-provider-row">
          <select
            aria-label={L('provider')}
            disabled={task.busy || !ready}
            value={provider}
            onChange={(e) => setProvider(e.target.value)}
          >
            {['BOOTH', 'Gumroad', 'Jinxxy', 'offline'].map((store) => (
              <option key={store} value={store}>
                {store === 'offline' ? L('offlineFiles') : store}
              </option>
            ))}
          </select>
          <input
            disabled={task.busy || !ready || provider === 'offline'}
            aria-label={L('accountName')}
            placeholder={L('accountName')}
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
          <button
            disabled={task.busy || !ready || provider === 'offline'}
            onClick={() =>
              task.run(async () => {
                await signIn(c, name, provider);
                setName('');
              })
            }
          >
            {L('addAccount')}
          </button>
        </div>
        <AccountRows c={{ ...c, locale, L }} task={task} />
      </div>
      <div className="wf-setup-download">
        <Toggle label={L('autoAll')} checked={auto} onChange={setAuto} />
        <span className="wf-download-choice">{L(auto ? 'enabled' : 'disabled')}</span>
      </div>
      <Status value={task.error || (c.defaultLibraryError ?? '')} L={L} error />
    </Panel>
  );
}
