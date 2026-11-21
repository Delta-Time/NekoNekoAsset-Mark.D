import { Plus, Trash2, UserRound } from 'lucide-react';
import type { NativeState } from './backend';
import { startAi, cancelAi, retryAi } from './ai-backend';
import type { Locale } from './model';
import Toggle from './Toggle';
import { getAiPersonalities, type AiPersonalization } from './ai-personalization';
import './AiSettings.css';
import { useSettingsRun } from './features/ai/useSettingsRun';
import { useAiJobs } from './features/ai/useAiJobs';
import { usePersonalizationDraft } from './features/ai/usePersonalizationDraft';
import { useAiEntries } from './features/ai/useAiEntries';

export interface AiSettingsProps {
  locale: Locale;
  nativeState?: NativeState;
}
export default function AiSettings({ locale, nativeState }: AiSettingsProps) {
  const runner = useSettingsRun({ locale });
  const { L, loading, busy, error, result, testing, mounted, disabled, run } = runner;
  const aiJobs = useAiJobs({ runner, nativeState });
  const { jobId, setJobId, scopes, auto, featuresEnabled, jobsActive, job, jobActive, savePrefs } =
    aiJobs;
  const personal = usePersonalizationDraft({ runner });
  const {
    personalization,
    personalizationLoaded,
    avatarInput,
    personalizationDirty,
    updatePersonalization,
    chooseAvatar,
    savePersonalization,
  } = personal;
  const apiEntries = useAiEntries({ runner, personal, aiJobs });
  const {
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
  } = apiEntries;
  const personalizationDisabled = disabled || editing || !personalizationLoaded;
  const connectionStatus =
    error || result === 'draftTested' || result === 'aiConnected' || testing ? (
      <div
        className={'ai-status' + (error ? ' error' : testing ? '' : ' success')}
        role={error ? 'alert' : 'status'}
      >
        {L(error || (testing ? 'aiTesting' : result))}
      </div>
    ) : null;
  return (
    <div className="wf-scroll ai-settings">
      <div className="ai-entry-list" role="listbox" aria-label={L('entries')} aria-busy={loading}>
        {!loading && !list.entries.length && (
          <span className="ai-entry-empty">{L('emptyEntries')}</span>
        )}
        {list.entries.map((entry) => (
          <button
            type="button"
            className="ai-entry"
            role="option"
            aria-selected={entry.id === list.activeEntryId}
            disabled={disabled || editing || jobsActive}
            key={entry.id}
            title={[
              presets.find((value) => value.id === entry.provider)?.label ?? entry.provider,
              entry.model,
              entry.endpoint,
            ].join('\n')}
            onClick={() => select(entry.id)}
          >
            <span className="ai-entry-provider">
              {presets.find((value) => value.id === entry.provider)?.label ?? entry.provider}
            </span>
            <span className="ai-entry-model">{entry.model}</span>
            {entry.keyPresent && (
              <span className="ai-entry-saved" aria-label={L('aiSaved')} title={L('aiSaved')}>
                ✓
              </span>
            )}
          </button>
        ))}
      </div>
      <div className="form-row">
        <button
          type="button"
          className="ai-add"
          disabled={disabled || editing || jobsActive}
          onClick={openEditor}
        >
          <Plus />
          {L('addEntry')}
        </button>
        <button
          type="button"
          className="danger"
          disabled={disabled || editing || jobsActive || !selected}
          onClick={remove}
        >
          <Trash2 />
          {L('remove')}
        </button>
        <button type="button" disabled={disabled || editing || !ready} onClick={testSaved}>
          {L('test')}
        </button>
      </div>
      {!editing && connectionStatus}
      {editing && (
        <form
          className="ai-editor"
          onSubmit={(event) => {
            event.preventDefault();
            save();
          }}
        >
          <label className="field">
            <span>{L('provider')}</span>
            <select
              disabled={busy}
              value={provider}
              onChange={(event) => chooseProvider(event.target.value)}
            >
              {presets.map((preset) => (
                <option key={preset.id} value={preset.id}>
                  {preset.label}
                </option>
              ))}
            </select>
          </label>
          <label className="field">
            <span>{L('endpoint')}</span>
            <input
              disabled={busy}
              value={endpoint}
              onChange={(event) => {
                setEndpoint(event.target.value);
                invalidate();
              }}
            />
          </label>
          <label className="field">
            <span>{L('model')}</span>
            <input
              disabled={busy}
              value={model}
              onChange={(event) => {
                setModel(event.target.value);
                invalidate();
              }}
            />
          </label>
          <label className="field">
            <span>{L('key')}</span>
            <input
              ref={keyInput}
              type="password"
              autoComplete="off"
              autoCapitalize="none"
              spellCheck={false}
              disabled={busy}
              onChange={(event) => {
                setHasKey(event.currentTarget.value.trim().length > 0);
                invalidate();
              }}
            />
          </label>
          <div className="form-row">
            <button type="button" disabled={busy || !canTest} onClick={testDraft}>
              {L('test')}
            </button>
            <button type="submit" className="primary" disabled={busy || !proof || !canTest}>
              {L('save')}
            </button>
            <button type="button" disabled={busy} onClick={cancelEditor}>
              {L('cancel')}
            </button>
          </div>
          {connectionStatus}
        </form>
      )}
      <section className="wf-section ai-personalization" aria-labelledby="ai-personalization-title">
        <h4 id="ai-personalization-title">{L('personalization')}</h4>
        <div className="field ai-avatar-field">
          <span>{L('avatar')}</span>
          <div className="ai-avatar-controls">
            <div className="ai-avatar-preview">
              {personalization.avatarDataUrl ? (
                <img
                  src={personalization.avatarDataUrl}
                  alt={personalization.userName || L('avatar')}
                />
              ) : (
                <UserRound aria-hidden="true" />
              )}
            </div>
            <button
              type="button"
              disabled={personalizationDisabled}
              onClick={() => avatarInput.current?.click()}
            >
              {L('changeAvatar')}
            </button>
            <button
              type="button"
              disabled={personalizationDisabled || !personalization.avatarDataUrl}
              onClick={() => updatePersonalization({ avatarDataUrl: null })}
            >
              {L('remove')}
            </button>
            <input
              ref={avatarInput}
              type="file"
              accept="image/jpeg,image/png,image/webp"
              hidden
              onChange={(event) => {
                const file = event.currentTarget.files?.[0];
                event.currentTarget.value = '';
                if (file) chooseAvatar(file);
              }}
            />
          </div>
        </div>
        <label className="field">
          <span>{L('userName')}</span>
          <input
            disabled={personalizationDisabled}
            maxLength={80}
            value={personalization.userName}
            placeholder={L('userNamePlaceholder')}
            onChange={(event) => updatePersonalization({ userName: event.currentTarget.value })}
          />
        </label>
        <label className="field">
          <span>{L('replyLanguage')}</span>
          <select
            disabled={personalizationDisabled}
            value={personalization.replyLanguage}
            onChange={(event) =>
              updatePersonalization({
                replyLanguage: event.currentTarget.value as AiPersonalization['replyLanguage'],
              })
            }
          >
            <option value="app">{L('followAppLanguage')}</option>
            <option value="en">English</option>
            <option value="ja">日本語</option>
            <option value="ko">한국어</option>
            <option value="zh-Hans">简体中文</option>
            <option value="zh-Hant">繁體中文</option>
          </select>
        </label>
        <label className="field">
          <span>{L('replyPersonality')}</span>
          <select
            disabled={personalizationDisabled}
            value={personalization.personality}
            onChange={(event) =>
              updatePersonalization({
                personality: event.currentTarget.value as AiPersonalization['personality'],
              })
            }
          >
            {getAiPersonalities(locale).map((personality) => (
              <option key={personality.id} value={personality.id}>
                {personality.title}
              </option>
            ))}
          </select>
        </label>
        {personalization.personality === 'custom' && (
          <label className="field">
            <span>{L('customPrompt')}</span>
            <textarea
              disabled={personalizationDisabled}
              maxLength={4000}
              value={personalization.customPrompt ?? ''}
              onChange={(event) =>
                updatePersonalization({ customPrompt: event.currentTarget.value })
              }
            />
          </label>
        )}
        <div className="form-row">
          <button
            type="button"
            className="primary"
            disabled={
              personalizationDisabled ||
              !personalizationDirty ||
              (personalization.personality === 'custom' && !personalization.customPrompt?.trim())
            }
            onClick={savePersonalization}
          >
            {L('save')}
          </button>
          {result === 'personalizationSaved' && (
            <span className="ai-personalization-saved" role="status">
              {L('personalizationSaved')}
            </span>
          )}
        </div>
      </section>
      <div className="wf-section">
        <button
          type="button"
          disabled={disabled || editing || jobsActive || !ready || !featuresEnabled}
          onClick={() =>
            void run(async () => {
              const value = await startAi({ mode: 'organize', productIds: [], scopes });
              if (mounted.current) setJobId(value.jobId);
            })
          }
        >
          {L('organizeTags')}
        </button>
        {jobId && (
          <div className="wf-progress">
            <span role="status">{L(job?.errorCode || job?.state || 'queued')}</span>
            {jobActive &&
              (typeof job?.progress === 'number' ? (
                <progress value={job.progress} max={100} />
              ) : (
                <progress />
              ))}
            {jobActive ? (
              <button
                disabled={busy}
                onClick={() =>
                  void run(async () => {
                    await cancelAi(jobId);
                  })
                }
              >
                {L('cancel')}
              </button>
            ) : (
              job &&
              ['failed', 'needsAction', 'cancelled', 'interrupted', 'paused'].includes(
                job.state,
              ) && (
                <button
                  disabled={busy || !featuresEnabled}
                  onClick={() =>
                    void run(async () => {
                      const value = await retryAi(jobId);
                      if (mounted.current) setJobId(value.jobId);
                    })
                  }
                >
                  {L('retry')}
                </button>
              )
            )}
          </div>
        )}
      </div>
      <div className="wf-section">
        <Toggle
          label={L('autoAI')}
          checked={auto}
          disabled={disabled || editing || (!ready && !auto)}
          onChange={(value) => savePrefs(scopes, value)}
        />
        <div className="ai-scopes">
          {(['title', 'tags', 'description', 'files', 'images'] as const).map((field) => (
            <label key={field}>
              <input
                type="checkbox"
                checked={scopes[field]}
                disabled={
                  disabled ||
                  editing ||
                  (field === 'images' && !selected?.visionSupported && !scopes.images)
                }
                onChange={(event) => savePrefs({ ...scopes, [field]: event.target.checked })}
              />
              {L('scope-' + field)}
            </label>
          ))}
        </div>
      </div>
      {!connectionStatus && (error || (result && result !== 'personalizationSaved') || busy) && (
        <div className={'ai-status' + (error ? ' error' : '')} role={error ? 'alert' : 'status'}>
          {L(error || result || (testing ? 'aiTesting' : 'running'))}
        </div>
      )}
    </div>
  );
}
