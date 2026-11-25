import { Field } from './import-parts';
import { boothListing, type BoothImport } from './useBoothImport';

/** BOOTH listings to add by their links: the account when there is a choice, the link field
 * and the listings so far. */
export function BoothFreeImport({
  booth,
  L,
  working,
}: {
  booth: BoothImport;
  L: (key: string) => string;
  working: boolean;
}) {
  return (
    <div className="booth-import">
      {booth.ready.length > 1 && (
        <Field label={L('account')}>
          <select
            value={booth.account?.id}
            disabled={working}
            onChange={(e) => booth.setAccountId(e.target.value)}
          >
            {booth.ready.map((account) => (
              <option key={account.id} value={account.id}>
                {account.name ?? account.label ?? account.store}
              </option>
            ))}
          </select>
        </Field>
      )}
      <div className="local-import-url">
        <input
          value={booth.text}
          disabled={working || !booth.ready.length}
          placeholder="https://booth.pm/ja/items/…"
          aria-label={L('boothFree')}
          onChange={(e) => booth.setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && !e.nativeEvent.isComposing) {
              e.preventDefault();
              booth.add();
            }
          }}
        />
        <button
          disabled={working || !booth.ready.length || !booth.text.trim()}
          onClick={() => booth.add()}
        >
          {L('add')}
        </button>
      </div>
      {booth.inputError && (
        <div className="wf-error" role="alert">
          {L(booth.inputError)}
        </div>
      )}
      {!booth.ready.length && (
        <div className="wf-error" role="alert">
          {L('boothSignInRequired')}
        </div>
      )}
      <div className="data-list booth-import-list" role="list" aria-label={L('boothFree')}>
        {booth.ids.map((id) => (
          <div className="data-row" role="listitem" key={id}>
            <span title={boothListing(id)}>{boothListing(id)}</span>
            <button
              disabled={working}
              aria-label={`${L('remove')} ${boothListing(id)}`}
              onClick={() => booth.remove(id)}
            >
              ×
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}
