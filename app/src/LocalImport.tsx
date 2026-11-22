import { categoryKeys } from './model';
import { t } from './i18n';
import { previewSource } from './backend';
import TagPicker from './TagPicker';
import './local-import.css';
import Preview from './Preview';
import {
  type LocalImportProps,
  absolutePath,
  fileName,
  Field,
} from './features/local-import/import-parts';
import { useImportDrafts } from './features/local-import/useImportDrafts';
import { useImportTarget } from './features/local-import/useImportTarget';
import { useImportCheck } from './features/local-import/useImportCheck';
import { useImportActions } from './features/local-import/useImportActions';
import { useBoothImport } from './features/local-import/useBoothImport';
import { BoothFreeImport } from './features/local-import/BoothFreeImport';

export default function LocalImport(c: LocalImportProps) {
  const drafts = useImportDrafts({ c });
  const {
    L,
    sources,
    setSources,
    mode,
    setMode,
    batchDrafts,
    setSelectedPath,
    editorPath,
    name,
    category,
    tags,
    pendingTag,
    sourceUrl,
    thumbnail,
    thumbnailUrl,
    setName,
    setCategory,
    setTags,
    setPendingTag,
    setSourceUrl,
    setThumbnail,
  } = drafts;
  const target = useImportTarget({ c });
  const { rootId, setRootId, libraryGate, setLibraryGate, libraryPath, roots, defaultRootId } =
    target;
  const booth = useBoothImport({ c, L, active: mode === 'booth' });
  const check = useImportCheck({ drafts, c, boothDirty: booth.dirty });
  const { suggestions, aliases, validUrl, batchInvalid, valid } = check;
  const actions = useImportActions({ drafts, target, check, c });
  const {
    dropping,
    setDropping,
    busy,
    error,
    lock,
    fetchMetadata,
    chooseSources,
    chooseThumbnail,
    chooseLibrary,
    submit,
  } = actions;
  const shownError = mode === 'booth' ? booth.error : error;
  const errorView = shownError && (
    <div className="wf-error" role="alert">
      {L(shownError)}
    </div>
  );
  const working = busy || booth.busy;
  if (libraryGate)
    return (
      <>
        <div className="wf-scroll">
          <Field label={L('libraryLocation')}>
            <div className="wf-location-choice">
              <input autoFocus readOnly value={libraryPath} />
              <button disabled={busy} onClick={chooseLibrary}>
                {L('chooseLocation')}
              </button>
            </div>
          </Field>
          {errorView}
        </div>
        <footer className="wf-actions">
          <button disabled={busy} onClick={c.onClose}>
            {L('cancel')}
          </button>
          <button disabled={busy} onClick={() => setLibraryGate(false)}>
            {L('back')}
          </button>
          <button
            className="primary"
            disabled={busy || !absolutePath(libraryPath)}
            onClick={submit}
          >
            {L(busy ? 'running' : 'importNow')}
          </button>
        </footer>
      </>
    );
  return (
    <>
      <div className="wf-scroll">
        <div className="wf-source-mode">
          {(['single', 'batch', 'booth'] as const).map((value) => (
            <label key={value}>
              <input
                type="radio"
                name="local-import-mode"
                checked={mode === value}
                disabled={working}
                onChange={() => setMode(value)}
              />
              {L(
                value === 'single'
                  ? 'singleProduct'
                  : value === 'batch'
                    ? 'batchProducts'
                    : 'boothFree',
              )}
            </label>
          ))}
        </div>
        {mode === 'booth' ? (
          <BoothFreeImport booth={booth} L={L} working={working} />
        ) : (
          <div className={`local-import-columns ${mode === 'batch' ? 'is-batch' : ''}`}>
            <div className="local-import-sources">
              <span className="wf-field-label">{L('filesOrFolder')}</span>
              <div
                className={`data-list local-import-drop ${dropping ? 'is-dropping' : ''}`}
                tabIndex={0}
                role="group"
                aria-label={L('filesOrFolder')}
                onDragOver={(event) => {
                  event.preventDefault();
                  setDropping(true);
                }}
                onDragLeave={() => setDropping(false)}
                onDrop={(event) => {
                  event.preventDefault();
                  setDropping(false);
                }}
              >
                <div className="local-import-drop-label">{L('dropFiles')}</div>
                {sources.map((source) => (
                  <div className="data-row" key={source.path}>
                    {mode === 'batch' ? (
                      <button
                        className="local-import-product"
                        aria-pressed={editorPath === source.path}
                        disabled={busy}
                        title={source.path}
                        onClick={() => setSelectedPath(source.path)}
                      >
                        {batchDrafts[source.path]?.name || fileName(source.path)}
                      </button>
                    ) : (
                      <span title={source.path}>{fileName(source.path)}</span>
                    )}
                    <button
                      disabled={busy}
                      aria-label={`${L('remove')} ${fileName(source.path)}`}
                      onClick={() =>
                        setSources((previous) =>
                          previous.filter((item) => item.path !== source.path),
                        )
                      }
                    >
                      ×
                    </button>
                  </div>
                ))}
              </div>
              <div className="form-row">
                <button disabled={busy} onClick={() => chooseSources(false)}>
                  {L('addFiles')}
                </button>
                <button disabled={busy} onClick={() => chooseSources(true)}>
                  {L('addFolder')}
                </button>
              </div>
            </div>
            <div className="local-import-editor" inert={mode === 'batch' && !sources.length}>
              {batchInvalid && (
                <div className="wf-error" role="alert">
                  {L('local_batch_requires_folder_or_zip')}
                </div>
              )}
              {roots.length > 1 && (
                <Field label={L('currentLocation')}>
                  <select
                    value={rootId || defaultRootId}
                    disabled={busy}
                    onChange={(e) => setRootId(e.target.value)}
                  >
                    {roots.map((root) => (
                      <option key={root.id} value={root.id}>
                        {root.label ?? root.path}
                      </option>
                    ))}
                  </select>
                </Field>
              )}
              <Field label={L('productURL')}>
                <div className="local-import-url">
                  <input
                    value={sourceUrl}
                    disabled={busy}
                    placeholder="https://"
                    onChange={(e) => setSourceUrl(e.target.value)}
                  />
                  <button
                    disabled={busy || !sourceUrl.trim() || !validUrl}
                    aria-busy={busy}
                    onClick={fetchMetadata}
                  >
                    {L(busy ? 'running' : 'fetch')}
                  </button>
                </div>
              </Field>
              <Field label={L('name')}>
                <input value={name} disabled={busy} onChange={(e) => setName(e.target.value)} />
              </Field>
              {!validUrl && (
                <div className="wf-error" role="alert">
                  {L('invalidURL')}
                </div>
              )}
              <Field label={L('category')}>
                <select
                  value={category}
                  disabled={busy}
                  onChange={(e) => setCategory(e.target.value)}
                >
                  {categoryKeys.map((key) => (
                    <option key={key} value={key}>
                      {t(c.locale, key)}
                    </option>
                  ))}
                </select>
              </Field>
              <div className="wf-import-extras">
                <div className="wf-thumbnail">
                  <span>{L('thumbnail')}</span>
                  <button
                    className="wf-thumbnail-pick"
                    disabled={busy}
                    aria-label={L('thumbnail')}
                    onClick={chooseThumbnail}
                  >
                    <Preview
                      src={thumbnail ? previewSource(thumbnail.path) : previewSource(thumbnailUrl)}
                      title={L('thumbnail')}
                      placeholderTitle="＋"
                    />
                  </button>
                  <button disabled={busy} onClick={chooseThumbnail}>
                    {L('chooseFile')}
                  </button>
                  {thumbnail && (
                    <button
                      disabled={busy}
                      title={thumbnail.name}
                      aria-label={L('remove')}
                      onClick={() => setThumbnail(null)}
                    >
                      ×
                    </button>
                  )}
                </div>
                <TagPicker
                  key={mode === 'batch' ? editorPath : 'single'}
                  initialQuery={pendingTag}
                  locale={c.locale}
                  label={L('tags')}
                  value={tags}
                  suggestions={suggestions}
                  aliases={aliases}
                  onChange={(value) => {
                    if (!lock.current) setTags(value);
                  }}
                  onDraftChange={(value) => {
                    if (!lock.current) setPendingTag(value);
                  }}
                />
              </div>
            </div>
          </div>
        )}
        {errorView}
      </div>
      <footer className="wf-actions">
        <button disabled={working} onClick={c.onClose}>
          {L('cancel')}
        </button>
        {mode === 'booth' ? (
          <button
            className="primary"
            disabled={working || !booth.account || (!booth.ids.length && !booth.text.trim())}
            onClick={() => void booth.submit()}
          >
            {L(working ? 'running' : 'importNow')}
          </button>
        ) : (
          <button className="primary" disabled={working || !valid} onClick={submit}>
            {L(working ? 'running' : 'importNow')}
          </button>
        )}
      </footer>
    </>
  );
}
