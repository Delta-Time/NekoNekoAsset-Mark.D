import { useEffect, useState } from 'react';
import type { PointerEvent } from 'react';

/** Window width, the draggable navigation and details dividers, and the details pane's width. */
export function useLayout({
  navWidth,
  setNavWidth,
  inspectorWidth,
  setInspectorWidth,
}: {
  navWidth: number;
  setNavWidth: (value: number | ((old: number) => number)) => void;
  inspectorWidth: number;
  setInspectorWidth: (value: number | ((old: number) => number)) => void;
}) {
  const [viewportWidth, setViewportWidth] = useState(innerWidth);
  const paneWidth = Math.max(
      240,
      Math.min(inspectorWidth, Math.max(640, viewportWidth) - navWidth - 248),
    ),
    wide = paneWidth >= 520;
  const resize = (side: 'left' | 'right', e: PointerEvent) => {
    e.preventDefault();
    const x = e.clientX,
      start = side === 'left' ? navWidth : inspectorWidth;
    const move = (event: globalThis.PointerEvent) => {
      const value = start + (event.clientX - x) * (side === 'left' ? 1 : -1);
      if (side === 'left') setNavWidth(Math.max(132, Math.min(300, viewportWidth - 500, value)));
      else setInspectorWidth(Math.max(240, Math.min(900, viewportWidth - navWidth - 248, value)));
    };
    const up = () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
      document.body.classList.remove('resizing');
    };
    document.body.classList.add('resizing');
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up, { once: true });
  };
  useEffect(() => {
    const resize = () => setViewportWidth(innerWidth);
    window.addEventListener('resize', resize);
    return () => window.removeEventListener('resize', resize);
  }, []);
  return { paneWidth, wide, resize };
}
