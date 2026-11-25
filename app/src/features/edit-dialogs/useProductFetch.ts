import { useEffect, useRef, useState } from 'react';
import { isCategory } from '../../model';
import { dialog, errorCode, nativeFileAction, previewSource } from '../../backend';
import { dismissAiSourceUrlCandidate } from '../../ai-backend';
import type { ProductMetadata } from '../../backend';
import { normalizeProductUrl, sameTag } from './parts';
import type { ListingDraft } from './useListingDraft';
import type { DialogFrame } from './useDialogFrame';
import type { EditSubject } from './useEditSubject';
import type { DetailDrafts } from './useDetailDrafts';
import type { TagDraft } from './useTagDraft';
import type { ThumbnailDraft } from './useThumbnailDraft';

/** Reading the listing page into the fields, re-reading only its thumbnail, uploading a thumbnail
 * and ignoring an AI-suggested URL. A newer request, an edited URL or closing drops an older reply. */
export function useProductFetch({
  listing,
  frame,
  subject,
  details,
  tagDraft,
  thumbnail,
}: {
  listing: ListingDraft;
  frame: DialogFrame;
  subject: EditSubject;
  details: DetailDrafts;
  tagDraft: TagDraft;
  thumbnail: ThumbnailDraft;
}) {
  const { sourceUrl, setSourceUrl, setTitle, setResetTitle, setIgnoredAiUrls } = listing;
  const { alive, inFlight, setError, box, setBusy } = frame;
  const { p, active } = subject;
  const { setCategory } = details;
  const { setTags, setPendingTag, setRejected } = tagDraft;
  const { setSourceThumbnailUrl, setThumbnailPath, setThumbnailURL, setThumbnailChoice } =
    thumbnail;
  const thumbnailRequest = useRef(0),
    metadataRequest = useRef(0),
    metadataPending = useRef(false),
    draftUrl = useRef(sourceUrl);
  draftUrl.current = sourceUrl;
  const [fetching, setFetching] = useState<'metadata' | 'thumbnail' | null>(null);
  const lastFetched = useRef<{ url: string; thumbnailUrl: string | null } | null>(null);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
      thumbnailRequest.current++;
      metadataRequest.current++;
    };
  }, []);
  const cancelFetch = () => {
    metadataRequest.current++;
    metadataPending.current = false;
    setFetching(null);
  };
  const fetchDraft = async (thumbnailOnly = false) => {
    if (inFlight.current || metadataPending.current || !p.onFetchMetadata) return;
    const url = normalizeProductUrl(sourceUrl);
    if (!url) {
      setError('invalidURL');
      box.current?.querySelector<HTMLInputElement>('#editor-product-url')?.focus();
      return;
    }
    const request = ++metadataRequest.current,
      thumbnailVersion = thumbnailRequest.current;
    metadataPending.current = true;
    setFetching(thumbnailOnly ? 'thumbnail' : 'metadata');
    setError('');
    try {
      const data =
        thumbnailOnly && lastFetched.current?.url === url
          ? lastFetched.current
          : await p.onFetchMetadata(url, thumbnailOnly ? undefined : active?.id);
      if (
        !alive.current ||
        request !== metadataRequest.current ||
        normalizeProductUrl(draftUrl.current) !== url ||
        inFlight.current
      )
        return;
      if (data.thumbnailUrl !== null && !normalizeProductUrl(data.thumbnailUrl))
        throw new Error('invalidMetadata');
      if (!thumbnailOnly) {
        const metadata = data as ProductMetadata;
        const normalized = normalizeProductUrl(metadata.sourceUrl);
        if (
          !normalized ||
          !metadata.title.trim() ||
          !isCategory(metadata.category) ||
          !Array.isArray(metadata.tags) ||
          metadata.tags.some((tag) => typeof tag !== 'string')
        )
          throw new Error('invalidMetadata');
        setSourceUrl(normalized);
        draftUrl.current = normalized;
        setTitle(metadata.title);
        setResetTitle(false);
        setCategory(metadata.category);
        setTags([...metadata.tags]);
        setPendingTag('');
        setRejected((values) =>
          values.filter((tag) => !metadata.tags.some((value) => sameTag(value, tag))),
        );
      }
      const fetchedUrl = thumbnailOnly ? url : (data as ProductMetadata).sourceUrl.trim();
      lastFetched.current = { url: fetchedUrl, thumbnailUrl: data.thumbnailUrl };
      setSourceThumbnailUrl(data.thumbnailUrl ?? '');
      if (thumbnailVersion === thumbnailRequest.current) {
        setThumbnailPath(null);
        setThumbnailURL('');
        setThumbnailChoice(data.thumbnailUrl ? 'source' : 'none');
      }
    } catch (error) {
      if (alive.current && request === metadataRequest.current) setError(errorCode(error));
    } finally {
      if (alive.current && request === metadataRequest.current) {
        metadataPending.current = false;
        setFetching(null);
      }
    }
  };
  const resetThumbnail = () => void fetchDraft(true);
  const ignoreAiUrl = async (url: string) => {
    if (!active || inFlight.current || metadataPending.current) return;
    inFlight.current = true;
    setBusy(true);
    setError('');
    try {
      await dismissAiSourceUrlCandidate({ productId: active.id, url });
      if (alive.current) setIgnoredAiUrls((values) => [...values, url]);
    } catch (error) {
      if (alive.current) setError(errorCode(error));
    } finally {
      inFlight.current = false;
      if (alive.current) setBusy(false);
    }
  };
  const uploadThumbnail = async () => {
    if (inFlight.current) return;
    cancelFetch();
    inFlight.current = true;
    setBusy(true);
    setError('');
    const request = ++thumbnailRequest.current;
    try {
      const chosen = await dialog('open_file', {
        filters: [{ name: 'Images', extensions: ['png', 'jpg', 'jpeg', 'webp'] }],
      });
      if (!chosen || !alive.current || request !== thumbnailRequest.current) return;
      const copied = await nativeFileAction('copy_image', {
        path: chosen.path,
      });
      if (!alive.current || request !== thumbnailRequest.current) return;
      setThumbnailPath(copied.path);
      setThumbnailURL(previewSource(copied.path) ?? '');
      setThumbnailChoice('custom');
    } catch (error) {
      if (alive.current) setError(errorCode(error));
    } finally {
      inFlight.current = false;
      if (alive.current) setBusy(false);
    }
  };
  return {
    draftUrl,
    fetching,
    cancelFetch,
    fetchDraft,
    resetThumbnail,
    ignoreAiUrl,
    uploadThumbnail,
  };
}
export type ProductFetch = ReturnType<typeof useProductFetch>;
