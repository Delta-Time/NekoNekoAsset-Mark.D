import type { MouseEvent, ReactNode } from 'react';

export default function Button({
  children,
  onClick,
  onMouseDown,
  title,
  primary = false,
  disabled = false,
  className = '',
}: {
  children: ReactNode;
  onClick?: () => void;
  onMouseDown?: (event: MouseEvent<HTMLButtonElement>) => void;
  title?: string;
  primary?: boolean;
  disabled?: boolean;
  className?: string;
}) {
  return (
    <button
      type="button"
      title={title}
      aria-label={title}
      className={`button ${primary ? 'primary' : ''} ${className}`}
      onClick={onClick}
      onMouseDown={onMouseDown}
      disabled={disabled}
    >
      {children}
    </button>
  );
}
