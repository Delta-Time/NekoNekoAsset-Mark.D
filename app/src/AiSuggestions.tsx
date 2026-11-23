import { ImagePlus } from 'lucide-react';
import type { Locale } from './model';
import AiPresetEditor from './AiPresetEditor';
import './ai-suggestions.css';
import './ai-chat.css';
import './ai-suggestions-responsive.css';
import { useChatWorkspace } from './features/ai/useChatWorkspace';
import { useConversation } from './features/ai/useConversation';
import { useHistoryEdits } from './features/ai/useHistoryEdits';
import { useSearchOptions } from './features/ai/useSearchOptions';
import { useComposer } from './features/ai/useComposer';
import { useChatRequest } from './features/ai/useChatRequest';
import { useImageDrop } from './features/ai/useImageDrop';
import { useChatActions } from './features/ai/useChatActions';
import { useSendMessage } from './features/ai/useSendMessage';
import ChatSidebar from './features/ai/ChatSidebar';
import ChatOptions from './features/ai/ChatOptions';
import ChatMessages from './features/ai/ChatMessages';
import ChatComposer from './features/ai/ChatComposer';

interface AiSuggestionsProps {
  locale: Locale;
  active?: boolean;
  frozen?: boolean;
  onOpenProduct: (productId: string) => void;
  avatarOptions?: { id: string; label: string }[];
  initialAvatarId?: string;
  onUpdateBlockedChange?: (blocked: boolean) => void;
}

export default function AiSuggestions({
  locale,
  active = true,
  frozen = false,
  onOpenProduct,
  avatarOptions = [],
  initialAvatarId = '',
  onUpdateBlockedChange,
}: AiSuggestionsProps) {
  const workspace = useChatWorkspace({ locale, active });
  const { L, setCustomPresets, editor, setEditor } = workspace;
  const conversation = useConversation({ workspace });
  const { main } = conversation;
  const history = useHistoryEdits();
  const search = useSearchOptions({ initialAvatarId });
  const composer = useComposer({ active });
  const { dropActive, setDropActive } = composer;
  const request = useChatRequest({
    workspace,
    conversation,
    history,
    composer,
    search,
    active,
    frozen,
    onUpdateBlockedChange,
  });
  const imageDrop = useImageDrop({ request, workspace, composer, active, frozen });
  const { drop, paste } = imageDrop;
  const chatActions = useChatActions({ conversation, request, history, workspace });
  const sending = useSendMessage({
    request,
    workspace,
    history,
    conversation,
    composer,
    search,
    locale,
    frozen,
    onUpdateBlockedChange,
  });
  const chatSidebar = (
    <ChatSidebar
      conversation={conversation}
      request={request}
      chatActions={chatActions}
      workspace={workspace}
      history={history}
    />
  );
  const optionsBar = (
    <ChatOptions
      workspace={workspace}
      search={search}
      request={request}
      locale={locale}
      avatarOptions={avatarOptions}
    />
  );
  const messageLog = (
    <ChatMessages
      workspace={workspace}
      conversation={conversation}
      request={request}
      onOpenProduct={onOpenProduct}
    />
  );
  const composerForm = (
    <ChatComposer
      sending={sending}
      composer={composer}
      imageDrop={imageDrop}
      request={request}
      workspace={workspace}
      conversation={conversation}
      search={search}
      locale={locale}
      frozen={frozen}
    />
  );
  return (
    <section
      className={'ai-chat ' + (dropActive ? 'ai-drop-active' : '')}
      onDragOver={(event) => {
        if (event.dataTransfer.types.includes('Files')) {
          event.preventDefault();
          setDropActive(true);
        }
      }}
      onDragLeave={(event) => {
        if (
          !(event.relatedTarget instanceof Node) ||
          !event.currentTarget.contains(event.relatedTarget)
        )
          setDropActive(false);
      }}
      onDrop={drop}
      onPaste={paste}
    >
      {dropActive && (
        <div className="ai-drop-indicator" role="status">
          <ImagePlus size={24} />
          {L('dropImages')}
        </div>
      )}
      {chatSidebar}
      <div className="ai-chat-main" ref={main}>
        {optionsBar}
        {messageLog}
        {composerForm}
      </div>
      {editor && (
        <AiPresetEditor
          key={editor.preset?.id ?? 'new'}
          locale={locale}
          preset={editor.preset}
          onSaved={(value) => {
            setCustomPresets((previous) => [
              ...previous.filter((preset) => preset.id !== value.id),
              value,
            ]);
          }}
          onClose={() => setEditor(null)}
        />
      )}
    </section>
  );
}
