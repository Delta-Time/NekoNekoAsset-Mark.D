import { getProductDisplayStatus } from '../../model';
import type { Product } from '../../model';
import { StatusIcon } from '../../LibraryIcons';

export default function ProductStatus({ p, q }: { p: Product; q: (key: string) => string }) {
  const status = getProductDisplayStatus(p);
  return (
    <span
      className={`product-status ${status === 'ready' ? 'good' : status === 'attention' ? 'warn' : ''}`}
      data-status={status}
      title={q(status)}
      aria-label={q(status)}
    >
      <StatusIcon status={status} />
      <span title={q(status)}>{q(status)}</span>
    </span>
  );
}
