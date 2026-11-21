import { useEffect, useRef } from 'react';
import { equal, focusable } from './parts';
import type { EditSubject } from './useEditSubject';
import type { ListingDraft } from './useListingDraft';
import type { ThumbnailDraft } from './useThumbnailDraft';
import type { DetailDrafts } from './useDetailDrafts';
import type { AvatarDraft } from './useAvatarDraft';
import type { TagDraft } from './useTagDraft';
import type { LicenseDraft } from './useLicenseDraft';
import type { ThemeDraft } from './useThemeDraft';
import type { LocalFileDraft } from './useLocalFileDraft';
import type { DialogFrame } from './useDialogFrame';
import type { ProductFetch } from './useProductFetch';

/** Whether anything changed, closing (asking first when it did), Escape, the Tab focus trap and
 * focus on the confirmation's Stay button. */
export function useCloseGuard({
  subject,
  listing,
  thumbnail,
  details,
  avatars,
  tagDraft,
  license,
  themeDraft,
  files,
  frame,
  productFetch,
}: {
  subject: EditSubject;
  listing: ListingDraft;
  thumbnail: ThumbnailDraft;
  details: DetailDrafts;
  avatars: AvatarDraft;
  tagDraft: TagDraft;
  license: LicenseDraft;
  themeDraft: ThemeDraft;
  files: LocalFileDraft;
  frame: DialogFrame;
  productFetch: ProductFetch;
}) {
  const { opening, p, batch } = subject;
  const { resetTitle, title, sourceUrl } = listing;
  const {
    sourceThumbnailUrl,
    thumbnailPath,
    thumbnailChoice,
    initialThumbnailChoice,
    thumbnailURL,
    initialThumbnailURL,
  } = thumbnail;
  const { note, category, sensitive, name, noteChanged, ratingChanged } = details;
  const { avatarChanged, batchAvatarChanged, avatarPickerShown } = avatars;
  const { tags, initialTags, pendingTag, rejected, tagMode, tagDelta, pendingDelta } = tagDraft;
  const { licenseDraft, licenseShown } = license;
  const { theme, initialTheme } = themeDraft;
  const { addedFiles, removedFiles } = files;
  const {
    inFlight,
    confirmClose,
    setConfirmClose,
    box,
    confirmationOpen,
    confirmation,
    saved,
    stayButton,
  } = frame;
  const { cancelFetch } = productFetch;
  const singleChanged =
    opening &&
    (resetTitle ||
      title !== opening.title ||
      sourceUrl !== (opening.sourceUrl ?? '') ||
      sourceThumbnailUrl !== opening.sourceThumbnailUrl ||
      note !== opening.note ||
      category !== opening.category ||
      avatarChanged ||
      !equal(tags, initialTags) ||
      !!pendingTag.trim() ||
      !equal(rejected, opening.rejectedTags ?? []) ||
      thumbnailPath !== undefined ||
      thumbnailChoice !== initialThumbnailChoice ||
      thumbnailURL !== initialThumbnailURL ||
      sensitive !== opening.sensitive ||
      licenseDraft !== undefined);
  const dirty =
    p.dialog === 'theme'
      ? !equal(theme, initialTheme.current) || name !== (p.themeName ?? '')
      : batch
        ? !!category ||
          noteChanged ||
          ratingChanged ||
          batchAvatarChanged ||
          (tagMode !== 'unchanged' && (!!tagDelta.length || !!pendingDelta.trim()))
        : p.dialog === 'edit'
          ? !!singleChanged || !!addedFiles.length || !!removedFiles.length
          : !!name.trim();
  const close = () => {
    if (inFlight.current) return;
    cancelFetch();
    if (p.dialog === 'theme') p.onThemePreview(null);
    p.onClose();
  };
  const requestClose = () => {
    if (inFlight.current) return;
    cancelFetch();
    confirmClose ? setConfirmClose(false) : dirty ? setConfirmClose(true) : close();
  };
  const closeRef = useRef(requestClose);
  closeRef.current = requestClose;
  useEffect(() => {
    const prior = document.activeElement as HTMLElement | null;
    const elements = focusable(box.current);
    (elements.find((element) => element.tagName !== 'BUTTON') ?? elements[0])?.focus();
    const key = (e: KeyboardEvent) => {
      if (e.isComposing || e.keyCode === 229) return;
      if (avatarPickerShown.current || licenseShown.current) return;
      if (e.key === 'Escape') {
        e.stopPropagation();
        e.preventDefault();
        closeRef.current();
      }
      if (e.key === 'Tab') {
        const elements = focusable(confirmationOpen.current ? confirmation.current : box.current);
        const first = elements[0],
          last = elements[elements.length - 1];
        if (
          e.shiftKey &&
          (document.activeElement === first ||
            !elements.includes(document.activeElement as HTMLElement))
        ) {
          e.preventDefault();
          last?.focus();
        } else if (
          !e.shiftKey &&
          (document.activeElement === last ||
            !elements.includes(document.activeElement as HTMLElement))
        ) {
          e.preventDefault();
          first?.focus();
        }
      }
    };
    document.addEventListener('keydown', key, true);
    return () => {
      document.removeEventListener('keydown', key, true);
      if (prior?.isConnected) prior.focus();
      if (p.dialog === 'theme' && !saved.current) p.onThemePreview(null);
    };
  }, []);
  useEffect(() => {
    if (!confirmClose) return;
    const prior = document.activeElement as HTMLElement | null;
    stayButton.current?.focus();
    return () => {
      if (prior?.isConnected) prior.focus();
    };
  }, [confirmClose]);
  return { dirty, close, requestClose };
}
export type CloseGuard = ReturnType<typeof useCloseGuard>;
