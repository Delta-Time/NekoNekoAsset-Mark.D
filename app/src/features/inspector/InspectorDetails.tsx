import type { ReactNode } from 'react';
import { ChevronDown, ExternalLink, Plus, SlidersHorizontal, X } from 'lucide-react';
import type { Locale, Product } from '../../model';
import { currentPriceLabel, fileCountLabel, priceLabel } from '../../model';
import { table } from '../../messages';
import Preview from '../../Preview';
import { StatusIcon } from '../../LibraryIcons';
import { contentLang } from '../../content-lang';
import VariantPrices from './VariantPrices';
import type { InspectorFacts } from './useInspectorFacts';
import type { InspectorPanes } from './useInspectorPanes';
import type { TagFold } from './useTagFold';

/** The details pane: image, title, status and Attention, tags, category, sources, dates and
 * prices, compatible avatars, note and the actions. */
export default function InspectorDetails({
  facts,
  panes,
  tagFold,
  p,
  categoryLabel,
  actions,
  onReveal,
  onEdit,
  onTagClick,
  onDismissAttention,
  locale,
}: {
  facts: InspectorFacts;
  panes: InspectorPanes;
  tagFold: TagFold;
  p: Product;
  categoryLabel: string | undefined;
  actions: ReactNode;
  onReveal: () => void;
  onEdit: () => void;
  onTagClick: ((tag: string) => void) | undefined;
  onDismissAttention: (() => void) | undefined;
  locale: Locale;
}) {
  const {
    image,
    blocked,
    L,
    pageAvailable,
    openPage,
    status,
    currentPrices,
    paidPrices,
    paidFallback,
    unpaidRow,
    owned,
    sourceCategory,
    productSources,
    accountSources,
  } = facts;
  const { detailScroll } = panes;
  const { tagClamp, tagsExpanded, setExpandedTags, tagsId, setTagList, tags } = tagFold;
  return (
    <div className="asset-inspector-scroll" ref={detailScroll}>
      {image && !blocked && (
        <div className="asset-inspector-ambient" aria-hidden="true">
          <Preview key={image} src={image} title="" />
        </div>
      )}
      <div className="asset-inspector-preview">
        {blocked ? (
          <button type="button" className="asset-inspector-reveal" onClick={onReveal}>
            {L('showR18')}
          </button>
        ) : pageAvailable ? (
          <button
            type="button"
            className="asset-inspector-page-link"
            aria-label={table(locale, 'fileAction').openPage}
            title={table(locale, 'fileAction').openPage}
            onClick={openPage}
          >
            <Preview src={image} title={p.title} />
            <span className="asset-inspector-page-hint" aria-hidden="true">
              <ExternalLink size={14} />
              <span>{table(locale, 'fileAction').openPage}</span>
            </span>
          </button>
        ) : (
          <Preview src={image} title={p.title} />
        )}
      </div>
      <div className="asset-inspector-body">
        <h2 lang={contentLang(p.title)} title={p.title}>
          {p.title}
        </h2>
        <div className="asset-inspector-shop">
          {p.provider === 'local'
            ? L('localProductShop')
            : p.shop && <span lang={contentLang(p.shop)}>{p.shop}</span>}
          {p.custom && <span className="badge">{L('custom')}</span>}
        </div>
        <div className="asset-inspector-status">
          <span
            className={
              'asset-inspector-state ' +
              (status === 'ready' ? 'positive' : status === 'attention' ? 'warning' : '')
            }
            data-status={status}
          >
            <StatusIcon status={status} />
            <span>{L(status)}</span>
            {status === 'attention' && onDismissAttention && (
              <button
                type="button"
                className="asset-inspector-dismiss"
                title={L('dismissAttention')}
                aria-label={L('dismissAttention')}
                onClick={(event) => {
                  event.currentTarget
                    .closest('.asset-inspector-body')
                    ?.querySelector<HTMLElement>('.detail-actions button:not(:disabled)')
                    ?.focus({ preventScroll: true });
                  onDismissAttention();
                }}
              >
                <X size={12} />
              </button>
            )}
          </span>
          <span
            className="asset-inspector-count"
            title={p.manifestComplete === true ? undefined : L('totalUnverified')}
          >
            {fileCountLabel(p)}
          </span>
        </div>
        {actions}
        <section className="asset-inspector-section">
          <div className="asset-inspector-label">{L('tags')}</div>
          <div
            className={'asset-inspector-tags-frame' + (tagClamp ? ' clamped' : '')}
            style={
              tagClamp ? { maxHeight: tagsExpanded ? tagClamp.full : tagClamp.clamp } : undefined
            }
            onFocus={(e) => {
              const item = e.target as HTMLElement;
              if (tagClamp && !tagsExpanded && item.offsetTop + item.offsetHeight > tagClamp.clamp)
                setExpandedTags(p.id);
            }}
          >
            <div className="asset-inspector-tags" id={tagsId} ref={setTagList}>
              {tags.map((tag) => (
                <button
                  type="button"
                  key={tag}
                  lang={contentLang(tag)}
                  title={tag}
                  onClick={() => onTagClick?.(tag)}
                >
                  {tag}
                </button>
              ))}
              <button
                type="button"
                className="asset-inspector-tag-add"
                aria-label={L('edit') + ' ' + L('tags')}
                title={L('edit') + ' ' + L('tags')}
                onClick={onEdit}
              >
                <Plus size={12} />
              </button>
            </div>
          </div>
          {tagClamp && (
            <button
              type="button"
              className="asset-inspector-tags-toggle"
              aria-expanded={tagsExpanded}
              aria-controls={tagsId}
              onClick={() => setExpandedTags(tagsExpanded ? '' : p.id)}
            >
              <span>{L(tagsExpanded ? 'showFewerTags' : 'showAllTags')}</span>
              <ChevronDown size={13} aria-hidden="true" />
            </button>
          )}
        </section>
        <section className="asset-inspector-section">
          <div className="asset-inspector-label">
            {L('note')}
            <button
              type="button"
              aria-label={L('edit') + ' ' + L('note')}
              title={L('edit') + ' ' + L('note')}
              onClick={onEdit}
            >
              <SlidersHorizontal size={12} />
            </button>
          </div>
          <p className="asset-inspector-note">{p.note || '—'}</p>
        </section>
        <section className="asset-inspector-section asset-inspector-metadata">
          <div>
            <span>{L('category')}</span>
            <span>{categoryLabel ?? L(p.category)}</span>
          </div>
          <div>
            <span>{L('acquired')}</span>
            <span>{p.acquired || L('unknown')}</span>
          </div>
          <div>
            <span>{L('currentPrice')}</span>
            <VariantPrices
              rows={
                currentPrices.some((row) => priceLabel(locale, row.price, row.currency))
                  ? currentPrices
                  : []
              }
              fallback={currentPriceLabel(locale, { ...p, ownedCurrentPrices: currentPrices })}
              facts={facts}
              locale={locale}
            />
          </div>
          <div>
            <span>{L('paidPrice')}</span>
            <VariantPrices
              rows={paidPrices}
              fallback={paidFallback}
              missing={unpaidRow}
              facts={facts}
              locale={locale}
            />
          </div>
        </section>
        <details className="asset-inspector-section asset-inspector-compatibility">
          <summary>{L('compatibility')}</summary>
          <div className="asset-inspector-metadata">
            <div>
              <span>{L('yourFiles')}</span>
              <strong>{owned.join(' · ') || L('unknown')}</strong>
            </div>
            <div>
              <span>{L('listing')}</span>
              <span>{p.listingAvatars.join(' · ') || L('unknown')}</span>
            </div>
            <div>
              <span>{L('localEvidence')}</span>
              <span>{L('unknown')}</span>
            </div>
          </div>
        </details>
        {(sourceCategory || productSources.length > 0 || accountSources.length > 0) && (
          <details className="asset-inspector-section asset-inspector-source">
            <summary>{L('sourceDetails')}</summary>
            {sourceCategory && <p>{sourceCategory}</p>}
            {[...productSources, ...accountSources].map((line, index) => (
              <p key={index}>{line}</p>
            ))}
          </details>
        )}
      </div>
    </div>
  );
}
