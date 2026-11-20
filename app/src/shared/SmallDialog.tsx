import { useEffect, useRef } from 'react';
import type { ReactNode } from 'react';
import { X } from 'lucide-react';
import Button from './Button';

/** A modal with a title, a close button and focus kept inside until it closes. */
export default function SmallDialog({
  label,
  close,
  q,
  children,
  className = '',
}: {
  label: string;
  close: () => void;
  q: (key: string) => string;
  children: ReactNode;
  className?: string;
}) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const previous = document.activeElement as HTMLElement;
    (
      ref.current?.querySelector<HTMLElement>('input,select') ??
      ref.current?.querySelector<HTMLElement>('button')
    )?.focus();
    return () => {
      if (previous?.isConnected) previous.focus();
    };
  }, []);
  return (
    <div
      className="modal-backdrop"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) close();
      }}
    >
      <div
        className={`modal shell-dialog ${className}`}
        role="dialog"
        aria-modal="true"
        aria-label={label}
        ref={ref}
        onKeyDown={(e) => {
          if (e.nativeEvent.isComposing) return;
          if (e.key === 'Escape') {
            e.stopPropagation();
            close();
          }
          if (e.key === 'Tab') {
            const nodes = Array.from(
              ref.current?.querySelectorAll<HTMLElement>(
                'button:not(:disabled),input:not(:disabled),select:not(:disabled),summary,[tabindex="0"]',
              ) ?? [],
            ).filter((node) => node.getClientRects().length);
            if (e.shiftKey && document.activeElement === nodes[0]) {
              e.preventDefault();
              nodes.at(-1)?.focus();
            } else if (!e.shiftKey && document.activeElement === nodes.at(-1)) {
              e.preventDefault();
              nodes[0]?.focus();
            }
          }
        }}
      >
        <header className="modal-header">
          <h2>{label}</h2>
          <Button title={q('close')} onClick={close}>
            <X size={14} />
          </Button>
        </header>
        <div className="modal-body">{children}</div>
        <footer className="modal-footer">
          <Button onClick={close}>{q('close')}</Button>
        </footer>
      </div>
    </div>
  );
}
