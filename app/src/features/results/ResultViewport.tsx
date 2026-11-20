import { useLayoutEffect, useRef, useState } from 'react';
import type { CSSProperties, ReactNode } from 'react';
import { resultGeometry } from '../../model';
import type { Product } from '../../model';
import { useDragGestures } from '../../drag-gestures';
import type { DragGestures } from '../../drag-gestures';

/** The virtualized product grid or list. Only rows near the viewport are rendered; `state`
 * keeps the scroll position across remounts. */
export default function ResultViewport({
  items,
  size,
  density,
  state,
  activeId,
  metadataCount,
  render,
  gestures,
}: {
  items: Product[];
  size: number;
  density: number;
  state: { index: number; offset: number };
  activeId: string;
  metadataCount: number;
  render: (p: Product, list: boolean) => ReactNode;
  gestures: DragGestures | null;
}) {
  const ref = useRef<HTMLDivElement>(null),
    previousActive = useRef(activeId);
  const [width, setWidth] = useState(500),
    [height, setHeight] = useState(500),
    [top, setTop] = useState(0);
  const { list, gap, columns, actual, info, row } = resultGeometry(
      width,
      size,
      density,
      metadataCount,
    ),
    total = Math.ceil(items.length / columns) * row,
    pointerDown = useDragGestures({ list, gap, columns, actual, row }, items, gestures);
  useLayoutEffect(() => {
    const element = ref.current!;
    const observer = new ResizeObserver(([entry]) => {
      setWidth(entry.contentRect.width);
      setHeight(entry.contentRect.height);
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  useLayoutEffect(() => {
    const element = ref.current;
    if (!element) return;
    let target =
      Math.floor(Math.min(state.index, Math.max(0, items.length - 1)) / columns) * row +
      Math.min(state.offset, row - 1);
    if (activeId !== previousActive.current) {
      previousActive.current = activeId;
      const index = items.findIndex((p) => p.id === activeId),
        start = Math.floor(index / columns) * row;
      if (index >= 0) {
        if (start < target) target = start;
        else if (start + row > target + height) target = start + row - height;
      }
    }
    target = Math.max(0, Math.min(target, total - height));
    element.scrollTop = target;
    setTop(target);
    state.index = Math.floor(target / row) * columns;
    state.offset = target % row;
  }, [row, columns, items, height, activeId]);
  const first = Math.max(0, Math.floor(top / row) - 2),
    end = Math.min(Math.ceil(items.length / columns), Math.ceil((top + height) / row) + 2);
  return (
    <div
      className={`result-viewport ${list ? 'list-mode' : 'grid-mode'} ${density === 40 ? 'density-compact' : density === 74 ? 'density-standard' : 'density-comfortable'}`}
      ref={ref}
      data-columns={columns}
      role="listbox"
      aria-multiselectable="true"
      onPointerDown={pointerDown}
      style={
        {
          '--result-row': `${row}px`,
          '--result-info': `${info}px`,
          '--result-gap': `${gap}px`,
          '--density': `${density}px`,
        } as CSSProperties
      }
      onScroll={(e) => {
        const value = e.currentTarget.scrollTop;
        setTop(value);
        state.index = Math.floor(value / row) * columns;
        state.offset = value % row;
      }}
    >
      <div style={{ height: total, position: 'relative' }}>
        <div
          className="result-items"
          style={{
            position: 'absolute',
            top: first * row,
            left: 12,
            right: 12,
            display: 'grid',
            gridTemplateColumns: list
              ? 'minmax(0,1fr)'
              : `repeat(${columns},minmax(0,${actual}px))`,
            gap: list ? 0 : gap,
          }}
        >
          {items.slice(first * columns, end * columns).map((p) => render(p, list))}
        </div>
      </div>
    </div>
  );
}
