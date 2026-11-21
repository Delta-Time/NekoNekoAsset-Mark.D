import { invoke, convertFileSrc, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { Product, Account, Locale, CustomCategory } from './model';
import type { CatalogueRequest, CatalogueResponses, Conflict } from './bindings';

export type Service = 'catalogue' | 'booth' | 'storage' | 'unity' | 'licenses';
/** A service whose actions are not typed yet takes any action and arguments. */
type Untyped = { untyped: true };
/** Each service's actions, generated from its Rust request enum. */
type Requests = {
  catalogue: CatalogueRequest;
  booth: Untyped;
  storage: Untyped;
  unity: Untyped;
  licenses: Untyped;
};
/** What each typed action returns; the others return `any`. */
type Responses = {
  catalogue: CatalogueResponses;
  booth: Record<never, never>;
  storage: Record<never, never>;
  unity: Record<never, never>;
  licenses: Record<never, never>;
};
type Typed<S extends Service> = Extract<Requests[S], { action: string }>;
export type Action<S extends Service> = Requests[S] extends Untyped ? string : Typed<S>['action'];
export type ActionArgs<S extends Service, A extends string> = Requests[S] extends Untyped
  ? Record<string, unknown>
  : Omit<Extract<Typed<S>, { action: A }>, 'action'>;
/** Arguments may be left out when every one of them is optional. */
export type ArgsParam<S extends Service, A extends string> =
  Partial<ActionArgs<S, A>> extends ActionArgs<S, A>
    ? [args?: ActionArgs<S, A>]
    : [args: ActionArgs<S, A>];
export type ActionResult<S extends Service, A extends string> = A extends keyof Responses[S]
  ? Responses[S][A]
  : any;
type DialogKind = 'folder' | 'open_file' | 'save_file';
/** A native action, or a native file dialog through the `dialog` service. */
export type NativeCall = {
  <S extends Service, A extends Action<S>>(
    service: S,
    action: A,
    ...args: ArgsParam<S, A>
  ): Promise<ActionResult<S, A>>;
  (service: 'dialog', action: DialogKind, options?: Record<string, unknown>): Promise<any>;
};
/** The untyped form of `NativeCall`, for wrappers that declare its overloads themselves. */
export type LooseNativeCall = (
  service: Service | 'dialog',
  action: string,
  args?: Record<string, unknown>,
) => Promise<any>;
export interface SourceProgress {
  stage?: string;
  source?: string;
  pages?: number;
  detailsCompleted?: number;
  requestsCompleted?: number;
  products?: number;
  published?: number;
  metadataCompleted?: number;
  metadataTotal?: number;
  availabilityWarnings?: number;
  incompleteProducts?: number;
}
export interface NativeJob extends UnityPreparation {
  id: string;
  type: string;
  productId?: string;
  productIds?: string[];
  completedProductIds?: string[];
  fileIds?: string[];
  selectedFileIds?: string[] | null;
  accountId?: string;
  accountName?: string;
  state: string;
  progress?: number | SourceProgress;
  errorCode?: string;
  cancelRequested?: boolean;
  createdAt?: string;
  updatedAt?: string;
  completedAt?: string;
  projectName?: string;
  unityVersion?: string;
  stage?: string;
  writesStarted?: boolean;
  resultCounts?: { applied: number; skipped: number; failed: number };
}
export function importOnlyUnityJob(job: NativeJob): NativeJob {
  if (job.type !== 'unity:import') return job;
  const projected = { ...job };
  Reflect.deleteProperty(projected, 'compileErrorCount');
  if (projected.stage === 'compiling') projected.stage = 'refreshing';
  if (['unity_compile_errors', 'UNITY_COMPILE_ERRORS'].includes(projected.errorCode ?? '')) {
    delete projected.errorCode;
    if (projected.stage === 'completed' && projected.resultCounts?.failed === 0)
      projected.state = 'completed';
    else if ((projected.resultCounts?.failed ?? 0) > 0)
      projected.errorCode = 'unity_partial_failure';
    else if (projected.stage === 'interrupted') projected.errorCode = 'unity_interrupted';
    else if (projected.stage === 'failed') projected.errorCode = 'unity_import_failed';
  }
  return projected;
}
export interface UnityPreparation {
  preparationPhase?: string | null;
  preparationWaitingForEditor?: boolean;
  preparationBytes?: number | null;
  preparationTotalBytes?: number | null;
  preparationEntries?: number | null;
  preparationTotalEntries?: number | null;
}
export function progressPercent(
  job: (NativeJob & UnityPreparation) | undefined,
): number | undefined {
  if (
    job?.type === 'unity:import' &&
    ['waitingForUser', 'importing', 'refreshing', 'compiling', 'completed'].includes(
      job.stage ?? '',
    )
  )
    return 100;
  if (typeof job?.progress === 'number') return job.progress;
  if (job?.stage !== 'preparing') return undefined;
  if (!job.preparationPhase || job.preparationWaitingForEditor) return undefined;
  const [done, total] =
    job.preparationPhase === 'checkTargets'
      ? [job.preparationEntries, job.preparationTotalEntries]
      : [job.preparationBytes, job.preparationTotalBytes];
  return typeof done === 'number' && typeof total === 'number' && total > 0
    ? Math.min(100, (100 * done) / total)
    : undefined;
}
export interface NativeRoot {
  id: string;
  path: string;
  label?: string;
  status: string;
}
export interface AvatarOption {
  id: string;
  productId?: string;
  displayName?: string;
  sourceDisplayName?: string;
  sourceIdentityValue?: string;
  officialName: string;
  englishName?: string;
  labelStatus?: string;
  nameKind?: 'single' | 'bundle' | 'fallback';
  labelQualifiers?: string[];
  shopName?: string;
  sourceShopName?: string;
  shopId?: string;
  aliases: string[];
  matchingProductCount?: number;
  baseProductIds?: string[];
  enabled?: boolean;
  explicitEnabled?: boolean;
}
export interface AvatarSetting {
  enabled?: boolean;
  originalName?: string;
  englishName?: string;
  displayName?: string;
  shopName?: string;
  aliases?: string[];
}
export interface AvatarDelta {
  add: string[];
  remove: string[];
}
export interface ProductOverlayChange {
  id: string;
  patch: Partial<Product>;
  ownedAvatarsDelta?: AvatarDelta;
}
export interface TagOption {
  id: string;
  label: string;
  kind: string;
  aliases: string[];
}
export interface FetchedProductMetadata {
  sourceUrl: string;
  title: string;
  category: Product['category'];
  tags: string[];
  thumbnailUrl: string | null;
}
export interface LocalFileChanges {
  draftId: string;
  addSources: string[];
  removeFileIds: string[];
}
export async function updateLocalFiles(
  productId: string,
  changes: LocalFileChanges,
): Promise<{ jobId: string; productIds: string[]; state: string }> {
  return invokeAction('storage', 'local_files', { productId, ...changes });
}
export interface NativeCollection {
  id: string;
  name: string;
  productIds: string[];
}
export interface LibraryCredential {
  id: string;
  name: string;
  store: string;
}
export interface LibraryStatus {
  state: 'ready' | 'setup' | 'unconfigured' | 'blocked';
  path?: string;
  defaultPath?: string;
  errorCode?: string;
  locale?: string;
  missingCredentials?: LibraryCredential[];
}
export async function libraryStatus(): Promise<LibraryStatus> {
  if (!isTauri()) throw new Error('nativeUnavailable');
  return invoke('library_status');
}
export async function openLibrary(
  path: string,
  locale?: Locale,
  newLibrary = false,
): Promise<{ restarting: boolean }> {
  if (!isTauri()) throw new Error('nativeUnavailable');
  return invoke('library_open', { path, locale, newLibrary });
}
export interface NativeState {
  version: number;
  products: Product[];
  accounts: Account[];
  roots: NativeRoot[];
  jobs: NativeJob[];
  preferences: Record<string, unknown>;
  collections: Record<string, unknown>[];
  savedSearches: Record<string, unknown>[];
  customCategories?: CustomCategory[];
  sourceCoverage: Record<string, unknown>[];
  conflicts: Conflict[];
  avatarOptions?: AvatarOption[];
  ownedBaseOptions?: AvatarOption[];
  tagOptions?: TagOption[];
  total?: number;
  counts?: { all: number; downloaded: number; attention: number; hidden?: number };
}
export const emptyState: NativeState = {
  version: 1,
  products: [],
  accounts: [],
  roots: [],
  jobs: [],
  preferences: {},
  collections: [],
  savedSearches: [],
  sourceCoverage: [],
  conflicts: [],
};
export interface DialogResult {
  path: string;
  content?: string;
  paths?: string[];
}
export async function invokeAction<S extends Service, A extends Action<S>>(
  service: S,
  action: A,
  ...rest: ArgsParam<S, A>
): Promise<ActionResult<S, A>> {
  if (!isTauri()) throw new Error('nativeUnavailable');
  const args = rest[0] ?? {};
  const value = await invoke<ActionResult<S, A>>(`${service}_action`, { action, args });
  if (
    service === 'catalogue' &&
    ['bootstrap', 'state'].includes(action) &&
    value &&
    typeof value === 'object' &&
    'jobs' in value &&
    Array.isArray(value.jobs)
  )
    value.jobs = value.jobs.map(importOnlyUnityJob);
  return value;
}
export async function dialog(
  kind: 'folder' | 'open_file' | 'save_file',
  options: Record<string, unknown> = {},
): Promise<DialogResult | null> {
  if (!isTauri()) throw new Error('nativeUnavailable');
  const result = await invoke<DialogResult | string | string[] | null>('native_dialog', {
    kind,
    options,
  });
  return typeof result === 'string'
    ? { path: result }
    : Array.isArray(result)
      ? { path: result[0] ?? '', paths: result }
      : result;
}
export interface NativeChange {
  scope?: string;
  productIds?: string[];
}
export async function subscribeChanged(
  reload: (change: NativeChange) => void,
): Promise<() => void> {
  return isTauri()
    ? listen<{ scope?: unknown; productIds?: unknown }>('nna:changed', (event) =>
        reload({
          scope: typeof event.payload?.scope === 'string' ? event.payload.scope : undefined,
          productIds: Array.isArray(event.payload?.productIds)
            ? event.payload.productIds.filter((id): id is string => typeof id === 'string')
            : undefined,
        }),
      )
    : () => {};
}
export async function nativeFileAction<T = any>(
  action: string,
  args: Record<string, unknown>,
): Promise<T> {
  if (!isTauri()) throw new Error('nativeUnavailable');
  return invoke<T>('native_file_action', { action, args });
}
export async function nativeZoom(scale: number): Promise<{ scale: number }> {
  if (!isTauri()) throw new Error('nativeUnavailable');
  return invoke('native_zoom', { scale });
}
export async function openExternal(url: string): Promise<void> {
  if (!isTauri()) throw new Error('nativeUnavailable');
  return invoke('external_open', { url });
}
export interface AppRelease {
  version: string;
  notes: string;
  releaseUrl: string;
}
export interface BridgeRelease {
  version: string;
  unitypackageUrl: string;
  releaseUrl: string;
}
export interface UpdateCheck {
  app: {
    currentVersion: string;
    installSupported: boolean;
    available?: AppRelease;
    errorCode?: string;
  };
  bridge?: BridgeRelease;
  bridgeError?: string;
}
export interface UpdateProgress {
  stage: 'downloading' | 'verifying' | 'installing';
  downloadedBytes: number;
  totalBytes?: number | null;
}
export async function checkUpdates(): Promise<UpdateCheck> {
  if (!isTauri()) throw new Error('nativeUnavailable');
  return invoke('updates_check');
}
export async function installUpdate(version: string): Promise<void> {
  if (!isTauri()) throw new Error('nativeUnavailable');
  return invoke('updates_install', { version });
}
export async function subscribeUpdateProgress(
  callback: (progress: UpdateProgress) => void,
): Promise<() => void> {
  return isTauri()
    ? listen<UpdateProgress>('updates:progress', (event) => callback(event.payload))
    : () => {};
}
export async function fetchProductMetadata(url: string): Promise<FetchedProductMetadata> {
  return invokeAction('booth', 'fetch_product_metadata', { url });
}
export function previewSource(path: string | null | undefined): string | undefined {
  if (!path) return undefined;
  if (/^(https:\/\/|asset:|https?:\/\/asset\.localhost\/)/.test(path)) return path;
  return isTauri() ? convertFileSrc(path) : undefined;
}
/** Only stable codes reach UI; private response text and URLs are never surfaced. */
export function errorCode(error: unknown): string {
  const value =
    typeof error === 'string'
      ? error
      : error instanceof Error
        ? error.message
        : error && typeof error === 'object' && 'code' in error && typeof error.code === 'string'
          ? error.code
          : '';
  return /^[A-Za-z][A-Za-z0-9_.-]{0,80}$/.test(value) ? value : 'operationFailed';
}

export interface UnityEditor {
  instanceId: string;
  sessionId: string;
  projectName: string;
  projectRoot: string;
  unityVersion: string;
  bridgeVersion?: string | null;
  busy: boolean;
}
export interface UnityLeaf {
  leafRef: string;
  productId: string;
  productTitle: string;
  fileId: string;
  sourceName: string;
  variantName: string;
  revision: string;
  memberPath: string;
  displayName: string;
  relativePath: string;
  kind: 'unitypackage' | 'file';
  bytes: number;
  sha256: string;
}
export interface UnityReceipt extends UnityPreparation {
  requestId: string;
  fingerprint: string;
  instanceId: string;
  sessionId: string;
  stage: string;
  staged: boolean;
  writesStarted: boolean;
  updatedUtc: string;
  error?: string | null;
  results: {
    entryId: string;
    path: string;
    state: 'applied' | 'skipped' | 'failed';
    message?: string | null;
  }[];
}
export interface UnityDetails {
  jobId: string;
  projectName: string;
  projectRoot: string;
  unityVersion: string;
  stage: string;
  errorCode?: string | null;
  receipt: UnityReceipt | null;
}
export const listUnityEditors = (): Promise<{ editors: UnityEditor[] }> =>
  invokeAction('unity', 'list_editors');
export const unityInventory = (productIds: string[]): Promise<{ files: UnityLeaf[] }> =>
  invokeAction('unity', 'inventory', { productIds });
/** One package opens in Unity's own importer and leaves no job; anything else is a Bridge request. */
export const submitUnity = (input: {
  productIds: string[];
  leafRefs: string[];
  instanceId: string;
  sessionId: string;
  locale?: string;
}): Promise<{ jobId?: string; native?: true }> => invokeAction('unity', 'submit', input);
export const unityDetails = (jobId: string): Promise<UnityDetails> =>
  invokeAction('unity', 'details', { jobId });
export const queryUnity = (jobId: string): Promise<UnityDetails> =>
  invokeAction('unity', 'status', { jobId });
export const cancelUnity = (jobId: string): Promise<UnityDetails> =>
  invokeAction('unity', 'cancel', { jobId });
export const retryUnityInventory = (
  jobId: string,
): Promise<{ productIds: string[]; leafRefs: string[] }> =>
  invokeAction('unity', 'retry_inventory', { jobId });
