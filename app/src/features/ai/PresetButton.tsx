import { ChevronRight, Pencil } from 'lucide-react';
import type { Conversation } from './useConversation';
import type { ChatRequest } from './useChatRequest';
import type { ChatActions } from './useChatActions';
import type { ChatWorkspace } from './useChatWorkspace';

/** A preset that starts a new chat, with Edit for the user's own presets. */
export default function PresetButton({
  preset,
  conversation,
  request,
  chatActions,
  workspace,
}: {
  preset: ChatWorkspace['presets'][number];
  conversation: Conversation;
  request: ChatRequest;
  chatActions: ChatActions;
  workspace: ChatWorkspace;
}) {
  const { presetId } = conversation;
  const { locked } = request;
  const { newChat } = chatActions;
  const { L, setEditor } = workspace;
  return (
    <div className="ai-preset-row" key={preset.id}>
      <button
        type="button"
        className={'ai-preset-choice ' + (presetId === preset.id ? 'selected' : '')}
        disabled={locked}
        onClick={() => newChat(preset.id)}
      >
        <span>
          <strong>{preset.title}</strong>
          <small>{preset.description}</small>
        </span>
        <ChevronRight size={15} />
      </button>
      {!preset.builtin && (
        <button
          type="button"
          className="ai-preset-edit icon-button"
          disabled={locked}
          aria-label={`${L('editPreset')} ${preset.title}`}
          onClick={() => setEditor({ preset })}
        >
          <Pencil size={14} />
        </button>
      )}
    </div>
  );
}
