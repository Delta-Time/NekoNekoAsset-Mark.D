import type { Locale, Product, CustomCategory } from './model';
import type { AvatarOption, TagOption, ProductMetadata, ProductOverlayChange } from './backend';
import './UnityWorkflow.css';
import './editor-styles.css';
import { LicenseEditor } from './Licenses';
import { Field, operationMessage } from './features/edit-dialogs/parts';
import { CollectionDialog } from './features/edit-dialogs/CollectionDialog';
import { ManagerDialog } from './features/edit-dialogs/ManagerDialog';
import { useEditSubject } from './features/edit-dialogs/useEditSubject';
import { useDialogFrame } from './features/edit-dialogs/useDialogFrame';
import { useListingDraft } from './features/edit-dialogs/useListingDraft';
import { useDetailDrafts } from './features/edit-dialogs/useDetailDrafts';
import { useLicenseDraft } from './features/edit-dialogs/useLicenseDraft';
import { useAvatarDraft } from './features/edit-dialogs/useAvatarDraft';
import { useTagDraft } from './features/edit-dialogs/useTagDraft';
import { useThumbnailDraft } from './features/edit-dialogs/useThumbnailDraft';
import { useProductFetch } from './features/edit-dialogs/useProductFetch';
import { useLocalFileDraft } from './features/edit-dialogs/useLocalFileDraft';
import { useThemeDraft } from './features/edit-dialogs/useThemeDraft';
import { useCloseGuard } from './features/edit-dialogs/useCloseGuard';
import { useEditSave } from './features/edit-dialogs/useEditSave';
import ListingFields from './features/edit-dialogs/ListingFields';
import CategoryFields from './features/edit-dialogs/CategoryFields';
import TagFields from './features/edit-dialogs/TagFields';
import BatchFields from './features/edit-dialogs/BatchFields';
import ThumbnailFields from './features/edit-dialogs/ThumbnailFields';
import LocalFilesField from './features/edit-dialogs/LocalFilesField';
import ThemeFields from './features/edit-dialogs/ThemeFields';
import EditAvatarPicker from './features/edit-dialogs/EditAvatarPicker';
import UnsavedChanges from './features/edit-dialogs/UnsavedChanges';

export interface EditDialogsProps {
  dialog: string | null;
  locale: Locale;
  products: Product[];
  selected: string[];
  active: Product | undefined;
  aiFeaturesEnabled?: boolean;
  licenseEnabled?: boolean;
  onClose: () => void;
  onPatch: (ids: string[], patch: Partial<Product>) => void | Promise<void>;
  onBatchPatch?: (changes: ProductOverlayChange[]) => void | Promise<void>;
  onNotice: (s: string) => void;
  onSaveSearch: (name: string) => void | Promise<void>;
  onCollection: (name: string, ids: string[], included: boolean) => void | Promise<void>;
  collections?: Record<string, string[]>;
  collectionIds?: Record<string, string>;
  collectionBusy?: boolean;
  savedSearches?: { id?: string; name: string }[];
  tagOptions?: TagOption[];
  avatarOptions?: AvatarOption[];
  avatarLabel?: (id: string) => string;
  onRefreshAvatars?: () => void | Promise<void>;
  onFetchMetadata?: (url: string, editedProductId?: string) => Promise<ProductMetadata>;
  onLocalFiles?: (
    productId: string,
    args: { draftId: string; addSources: string[]; removeFileIds: string[] },
  ) => Promise<unknown>;
  onCreateCollection?: (name: string) => void | Promise<void>;
  onRenameCollection?: (oldName: string, newName: string) => void | Promise<void>;
  onDeleteCollection?: (name: string, capturedId?: string) => void | Promise<void>;
  onRenameSearch?: (index: number, name: string) => void | Promise<void>;
  customCategories?: CustomCategory[];
  onCreateCategory?: (name: string) => Promise<void>;
  onRenameCategory?: (id: string, name: string) => Promise<void>;
  onDeleteCategory?: (id: string) => Promise<void>;
  onDeleteSearch?: (index: number, capturedId?: string) => void | Promise<void>;
  themeTokens: Record<string, string>;
  onThemePreview: (tokens: Record<string, string> | null) => void;
  onThemeSave: (tokens: Record<string, string>, name?: string) => void | Promise<void>;
  themeName?: string;
}

/** Edit, Batch edit, Save search and the theme editor. */
function DialogBody(props: EditDialogsProps) {
  const subject = useEditSubject({ props });
  const { p, L, active, batch, scoped } = subject;
  const frame = useDialogFrame();
  const { error, busy, confirmClose, box } = frame;
  const listing = useListingDraft({ subject });
  const details = useDetailDrafts({ subject });
  const { name, setName } = details;
  const license = useLicenseDraft();
  const { licenseDraft, setLicenseDraft, licenseOpen, setLicenseOpen } = license;
  const avatars = useAvatarDraft({ subject, frame });
  const { avatarPickerOpen } = avatars;
  const tagDraft = useTagDraft({ subject });
  const thumbnail = useThumbnailDraft({ subject });
  const productFetch = useProductFetch({ listing, frame, subject, details, tagDraft, thumbnail });
  const files = useLocalFileDraft({ subject, frame, avatars });
  const { localFilesEditable } = files;
  const themeDraft = useThemeDraft({ subject, frame, details });
  const { themeValid, themeDialogStyle } = themeDraft;
  const closeGuard = useCloseGuard({
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
  });
  const { dirty, close, requestClose } = closeGuard;
  const editSave = useEditSave({
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
  });
  const { save } = editSave;
  const labelKey = p.dialog === 'saveSearch' ? 'saveSearch' : (p.dialog ?? 'edit');
  const listingFields = (
    <ListingFields subject={subject} listing={listing} productFetch={productFetch} frame={frame} />
  );
  const categoryFields = <CategoryFields subject={subject} details={details} avatars={avatars} />;
  const tagFields = <TagFields subject={subject} tagDraft={tagDraft} />;
  const batchFields = <BatchFields subject={subject} details={details} />;
  const thumbnailFields = (
    <ThumbnailFields
      subject={subject}
      productFetch={productFetch}
      thumbnail={thumbnail}
      details={details}
      license={license}
    />
  );
  const localFilesField = <LocalFilesField subject={subject} files={files} />;
  const themeFields = (
    <ThemeFields subject={subject} details={details} themeDraft={themeDraft} frame={frame} />
  );
  const avatarPicker = <EditAvatarPicker subject={subject} avatars={avatars} />;
  const unsavedChanges = (
    <UnsavedChanges
      frame={frame}
      themeDraft={themeDraft}
      subject={subject}
      editSave={editSave}
      closeGuard={closeGuard}
    />
  );
  return (
    <>
      <div
        className="modal-backdrop"
        onMouseDown={(e) => {
          if (e.target === e.currentTarget) requestClose();
        }}
      >
        <div
          className="modal editor-modal"
          role="dialog"
          aria-modal={!confirmClose && !avatarPickerOpen && !licenseOpen}
          aria-hidden={confirmClose || avatarPickerOpen || licenseOpen || undefined}
          inert={confirmClose || avatarPickerOpen || licenseOpen}
          aria-busy={busy}
          aria-labelledby="edit-dialog-title"
          ref={box}
          style={themeDialogStyle}
        >
          <div className="modal-header">
            <h2 id="edit-dialog-title">
              {L(labelKey)}
              {batch && ` · ${scoped.length}`}
            </h2>
            <button disabled={busy} aria-label={L('close')} onClick={requestClose}>
              ×
            </button>
          </div>
          <form
            onSubmit={(e) => {
              e.preventDefault();
              save();
            }}
            style={{ display: 'contents' }}
          >
            <div className="modal-body" inert={busy}>
              {(p.dialog === 'edit' || batch) && (
                <>
                  {listingFields}
                  {categoryFields}
                  {tagFields}
                  {batchFields}
                  {!batch && (
                    <>
                      {thumbnailFields}
                      {localFilesEditable && localFilesField}
                    </>
                  )}
                </>
              )}
              {p.dialog === 'saveSearch' && (
                <Field label={L('name')}>
                  <input value={name} required onChange={(e) => setName(e.target.value)} />
                </Field>
              )}
              {p.dialog === 'theme' && themeFields}
              {error && (
                <div className="callout error" role="alert">
                  {[
                    'nameRequired',
                    'invalidURL',
                    'invalidTheme',
                    'invalidThumbnail',
                    'themeNameRequired',
                  ].includes(error)
                    ? L(error)
                    : operationMessage(p.locale, error)}
                </div>
              )}
            </div>
            <div className="modal-footer">
              <button type="button" disabled={busy} onClick={close}>
                {L('cancel')}
              </button>
              <button
                className="primary"
                type="submit"
                disabled={
                  busy ||
                  (batch && (!scoped.length || !dirty)) ||
                  (p.dialog === 'theme' && !themeValid) ||
                  (p.dialog === 'edit' && !active)
                }
              >
                {L('save')}
              </button>
            </div>
          </form>
        </div>
      </div>
      {avatarPickerOpen && avatarPicker}
      {licenseOpen && active && (
        <LicenseEditor
          locale={p.locale}
          productId={active.id}
          draft={licenseDraft}
          onDone={(cells) => {
            setLicenseDraft(cells);
            setLicenseOpen(false);
          }}
          onClose={() => setLicenseOpen(false)}
        />
      )}
      {confirmClose && unsavedChanges}
    </>
  );
}
export function EditDialogs(props: EditDialogsProps) {
  if (props.dialog === 'collection') return <CollectionDialog {...props} />;
  if (
    props.dialog === 'manageCollections' ||
    props.dialog === 'manageCategories' ||
    props.dialog === 'manageSearches'
  )
    return <ManagerDialog key={props.dialog} {...props} />;
  if (!props.dialog || !['edit', 'batch', 'saveSearch', 'theme'].includes(props.dialog))
    return null;
  return <DialogBody key={props.dialog} {...props} />;
}
export default EditDialogs;
