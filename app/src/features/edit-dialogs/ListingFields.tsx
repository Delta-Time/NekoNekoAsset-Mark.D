import { Field, normalizeProductUrl } from './parts';
import type { EditSubject } from './useEditSubject';
import type { ListingDraft } from './useListingDraft';
import type { ProductFetch } from './useProductFetch';
import type { DialogFrame } from './useDialogFrame';

/** Title with its reset to the listing title, the listing URL with Fetch, and suggested URLs. */
export default function ListingFields({
  subject,
  listing,
  productFetch,
  frame,
}: {
  subject: EditSubject;
  listing: ListingDraft;
  productFetch: ProductFetch;
  frame: DialogFrame;
}) {
  const { batch, L, offline, active, p } = subject;
  const { title, setTitle, setResetTitle, sourceUrl, setSourceUrl, aiUrlCandidates, AIText } =
    listing;
  const { draftUrl, cancelFetch, fetching, fetchDraft, ignoreAiUrl } = productFetch;
  const { error, setError } = frame;
  return (
    <>
      {!batch && (
        <>
          <Field label={L('title')}>
            <input
              value={title}
              onChange={(e) => {
                setTitle(e.target.value);
                setResetTitle(false);
              }}
              required
            />
          </Field>
          {!offline && (
            <div className="form-row">
              <small className="muted">
                {L('sourceValue')}: {active?.sourceTitle ?? active?.title}
              </small>
              <button
                type="button"
                onClick={() => {
                  setTitle(active?.sourceTitle ?? active?.title ?? '');
                  setResetTitle(true);
                }}
              >
                {L('resetToSource')}
              </button>
            </div>
          )}
        </>
      )}
      {!batch && (
        <Field label={L('productURL')}>
          <div className="editor-url-control">
            <input
              id="editor-product-url"
              inputMode="url"
              autoCapitalize="none"
              spellCheck={false}
              value={sourceUrl}
              aria-invalid={normalizeProductUrl(sourceUrl) === null}
              onChange={(e) => {
                draftUrl.current = e.target.value;
                setSourceUrl(e.target.value);
                cancelFetch();
                if (error === 'invalidURL') setError('');
              }}
            />
            <button
              type="button"
              disabled={!!fetching || !p.onFetchMetadata}
              aria-busy={fetching === 'metadata'}
              onClick={() => void fetchDraft()}
            >
              {L(fetching === 'metadata' ? 'fetchingProductData' : 'fetchProductData')}
            </button>
          </div>
        </Field>
      )}
      {!batch &&
        aiUrlCandidates.map((candidate) => (
          <div className="field" key={candidate.url}>
            <small>
              {AIText('aiUrlCandidate')} ·{' '}
              {AIText(candidate.status === 'supported' ? 'aiUrlSupported' : 'aiUrlUncertain')}
            </small>
            <small style={{ overflowWrap: 'anywhere' }}>{candidate.url}</small>
            <small title={candidate.evidenceRefs.join(' · ')}>{AIText(candidate.basis)}</small>
            <div className="form-row">
              <button
                type="button"
                disabled={!!fetching}
                onClick={() => {
                  const url = normalizeProductUrl(candidate.url);
                  if (!url) return;
                  cancelFetch();
                  draftUrl.current = url;
                  setSourceUrl(url);
                  if (error === 'invalidURL') setError('');
                }}
              >
                {AIText('aiUseUrl')}
              </button>
              <button
                type="button"
                disabled={!!fetching}
                onClick={() => void ignoreAiUrl(candidate.url)}
              >
                {AIText('aiIgnoreUrl')}
              </button>
            </div>
          </div>
        ))}
    </>
  );
}
