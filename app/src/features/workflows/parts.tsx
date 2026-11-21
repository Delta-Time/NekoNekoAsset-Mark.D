import type { Props } from '../../Workflows';
import { type ReactNode, useRef, useState } from 'react';
import { errorCode } from '../../backend';
import type { Action, ActionResult, ArgsParam, LooseNativeCall, Service } from '../../backend';

export type Ctx = Props & { L: (key: string) => string };
export function Panel({ children, actions }: { children: ReactNode; actions?: ReactNode }) {
  return (
    <>
      <div className="wf-scroll">{children}</div>
      {actions && <footer className="wf-actions">{actions}</footer>}
    </>
  );
}
export function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="field">
      <span>{label}</span>
      {children}
    </label>
  );
}
export function Row({ label, children }: { label: ReactNode; children: ReactNode }) {
  return (
    <div className="data-row">
      <span>{label}</span>
      <span>{children}</span>
    </div>
  );
}
export function Buttons({ children }: { children: ReactNode }) {
  return <div className="form-row">{children}</div>;
}
export function Status({
  value,
  L,
  error = false,
}: {
  value: string;
  L: Ctx['L'];
  error?: boolean;
}) {
  return value ? (
    <div className={error ? 'wf-error' : 'status-line'} role={error ? 'alert' : 'status'}>
      {L(value)}
    </div>
  ) : null;
}
/** Presentation only: callers retain the original native path for every command. */
export function displayWindowsPath(path: string): string {
  const prefix = '\\\\?\\';
  if (path.slice(0, 8).toUpperCase() === prefix + 'UNC\\') return '\\\\' + path.slice(8);
  if (path.startsWith(prefix) && /^[A-Za-z]:[\\/]/.test(path.slice(4))) return path.slice(4);
  return path;
}
export const validPath = (s: string) => {
  const display = displayWindowsPath(s.trim());
  return (
    /^(?:[A-Za-z]:[\\/]|\\\\[^\\]+\\[^\\]+|\/)/.test(display) &&
    display.length > 3 &&
    !/[?*<>|]/.test(display)
  );
};
export function native<S extends Service, A extends Action<S>>(
  c: Ctx,
  service: S,
  action: A,
  ...args: ArgsParam<S, A>
): Promise<ActionResult<S, A>>;
export function native(
  c: Ctx,
  service: 'dialog',
  action: 'folder' | 'open_file' | 'save_file',
  options?: Record<string, unknown>,
): Promise<any>;
export async function native(
  c: Ctx,
  service: Service | 'dialog',
  action: string,
  args: Record<string, unknown> = {},
) {
  if (!c.onNative) throw new Error('nativeUnavailable');
  return (c.onNative as LooseNativeCall)(service, action, args);
}
export async function savePreferences(c: Ctx, patch: Record<string, unknown>) {
  if (c.onPreferences) return c.onPreferences(patch);
  await native(c, 'catalogue', 'preferences', { patch });
}
export async function choose(
  c: Ctx,
  kind: 'folder' | 'open_file' | 'save_file',
  options: Record<string, unknown> = {},
) {
  const result = await native(c, 'dialog', kind, options);
  return typeof result === 'string' ? result : (result?.path as string | undefined);
}
export function useAction() {
  const lock = useRef(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const run = async (action: () => Promise<void>) => {
    if (lock.current) return;
    lock.current = true;
    setBusy(true);
    setError('');
    try {
      await action();
    } catch (e) {
      setError(errorCode(e));
    } finally {
      lock.current = false;
      setBusy(false);
    }
  };
  return { busy, error, run };
}
export type ActionTask = ReturnType<typeof useAction>;
