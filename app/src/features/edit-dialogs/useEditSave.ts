import { getEffectiveTags } from '../../model';
import type { Product } from '../../model';
import { errorCode, invokeAction } from '../../backend';
import { resolveTag } from '../../TagPicker';
import { productAvatarValues } from '../../AvatarPicker';
import { prefixed } from '../appearance/themeConfig';
import { equal, normalizeProductUrl, sameTag, withPendingTag } from './parts';
import type { DialogFrame } from './useDialogFrame';
import type { ProductFetch } from './useProductFetch';
import type { EditSubject } from './useEditSubject';
import type { ListingDraft } from './useListingDraft';
import type { TagDraft } from './useTagDraft';
import type { ThumbnailDraft } from './useThumbnailDraft';
import type { DetailDrafts } from './useDetailDrafts';
import type { AvatarDraft } from './useAvatarDraft';
import type { LicenseDraft } from './useLicenseDraft';
import type { LocalFileDraft } from './useLocalFileDraft';
import type { ThemeDraft } from './useThemeDraft';

/** Saving: a product's changed fields, license and files; each batch product's patch; a theme; or
 * a saved search. A missing name or a bad URL focuses its field instead. */
export function useEditSave({
  frame,
  productFetch,
  subject,
  listing,
  tagDraft,
  thumbnail,
  details,
  avatars,
  license,
  files,
  themeDraft,
}: {
  frame: DialogFrame;
  productFetch: ProductFetch;
  subject: EditSubject;
  listing: ListingDraft;
  tagDraft: TagDraft;
  thumbnail: ThumbnailDraft;
  details: DetailDrafts;
  avatars: AvatarDraft;
  license: LicenseDraft;
  files: LocalFileDraft;
  themeDraft: ThemeDraft;
}) {
  const { setConfirmClose, setError, box, inFlight, setBusy, saved, alive } = frame;
  const { cancelFetch } = productFetch;
  const { p, active, allSuggestions, opening, batch, scoped } = subject;
  const { title, sourceUrl, resetTitle } = listing;
  const {
    tags,
    pendingTag,
    suggestions,
    aliases,
    rejected,
    initialTags,
    tagDelta,
    pendingDelta,
    tagMode,
  } = tagDraft;
  const { sourceThumbnailUrl, thumbnailPath, thumbnailChoice, initialThumbnailChoice } = thumbnail;
  const { note, category, sensitive, noteChanged, ratingChanged, rating, name } = details;
  const { avatarChanged, avatarOptions, selectedAvatars, initialAvatarOptions, batchAvatarDeltas } =
    avatars;
  const { licenseDraft } = license;
  const { localFilesEditable, addedFiles, removedFiles, filesDraft } = files;
  const { themeValid, theme } = themeDraft;
  const invalid = (message: string) => {
    setConfirmClose(false);
    setError(message);
    requestAnimationFrame(() =>
      box.current?.querySelector<HTMLInputElement>('input[required]')?.focus(),
    );
  };
  const save = async () => {
    if (inFlight.current) return;
    cancelFetch();
    inFlight.current = true;
    setBusy(true);
    setError('');
    try {
      if (p.dialog === 'edit' && active) {
        if (!title.trim()) {
          invalid('nameRequired');
          return;
        }
        const finalUrl = normalizeProductUrl(sourceUrl);
        if (finalUrl === null) {
          setConfirmClose(false);
          setError('invalidURL');
          requestAnimationFrame(() =>
            box.current?.querySelector<HTMLInputElement>('#editor-product-url')?.focus(),
          );
          return;
        }
        const finalTags = withPendingTag(tags, pendingTag, suggestions, aliases);
        const finalRejected = pendingTag.trim()
          ? rejected.filter((tag) => !sameTag(tag, resolveTag(pendingTag, allSuggestions, aliases)))
          : rejected;
        const patch: Partial<Product> = {
          custom: true,
          sourceTitle: active.sourceTitle ?? active.title,
        };
        if (title !== opening?.title || resetTitle) {
          patch.title = title.trim();
          patch.titleOverride = !resetTitle;
        }
        if (finalUrl !== (opening?.sourceUrl ?? '')) patch.sourceUrl = finalUrl;
        if (sourceThumbnailUrl !== opening?.sourceThumbnailUrl)
          patch.sourceThumbnailUrl = sourceThumbnailUrl;
        if (note !== opening?.note) patch.note = note;
        if (category !== opening?.category) {
          patch.category = category;
          patch.categoryOverride = true;
        }
        if (avatarChanged && opening)
          patch.ownedAvatars = productAvatarValues(
            opening,
            avatarOptions,
            selectedAvatars,
            initialAvatarOptions,
          );
        if (sensitive !== opening?.sensitive) patch.sensitive = sensitive;
        if (!equal(finalTags, initialTags)) {
          patch.tags = finalTags;
          patch.sourceTags = active.sourceTags ?? [...active.tags];
        }
        if (!equal(finalRejected, opening?.rejectedTags ?? [])) patch.rejectedTags = finalRejected;
        if (thumbnailPath !== undefined || thumbnailChoice !== initialThumbnailChoice) {
          if (thumbnailChoice === 'custom') {
            if (thumbnailPath) patch.thumbnailSourcePath = thumbnailPath;
            else patch.thumbnailHidden = false;
          } else if (thumbnailChoice === 'source' || thumbnailPath === null)
            patch.thumbnailSourcePath = null;
          else {
            patch.thumbnailOverrideUrl = '';
            patch.previewUrl = '';
          }
        }
        await p.onPatch([active.id], patch);
        if (licenseDraft !== undefined)
          await invokeAction('licenses', 'set', { productId: active.id, cells: licenseDraft });
        if (localFilesEditable && (addedFiles.length || removedFiles.length))
          await p.onLocalFiles!(active.id, {
            draftId: filesDraft.current,
            addSources: addedFiles,
            removeFileIds: removedFiles,
          });
      } else if (batch) {
        const delta = withPendingTag(tagDelta, pendingDelta, suggestions, aliases);
        const changes = scoped
          .map((item) => {
            const patch: Partial<Product> = {};
            if (category) {
              patch.category = category;
              patch.categoryOverride = true;
            }
            if (noteChanged) patch.note = note;
            if (ratingChanged) patch.sensitive = rating === 'on';
            if (tagMode === 'addTag' && delta.length) {
              patch.tags = [...new Set([...getEffectiveTags(item), ...delta])];
              patch.rejectedTags = (item.rejectedTags ?? []).filter(
                (tag) => !delta.some((value) => sameTag(value, tag)),
              );
            }
            if (tagMode === 'removeTag' && delta.length) {
              patch.tags = getEffectiveTags(item).filter(
                (tag) => !delta.some((value) => sameTag(value, tag)),
              );
              patch.rejectedTags = [
                ...new Set([
                  ...(item.rejectedTags ?? []),
                  ...item.tags.filter((tag) => delta.some((value) => sameTag(value, tag))),
                ]),
              ];
            }
            if (patch.tags) patch.sourceTags = item.sourceTags ?? [...item.tags];
            const avatarDelta = batchAvatarDeltas.find((change) => change.id === item.id)?.delta;
            return {
              id: item.id,
              patch,
              ...(avatarDelta && (avatarDelta.add.length || avatarDelta.remove.length)
                ? { ownedAvatarsDelta: avatarDelta }
                : {}),
            };
          })
          .filter((change) => Object.keys(change.patch).length || change.ownedAvatarsDelta);
        if (changes.length) {
          if (p.onBatchPatch) await p.onBatchPatch(changes);
          else for (const change of changes) await p.onPatch([change.id], change.patch);
        }
      } else if (p.dialog === 'theme') {
        if (!themeValid) {
          invalid('invalidTheme');
          return;
        }
        if (!name.trim()) {
          invalid('themeNameRequired');
          return;
        }
        await p.onThemeSave(prefixed(theme), name.trim());
        saved.current = true;
        p.onThemePreview(null);
      } else if (p.dialog === 'saveSearch') {
        if (!name.trim()) {
          invalid('nameRequired');
          return;
        }
        await p.onSaveSearch(name.trim());
      }
      p.onClose();
    } catch (error) {
      if (alive.current) {
        setConfirmClose(false);
        setError(errorCode(error));
      }
    } finally {
      inFlight.current = false;
      if (alive.current) setBusy(false);
    }
  };
  return { save };
}
export type EditSave = ReturnType<typeof useEditSave>;
