import { isTauri, invoke } from '@tauri-apps/api/core';
import { invokeAction } from './backend';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import type {
  AiJobRequest,
  AiJobResponses,
  AiMode,
  AiPersonalization,
  AiPresetDraft,
  AiRequest,
  AiResponses,
  AiScopes,
  AiSuggestionInput,
  AiSuggestionRequest,
  AiSuggestionResponses,
} from './bindings';
export type {
  AiChatDetail,
  AiChatSummary,
  AiConnectionResult,
  AiCustomPreset,
  AiDraftTest,
  AiEntry,
  AiEntryList,
  AiImage,
  AiKeyStatus,
  AiMode,
  AiPresetDraft,
  AiProviderPreset,
  AiScopes,
  AiStoredTurn,
  AiSuggestionInput,
  AiSuggestionResponse,
  AiWorkspace,
} from './bindings';

/** A provider entry being tested or saved. */
export type AiDraft = Omit<Extract<AiRequest, { action: 'test_draft' }>, 'action'>;
async function invokeAi<A extends AiRequest['action']>(
  request: Extract<AiRequest, { action: A }>,
): Promise<AiResponses[A]> {
  if (!isTauri()) throw new Error('nativeUnavailable');
  return invoke('ai_action', { request });
}
export function getAiKeyStatus() {
  return invokeAi({ action: 'key_status' });
}
export function testAiConnection() {
  return invokeAi({ action: 'test_connection' });
}
export function getAiProviders() {
  return invokeAi({ action: 'providers' });
}
export function listAiEntries() {
  return invokeAi({ action: 'list_entries' });
}
export function testAiDraft(draft: AiDraft) {
  return invokeAi({ action: 'test_draft', ...draft });
}
export function saveAiEntry(draft: AiDraft & { testToken: string }) {
  return invokeAi({ action: 'save_entry', ...draft });
}
export function selectAiEntry(entryId: string) {
  return invokeAi({ action: 'select_entry', entryId });
}
export function removeAiEntry(entryId: string) {
  return invokeAi({ action: 'remove_entry', entryId });
}
export function testAiEntry(entryId: string) {
  return invokeAi({ action: 'test_entry', entryId });
}
async function invokeAiSuggestion<A extends AiSuggestionRequest['action']>(
  request: Extract<AiSuggestionRequest, { action: A }>,
): Promise<AiSuggestionResponses[A]> {
  if (!isTauri()) throw new Error('nativeUnavailable');
  return invoke('ai_suggestion_action', { request });
}
export function sendAiSuggestion(input: AiSuggestionInput) {
  return invokeAiSuggestion({ action: 'send', ...input });
}
export function cancelAiSuggestion(requestId: string) {
  return invokeAiSuggestion({ action: 'cancel', requestId });
}
async function invokeAiAnalysis<A extends AiJobRequest['action']>(
  request: Extract<AiJobRequest, { action: A }>,
): Promise<AiJobResponses[A]> {
  if (!isTauri()) throw new Error('nativeUnavailable');
  return invoke('ai_analysis_action', { request });
}
export function startAi(request: {
  mode: AiMode;
  productIds: string[];
  scopes: AiScopes;
}): Promise<{ jobId: string }> {
  return invokeAiAnalysis({ action: 'start', ...request });
}
export function cancelAi(jobId: string): Promise<unknown> {
  return invokeAiAnalysis({ action: 'cancel', jobId });
}
export function retryAi(jobId: string): Promise<{ jobId: string }> {
  return invokeAiAnalysis({ action: 'retry', jobId });
}
export function dismissAiSourceUrlCandidate(request: {
  productId: string;
  url: string;
}): Promise<unknown> {
  return invokeAiAnalysis({ action: 'dismiss_source_url', ...request });
}
export function saveAiPreferences(patch: {
  aiEnabled: boolean;
  aiScopes: AiScopes;
}): Promise<unknown> {
  return invokeAction('catalogue', 'preferences', { patch });
}
export function listAiWorkspace() {
  return invokeAiSuggestion({ action: 'list_workspace' });
}
export function createAiChat(presetId: string) {
  return invokeAiSuggestion({ action: 'create_chat', presetId });
}
export function getAiChat(chatId: string) {
  return invokeAiSuggestion({ action: 'get_chat', chatId });
}
export async function renameAiChat(chatId: string, title: string) {
  const result = await invokeAiSuggestion({ action: 'rename_chat', chatId, title });
  return result.chat;
}
export function deleteAiChat(chatId: string) {
  return invokeAiSuggestion({ action: 'delete_chat', chatId });
}
export async function saveAiPersonalization(personalization: AiPersonalization) {
  const result = await invokeAiSuggestion({ action: 'save_personalization', personalization });
  return result.personalization;
}
export async function saveAiPreset(preset: AiPresetDraft) {
  const result = await invokeAiSuggestion({ action: 'save_preset', preset });
  return result.preset;
}
export function deleteAiPreset(presetId: string) {
  return invokeAiSuggestion({ action: 'delete_preset', presetId });
}
export async function importAiPreset(json: string) {
  const result = await invokeAiSuggestion({ action: 'import_preset', json });
  return result.preset;
}
export function exportAiPreset(presetId: string) {
  return invokeAiSuggestion({ action: 'export_preset', presetId });
}
export async function readAiDroppedImages(paths: string[]) {
  const result = await invokeAiSuggestion({ action: 'read_images', paths });
  return result.images;
}
export async function subscribeAiImageDrop(
  callback: (paths: string[]) => void,
): Promise<() => void> {
  if (!isTauri()) return () => {};
  return getCurrentWebview().onDragDropEvent((event) => {
    if (event.payload.type === 'drop') callback(event.payload.paths);
  });
}
