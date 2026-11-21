import { ExternalLink, ImagePlus } from 'lucide-react';
import type { AiSuggestionResponse } from '../../ai-backend';
import Preview from '../../Preview';
import { contentLang } from '../../content-lang';
import type { ChatWorkspace } from './useChatWorkspace';

/** A recommended product: library results open it, store results open their page. */
export default function ChatResult({
  result,
  workspace,
  onOpenProduct,
}: {
  result: AiSuggestionResponse['recommendations'][number];
  workspace: ChatWorkspace;
  onOpenProduct: (productId: string) => void;
}) {
  const { storeLabel, L, openPage } = workspace;
  const source = result.thumbnailPath || result.thumbnailUrl;
  const body = (
    <>
      <div className="ai-chat-result-image">
        {source ? <Preview src={source} title={result.title} lazy /> : <ImagePlus size={25} />}
        <span>{storeLabel(result.provider)}</span>
      </div>
      <div className="ai-chat-result-body">
        <strong lang={contentLang(result.title)}>{result.title}</strong>
        {result.reason && <small>{result.reason}</small>}
      </div>
    </>
  );
  return result.origin === 'library' && result.productId ? (
    <button
      type="button"
      className="ai-chat-result"
      key={result.candidateId}
      title={L('openProduct')}
      onClick={() => onOpenProduct(result.productId!)}
    >
      {body}
    </button>
  ) : (
    <div className="ai-chat-result" key={result.candidateId}>
      {body}
      {result.url && (
        <button
          type="button"
          className="ai-chat-result-action"
          onClick={() => void openPage(result.url!)}
        >
          <ExternalLink size={13} />
          {L('openPage')}
        </button>
      )}
    </div>
  );
}
