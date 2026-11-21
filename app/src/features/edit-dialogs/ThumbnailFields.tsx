import { ImageUp } from 'lucide-react';
import Preview from '../../Preview';
import Toggle from '../../Toggle';
import { licenseText } from '../../license-i18n';
import { Field } from './parts';
import type { EditSubject } from './useEditSubject';
import type { ProductFetch } from './useProductFetch';
import type { ThumbnailDraft } from './useThumbnailDraft';
import type { DetailDrafts } from './useDetailDrafts';
import type { LicenseDraft } from './useLicenseDraft';

/** The thumbnail (click to upload) beside the note, then Reset thumbnail, Edit license and R-18. */
export default function ThumbnailFields({
  subject,
  productFetch,
  thumbnail,
  details,
  license,
}: {
  subject: EditSubject;
  productFetch: ProductFetch;
  thumbnail: ThumbnailDraft;
  details: DetailDrafts;
  license: LicenseDraft;
}) {
  const { L, p, batch } = subject;
  const { uploadThumbnail, fetching, resetThumbnail } = productFetch;
  const { thumbnailPreview } = thumbnail;
  const { note, setNote, sensitive, setSensitive } = details;
  const { licenseTrigger, setLicenseOpen } = license;
  return (
    <>
      <div className="editor-thumbnail-note">
        <div>
          <div className="field">
            <span>{L('thumbnail')}</span>
          </div>
          <button
            type="button"
            className="editor-thumbnail-preview"
            title={L('uploadThumbnail')}
            aria-label={L('uploadThumbnail')}
            onClick={() => void uploadThumbnail()}
          >
            <Preview
              src={thumbnailPreview}
              title={L('thumbnail')}
              placeholderTitle={L('noImage')}
            />
            <span className="editor-thumbnail-upload" aria-hidden="true">
              <ImageUp size={26} />
            </span>
          </button>
        </div>
        <Field label={L('note')}>
          <textarea rows={5} value={note} onChange={(e) => setNote(e.target.value)} />
        </Field>
      </div>
      <div className="editor-thumbnail-bottom">
        <div className="editor-thumbnail-actions">
          <button
            type="button"
            disabled={!!fetching || !p.onFetchMetadata}
            aria-busy={fetching === 'thumbnail'}
            onClick={resetThumbnail}
          >
            {L(fetching === 'thumbnail' ? 'resettingThumbnail' : 'resetThumbnail')}
          </button>
        </div>
        {p.licenseEnabled && !batch && (
          <button
            type="button"
            ref={licenseTrigger}
            className="editor-license-button"
            onClick={() => setLicenseOpen(true)}
          >
            {licenseText(p.locale, 'editLicense')}
          </button>
        )}
        <label className="editor-sensitive-toggle" htmlFor="editor-sensitive">
          <Toggle id="editor-sensitive" label="R-18" checked={sensitive} onChange={setSensitive} />
        </label>
      </div>
    </>
  );
}
