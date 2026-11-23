import { invoke, convertFileSrc, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { Product, Locale } from './model';
import type {
  Bootstrap,
  CatalogueRequest,
  CatalogueResponses,
  DialogKind,
  DialogOptions,
  DialogPick,
  FileRequest,
  FileResponses,
  Job,
  LicensesRequest,
  LibraryOpened,
  LibraryStatus,
  LicensesResponses,
  LocalFiles,
  NativeChange,
  OverlayChange,
  Preferences,
  ProductMetadata,
  StorageRequest,
  StorageResponses,
  StoreRequest,
  StoreResponses,
  UnityRequest,
  UnityResponses,
  UpdateCheck,
  UpdateProgress,
  Zoomed,
} from './bindings';
export type {
  AvatarDelta,
  AvatarOption,
  AvatarSetting,
  BridgeRelease,
  Collection,
  Job,
  LibraryCredential,
  LibraryStatus,
  Preferences,
  ProductMetadata,
  Root,
  TagOption,
  UnityDetails,
  UnityEditor,
  UnityLeaf,
  UpdateCheck,
  UpdateProgress,
} from './bindings';

export type Service = 'catalogue' | 'booth' | 'storage' | 'unity' | 'licenses';
/** Each service's actions, generated from its Rust request enum. */
type Requests = {
  catalogue: CatalogueRequest;
  booth: StoreRequest;
  storage: StorageRequest;
  unity: UnityRequest;
  licenses: LicensesRequest;
};
/** What each typed action returns; the others return `any`. */
type Responses = {
  catalogue: CatalogueResponses;
  booth: StoreResponses;
  storage: StorageResponses;
  unity: UnityResponses;
  licenses: LicensesResponses;
};
export type Action<S extends Service> = Requests[S]['action'];
type ActionArgs<S extends Service, A extends string> = Omit<
  Extract<Requests[S], { action: A }>,
  'action'
>;
/** Arguments may be left out when every one of them is optional. */
export type ArgsParam<S extends Service, A extends string> =
  Partial<ActionArgs<S, A>> extends ActionArgs<S, A>
    ? [args?: ActionArgs<S, A>]
    : [args: ActionArgs<S, A>];
export type ActionResult<S extends Service, A extends string> = A extends keyof Responses[S]
  ? Responses[S][A]
  : any;
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
function importOnlyUnityJob(job: Job): Job {
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
/** How a Unity request's transfer is going, on its job or its receipt. */
export type UnityPreparation = Pick<
  Job,
  | 'preparationPhase'
  | 'preparationWaitingForEditor'
  | 'preparationBytes'
  | 'preparationTotalBytes'
  | 'preparationEntries'
  | 'preparationTotalEntries'
>;
export function progressPercent(job: Job | undefined): number | undefined {
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
/** An overlay change with its patch typed as product fields. */
export type ProductOverlayChange = Omit<OverlayChange, 'patch'> & { patch: Partial<Product> };
/** Files added to and removed from a product's own copies. */
export type LocalFileChanges = Omit<LocalFiles, 'productId'>;
export function updateLocalFiles(productId: string, changes: LocalFileChanges) {
  return invokeAction('storage', 'local_files', { productId, ...changes });
}
export async function libraryStatus(): Promise<LibraryStatus> {
  if (!isTauri()) throw new Error('nativeUnavailable');
  return invoke('library_status');
}
export async function openLibrary(
  path: string,
  locale?: Locale,
  newLibrary = false,
): Promise<LibraryOpened> {
  if (!isTauri()) throw new Error('nativeUnavailable');
  return invoke('library_open', { path, locale, newLibrary });
}
/** The parts of `bootstrap` the shell may not have loaded yet. */
type LaterParts =
  | 'customCategories'
  | 'avatarOptions'
  | 'ownedBaseOptions'
  | 'tagOptions'
  | 'total'
  | 'counts';
/** The open library as the shell holds it: `bootstrap`, refreshed by `state` and list queries.
 * Preferences missing from the library read as none. */
export type NativeState = Omit<Bootstrap, LaterParts | 'products' | 'preferences'> &
  Partial<Pick<Bootstrap, LaterParts>> & {
    products: Product[];
    preferences: Preferences;
  };
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
};
/** What a file dialog picked, with the first path as `path`. */
interface DialogResult {
  path: string;
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
  kind: DialogKind,
  options: DialogOptions = {},
): Promise<DialogResult | null> {
  if (!isTauri()) throw new Error('nativeUnavailable');
  const result = await invoke<DialogPick | null>('native_dialog', { kind, options });
  return result === null
    ? null
    : typeof result === 'string'
      ? { path: result }
      : { path: result[0] ?? '', paths: result };
}
export async function subscribeChanged(
  reload: (change: NativeChange) => void,
): Promise<() => void> {
  return isTauri()
    ? listen<NativeChange>('nna:changed', (event) => reload(event.payload))
    : () => {};
}
type FileAction = FileRequest['action'];
export async function nativeFileAction<A extends FileAction>(
  action: A,
  args: Omit<Extract<FileRequest, { action: A }>, 'action'>,
): Promise<FileResponses[A]> {
  if (!isTauri()) throw new Error('nativeUnavailable');
  return invoke<FileResponses[A]>('native_file_action', { action, args });
}
export async function nativeZoom(scale: number): Promise<Zoomed> {
  if (!isTauri()) throw new Error('nativeUnavailable');
  return invoke('native_zoom', { scale });
}
export async function openExternal(url: string): Promise<void> {
  if (!isTauri()) throw new Error('nativeUnavailable');
  return invoke('external_open', { url });
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
export function fetchProductMetadata(url: string): Promise<ProductMetadata> {
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

export const listUnityEditors = () => invokeAction('unity', 'list_editors');
export const unityInventory = (productIds: string[]) =>
  invokeAction('unity', 'inventory', { productIds });
/** One package opens in Unity's own importer and leaves no job; anything else is a Bridge request. */
export const submitUnity = (input: {
  productIds: string[];
  leafRefs: string[];
  instanceId: string;
  sessionId: string;
  locale?: string;
}) => invokeAction('unity', 'submit', input);
export const unityDetails = (jobId: string) => invokeAction('unity', 'details', { jobId });
export const queryUnity = (jobId: string) => invokeAction('unity', 'status', { jobId });
export const cancelUnity = (jobId: string) => invokeAction('unity', 'cancel', { jobId });
export const retryUnityInventory = (jobId: string) =>
  invokeAction('unity', 'retry_inventory', { jobId });
