import type { CSSProperties } from 'react';
import { builtInTheme, tokenLabels, validColor } from '../appearance/themeConfig';
import { Field } from './parts';
import type { EditSubject } from './useEditSubject';
import type { DetailDrafts } from './useDetailDrafts';
import type { ThemeDraft } from './useThemeDraft';
import type { DialogFrame } from './useDialogFrame';

/** The theme editor: name, reset, import and export, a colour row per token and a preview. */
export default function ThemeFields({
  subject,
  details,
  themeDraft,
  frame,
}: {
  subject: EditSubject;
  details: DetailDrafts;
  themeDraft: ThemeDraft;
  frame: DialogFrame;
}) {
  const { L } = subject;
  const { name, setName } = details;
  const { setTheme, importTheme, exportTheme, theme } = themeDraft;
  const { setError } = frame;
  return (
    <>
      <Field label={L('themeName')}>
        <input value={name} required onChange={(e) => setName(e.target.value)} />
      </Field>
      <div className="form-row">
        <button
          type="button"
          onClick={() => {
            setTheme({ ...builtInTheme });
            setError('');
          }}
        >
          {L('reset')}
        </button>
        <button type="button" onClick={() => void importTheme()}>
          {L('importTheme')}
        </button>
        <button type="button" onClick={() => void exportTheme()}>
          {L('exportTheme')}
        </button>
      </div>
      {Object.keys(builtInTheme).map((key) => (
        <div className="form-row" key={key}>
          <label htmlFor={`theme-${key}`}>{L(tokenLabels[key])}</label>
          <div className="color-field">
            <input
              type="color"
              aria-label={`${L(tokenLabels[key])} ${L('preview')}`}
              value={validColor(theme[key]) ? theme[key] : '#000000'}
              onChange={(e) => setTheme((v) => ({ ...v, [key]: e.target.value }))}
            />
            <input
              id={`theme-${key}`}
              type="text"
              value={theme[key]}
              aria-invalid={!validColor(theme[key])}
              onChange={(e) => setTheme((v) => ({ ...v, [key]: e.target.value }))}
            />
          </div>
        </div>
      ))}
      <div
        style={
          {
            ...Object.fromEntries(
              Object.entries(theme)
                .filter(([, v]) => validColor(v))
                .map(([k, v]) => [`--${k}`, v]),
            ),
            background: 'var(--bg)',
            color: 'var(--text)',
            border: '1px solid var(--border)',
            borderRadius: 5,
            padding: 14,
          } as CSSProperties
        }
      >
        <strong>{L('preview')}</strong>
        <div className="form-row">
          <button type="button">{L('edit')}</button>
          <button type="button" className="primary">
            {L('selected')}
          </button>
          <button
            type="button"
            style={{
              background: 'var(--secondary-accent)',
              borderColor: 'var(--secondary-accent)',
              color: 'var(--secondary-accent-text)',
            }}
          >
            {L('unity')}
          </button>
          <button type="button" disabled>
            {L('disabled')}
          </button>
        </div>
        <div className="form-row">
          <span className="badge ready">{L('ready')}</span>
          <span className="badge warning">{L('attention')}</span>
          <span className="badge error">{L('failed')}</span>
        </div>
      </div>
    </>
  );
}
