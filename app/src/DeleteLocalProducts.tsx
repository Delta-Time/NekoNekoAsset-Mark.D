import ConfirmProducts from './ConfirmProducts';
import type { Locale } from './model';
import { shellText } from './shell-i18n';
import { t } from './i18n';

export default function DeleteLocalProducts({
  locale,
  ids,
  names,
  onDelete,
  onClose,
}: {
  locale: Locale;
  ids: string[];
  names: string[];
  onDelete: (ids: string[]) => Promise<void>;
  onClose: () => void;
}) {
  const L = (key: string) => shellText(locale, key) ?? t(locale, key);
  return (
    <ConfirmProducts
      locale={locale}
      title={L('deleteLocalProducts')}
      message={L('confirmDeleteLocalProducts')}
      count={ids.length}
      names={names}
      confirm={L('delete')}
      danger
      onClose={onClose}
      onConfirm={async () => {
        // onNative reports any queue admission error through the shell notice.
        void onDelete(ids).catch(() => {});
      }}
    />
  );
}
