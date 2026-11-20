import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react';
import type { Product, Locale, Account } from './model';
import { nativeWorkflowText } from './native-workflow-i18n';
import type { NativeState, LibraryStatus, LibraryCredential } from './backend';
import './workflow-panels.css';
import type { ThemeOption } from './ThemePicker';
import AiSettings from './AiSettings';
import LocalImport from './LocalImport';
import { resetErrorText } from './settings-reset-i18n';
import UnityWorkflow, { UnityResults, type UnitySeed } from './UnityWorkflow';
import { unityText } from './unity-i18n';
import { Panel, Field, displayWindowsPath, Status } from './features/workflows/parts';
import { Accounts, DeviceCredentials } from './features/workflows/Accounts';
import { Setup } from './features/workflows/Setup';
import { Sync } from './features/workflows/Sync';
import { Downloads } from './features/workflows/Downloads';
import { Storage } from './features/workflows/Storage';
import { AI } from './features/workflows/AI';

export type WorkflowAccount = Account;
export type ImportSeed = {
  name?: string;
  files?: string[];
  sources?: string[];
  source?: string;
  sourceKind?: 'local' | 'remote';
};
export type Props = {
  flow: string | null;
  unitySeed?: UnitySeed;
  onUnitySubmitted?: (jobId: string) => void;
  locale: Locale;
  products: Product[];
  selected: string[];
  onClose: () => void;
  onPatch?: (ids: string[], patch: Partial<Product>) => void;
  onNotice: (message: string) => void;
  onPolicyChange?: (all: boolean) => void;
  onAdd?: (products: Product[]) => void;
  autoDownloadAll?: boolean;
  onUserPatch?: (ids: string[], patch: Partial<Product>) => void;
  persistSynthetic?: boolean;
  onBatchUserPatch?: (changes: { id: string; patch: Partial<Product> }[]) => void;
  sensitiveMode?: string;
  accounts?: WorkflowAccount[];
  onAccountsChange?: (accounts: WorkflowAccount[]) => void;
  importSeed?: ImportSeed;
  onNavigate?: (flow: string) => void;
  onNewAssets?: (ids: string[], category?: string, jobId?: string) => void;
  onLocaleChange?: (locale: Locale) => void;
  themeOptions?: ThemeOption[];
  themeId?: string;
  onThemeChange?: (id: string) => void;
  libraryReady?: boolean;
  setupReady?: boolean;
  defaultLibraryPath?: string;
  defaultLibraryError?: string;
  onLibraryReady?: (location: string, rootId?: string, newLibrary?: boolean) => Promise<void>;
  missingCredentials?: LibraryCredential[];
  nativeState?: NativeState;
  catalogueVersion?: number;
  onNative?: (
    service: 'catalogue' | 'booth' | 'storage' | 'unity' | 'dialog',
    action: string,
    args?: Record<string, unknown>,
  ) => Promise<any>;
  onPreferences?: (patch: Record<string, unknown>) => Promise<void>;
};
const aliases: Record<string, string> = {
  downloads: 'download',
  backup: 'storage',
  devices: 'storage',
  history: 'import',
  updates: 'sync',
  diagnostics: 'sync',
};
function Content(
  props: Props & {
    onImportDirtyChange?: (dirty: boolean) => void;
    onBridgeVersionChange?: (version: string | null) => void;
  },
) {
  if (!props.flow) return null;
  const raw = props.flow.split(':')[0];
  const base = aliases[raw] ?? raw;
  const L = (key: string) => nativeWorkflowText(props.locale, key);
  const c = { ...props, L };
  const bodies: Record<string, ReactNode> = {
    setup: <Setup {...c} />,
    accounts: <Accounts {...c} />,
    signInOnDevice: <DeviceCredentials {...c} />,
    sync: <Sync {...c} />,
    download: <Downloads {...c} />,
    storage: <Storage {...c} />,
    import: <LocalImport {...c} onDirtyChange={props.onImportDirtyChange} />,
    ai: <AI {...c} />,
    'ai-settings': <AiSettings locale={c.locale} nativeState={c.nativeState} />,
    unity: (
      <UnityWorkflow
        locale={c.locale}
        products={c.products}
        seed={c.unitySeed ?? { productIds: c.selected }}
        onBridgeVersionChange={props.onBridgeVersionChange}
        onClose={c.onClose}
        onSubmitted={(id) => (c.onUnitySubmitted ? c.onUnitySubmitted(id) : c.onClose())}
      />
    ),
    'unity-results': (
      <UnityResults locale={c.locale} jobId={props.flow.split(':')[1] ?? ''} onClose={c.onClose} />
    ),
  };
  return (
    <div className={'workflow-content wf-' + base} key={base}>
      {bodies[base] ?? <Status value="unknown" L={L} />}
    </div>
  );
}
export function WorkflowContent(props: Props) {
  return <Content {...props} />;
}
function trapTab(e: React.KeyboardEvent, element: HTMLElement | null) {
  if (e.key !== 'Tab') return;
  const items = Array.from(
    element?.querySelectorAll<HTMLElement>(
      'button:not(:disabled),input:not(:disabled),select:not(:disabled),textarea:not(:disabled),[tabindex="0"]',
    ) ?? [],
  ).filter((x) => x.getClientRects().length);
  const first = items[0],
    last = items[items.length - 1];
  if (e.shiftKey && document.activeElement === first) {
    e.preventDefault();
    last?.focus();
  } else if (!e.shiftKey && document.activeElement === last) {
    e.preventDefault();
    first?.focus();
  }
}
export function LibraryAccess({
  locale,
  status,
  busy,
  onChoose,
  onRetry,
}: {
  locale: Locale;
  status: LibraryStatus | null;
  busy: boolean;
  onChoose: () => void;
  onRetry: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null),
    L = (key: string) => resetErrorText(locale, key) ?? nativeWorkflowText(locale, key);
  const waiting = busy || !status || status.errorCode === 'reset_in_progress';
  useEffect(() => {
    const previous = document.activeElement as HTMLElement;
    if (waiting) ref.current?.focus();
    else ref.current?.querySelector<HTMLElement>('button:not(:disabled)')?.focus();
    return () => {
      if (waiting && previous?.isConnected) previous.focus();
    };
  }, [waiting, status?.errorCode]);
  if (waiting)
    return (
      <div
        ref={ref}
        className="library-opening"
        role="status"
        tabIndex={-1}
        onKeyDown={(e) => {
          if (e.key === 'Tab') {
            e.preventDefault();
            e.stopPropagation();
          }
        }}
      >
        <progress />
        <span>
          {L(status?.errorCode === 'reset_in_progress' ? 'resetInProgress' : 'openingLibrary')}
        </span>
      </div>
    );
  const error = status.errorCode ?? 'LIBRARY_UNAVAILABLE',
    libraryError = /^library_|^LIBRARY_|^reset_/.test(error) ? error : 'libraryCannotOpen';
  return (
    <div className="modal-backdrop">
      <div
        ref={ref}
        className="modal workflow-modal library-access-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="library-access-title"
        onKeyDown={(e) => {
          if (e.key === 'Escape') {
            e.preventDefault();
            e.stopPropagation();
          }
          trapTab(e, ref.current);
        }}
      >
        <header className="modal-header">
          <h2 id="library-access-title">
            {L(error.toLowerCase() === 'library_in_use' ? 'libraryInUse' : 'libraryUnavailable')}
          </h2>
        </header>
        <div className="workflow-content">
          <Panel
            actions={
              <>
                <button disabled={error === 'reset_incomplete'} onClick={onChoose}>
                  {L('chooseOtherLocation')}
                </button>
                <button className="primary" disabled={!status.path} onClick={onRetry}>
                  {L('retry')}
                </button>
              </>
            }
          >
            <Field label={L('libraryLocation')}>
              <input
                readOnly
                value={displayWindowsPath(status.path ?? '')}
                title={displayWindowsPath(status.path ?? '')}
              />
            </Field>
            <Status value={libraryError} L={L} error />
          </Panel>
        </div>
      </div>
    </div>
  );
}
export default function Workflows(props: Props) {
  const dialog = useRef<HTMLDivElement>(null);
  const dirty = useRef(false);
  const [confirmClose, setConfirmClose] = useState(false);
  const [bridgeVersion, setBridgeVersion] = useState<string | null>(null);
  const { flow, locale, onClose } = props;
  const setupRequired =
    flow?.split(':')[0] === 'setup' && props.nativeState?.preferences.setupComplete !== true;
  const requestClose = () => {
    if (setupRequired) return;
    if (dirty.current) setConfirmClose(true);
    else onClose();
  };
  const onImportDirtyChange = useCallback((value: boolean) => {
    dirty.current = value;
  }, []);
  useEffect(() => {
    if (!flow) {
      dirty.current = false;
      setConfirmClose(false);
    }
  }, [flow]);
  useEffect(() => {
    if (!flow) return;
    const previous = document.activeElement as HTMLElement;
    dialog.current
      ?.querySelector<HTMLElement>(
        'input:not(:disabled),select:not(:disabled),button:not(:disabled)',
      )
      ?.focus();
    return () => previous?.focus();
  }, [flow]);
  useEffect(() => {
    if (flow && ['download', 'downloads'].includes(flow.split(':')[0])) {
      onClose();
      props.onNavigate?.('download');
    }
  }, [flow]);
  if (!flow || ['download', 'downloads'].includes(flow.split(':')[0])) return null;
  const raw = flow.split(':')[0];
  const base = aliases[raw] ?? raw;
  const L = (key: string) => nativeWorkflowText(locale, key);
  return (
    <div
      className="modal-backdrop"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) requestClose();
      }}
    >
      <div
        ref={dialog}
        className={'modal workflow-modal' + (base === 'unity' ? ' unity-picker-modal' : '')}
        role="dialog"
        aria-modal="true"
        aria-labelledby="workflow-title"
        onKeyDown={(e) => {
          if (e.key === 'Escape' && !e.nativeEvent.isComposing && e.keyCode !== 229) {
            e.stopPropagation();
            if (confirmClose) setConfirmClose(false);
            else requestClose();
          }
          trapTab(e, dialog.current);
        }}
      >
        <header className="modal-header">
          <h2 id="workflow-title">
            {base === 'unity-results' ? unityText(locale, 'results') : L(base)}
          </h2>
          {base === 'unity' ? (
            <span className="unity-bridge-version">
              {bridgeVersion ? 'Bridge v' + bridgeVersion : 'Bridge —'}
            </span>
          ) : (
            !setupRequired && (
              <button aria-label={L('close')} onClick={requestClose}>
                ×
              </button>
            )
          )}
        </header>
        {confirmClose ? (
          <div className="workflow-content">
            <Panel
              actions={
                <>
                  <button autoFocus onClick={() => setConfirmClose(false)}>
                    {L('stay')}
                  </button>
                  <button
                    onClick={() => {
                      dirty.current = false;
                      onClose();
                    }}
                  >
                    {L('discard')}
                  </button>
                </>
              }
            >
              <Status value="dirty" L={L} />
            </Panel>
          </div>
        ) : null}
        <div className="workflow-body" hidden={confirmClose}>
          <Content
            {...props}
            onClose={() => {
              dirty.current ? requestClose() : onClose();
            }}
            onImportDirtyChange={onImportDirtyChange}
            onBridgeVersionChange={setBridgeVersion}
          />
        </div>
      </div>
    </div>
  );
}
