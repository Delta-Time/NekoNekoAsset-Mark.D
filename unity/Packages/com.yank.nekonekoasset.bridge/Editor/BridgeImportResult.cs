using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using NekoNekoAsset.Bridge.SmartPackage;
using UnityEditor;
using UnityEngine;

namespace NekoNekoAsset.Bridge
{
    public static partial class BridgeRuntime
    {
        private const string PendingImportResultKey = SessionKey + ".PendingImportResult";

        private static void ShowImportResult(StoredBridgeRequest request)
        {
            if (Application.isBatchMode || stopped || request == null || !Terminal(request.receipt.stage)
                || request.receipt.stage == "cancelled") return;
            string requestId = request.receipt.requestId;
            SessionState.SetString(PendingImportResultKey, requestId);
            EditorApplication.delayCall += () => {
                if (stopped || SessionState.GetString(PendingImportResultKey, "") != requestId) return;
                BridgeImportResultWindow.Show(request, Locale(request));
                if (SessionState.GetString(PendingImportResultKey, "") == requestId)
                    SessionState.EraseString(PendingImportResultKey);
            };
        }

        private static void RestoreImportResult()
        {
            if (Application.isBatchMode) return;
            string requestId = SessionState.GetString(PendingImportResultKey, "");
            if (TryGetRequest(requestId, out StoredBridgeRequest request)
                && request.receipt.sessionId == registry.sessionId) ShowImportResult(request);
        }

        internal static StoredBridgeRequest ImportResultRequest(string requestId)
            => TryGetRequest(requestId, out StoredBridgeRequest request) ? request : null;

        // Applied, skipped and failed files as the review counts them: a result whose path holds
        // other results is a folder, and a loose file's .meta goes with its file. The App agrees.
        internal static int[] FileCounts(List<BridgeFileResult> results)
        {
            var folders = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
            foreach (BridgeFileResult result in results)
                for (int separator = (result.path ?? "").IndexOf('/'); separator >= 0; separator = result.path.IndexOf('/', separator + 1))
                    folders.Add(result.path.Substring(0, separator));
            var counts = new int[3];
            foreach (BridgeFileResult result in results)
                if (!string.IsNullOrEmpty(result.path) && !folders.Contains(result.path)
                    && !result.path.EndsWith(".meta", StringComparison.OrdinalIgnoreCase))
                    counts[result.state == "applied" ? 0 : result.state == "skipped" ? 1 : 2]++;
            return counts;
        }

        // Only new, owned targets are eligible. Overwritten preexisting targets are
        // intentionally retained. Asset/meta hashes guard all subsequent edits.
        internal static int[] CleanupOwnedWrites(string project, StoredBridgeRequest request)
        {
            int removed = 0, retained = 0;
            if (request.cleanupDone || request.ownedWrites == null) return new[] { removed, retained };
            var writes = request.ownedWrites.OrderByDescending(write => write.path.Length).ToList();
            foreach (BridgeOwnedWrite write in writes)
            {
                try
                {
                    if (!ProjectPackagePath.TryGetAbsolutePath(project, write.path, out string target, out _))
                    { retained++; continue; }
                    var current = ObservedAsset.Capture(new FileAssetProbe(project), write.path);
                    if (!current.Exists) continue;
                    if (write.isFolder != current.IsFolder || !current.ContentKnown
                        || (!write.isFolder && current.Content.AssetHash != write.assetHash)
                        || current.Content.MetaHash != write.metaHash
                        || (write.isFolder && Directory.GetFileSystemEntries(target).Length != 0))
                    { retained++; continue; }
                    // Revalidate links and pair immediately before removing the owned bytes.
                    BridgeFiles.RejectLinks(target);
                    BridgeFiles.RejectLinks(target + ".meta");
                    if (write.isFolder) Directory.Delete(target, false);
                    else File.Delete(target);
                    if (File.Exists(target + ".meta")) File.Delete(target + ".meta");
                    removed++;
                }
                catch (IOException) { retained++; }
                catch (UnauthorizedAccessException) { retained++; }
            }
            // Keep implicit parents: empty-directory state alone cannot distinguish an
            // owned directory from one subsequently removed/recreated by another tool.
            request.cleanupDone = true;
            return new[] { removed, retained };
        }

        internal static int[] CleanupLastWrites(StoredBridgeRequest request)
        {
            if (registry == null || active != null || !Terminal(request.receipt.stage)) return new[] { 0, request.ownedWrites.Count };
            int[] counts = CleanupOwnedWrites(registry.projectRoot, request);
            Save(request);
            AssetDatabase.Refresh();
            return counts;
        }
    }

    internal sealed class BridgeImportResultWindow : EditorWindow
    {
        [SerializeField] private StoredBridgeRequest request;
        [SerializeField] private string locale;
        private BridgeLocalization labels;
        private Vector2 scroll;
        private string cleanupSummary;
        private int[] counts;
        internal static void Show(StoredBridgeRequest request, string locale)
        {
            var window = CreateInstance<BridgeImportResultWindow>();
            window.request = request;
            window.locale = locale;
            window.labels = new BridgeLocalization(locale);
            window.titleContent = new GUIContent(window.labels.Text("resultTitle"));
            if (Succeeded(request))
            {
                // A success needs no path list: its counts and the way to the imported files.
                window.minSize = new Vector2(360, 76);
                Rect main = EditorGUIUtility.GetMainWindowPosition();
                window.position = new Rect(main.x + (main.width - 420f) * .5f, main.y + (main.height - 96f) * .5f, 420f, 96f);
            }
            else window.minSize = new Vector2(440, 280);
            window.ShowUtility();
        }

        private static bool Succeeded(StoredBridgeRequest request)
            => request.receipt.stage == "completed" && !request.receipt.results.Any(result => result.state == "failed");

        private void OnEnable()
        {
            labels = new BridgeLocalization(locale);
        }

        private void OnGUI()
        {
            if (request == null || labels == null) return;
            if (Event.current.type == EventType.KeyDown && Event.current.keyCode == KeyCode.Escape)
            {
                Event.current.Use();
                Close();
                GUIUtility.ExitGUI();
            }
            request = BridgeRuntime.ImportResultRequest(request.receipt.requestId) ?? request;
            if (counts == null) counts = BridgeRuntime.FileCounts(request.receipt.results);
            int applied = counts[0], skipped = counts[1], failed = counts[2];
            bool succeeded = Succeeded(request);
            GUILayout.Label(labels.Text(succeeded ? "completed" : applied > 0 ? "partlyImported" : "failed"), EditorStyles.boldLabel);
            GUILayout.Label(labels.Format("resultCounts", applied, skipped, failed));
            if (succeeded) GUILayout.FlexibleSpace();
            else
            {
                if (!string.IsNullOrEmpty(request.receipt.error)
                    && (request.appliedFailed || !BridgeRuntime.LegacyCompilationError(request.receipt.error)))
                    EditorGUILayout.HelpBox(request.receipt.error, MessageType.Warning);
                scroll = EditorGUILayout.BeginScrollView(scroll);
                foreach (BridgeFileResult result in request.receipt.results) if (result.state == "failed") DrawResult(result);
                foreach (BridgeFileResult result in request.receipt.results) if (result.state != "failed") DrawResult(result);
                EditorGUILayout.EndScrollView();
                if (!string.IsNullOrEmpty(cleanupSummary)) GUILayout.Label(cleanupSummary, EditorStyles.wordWrappedMiniLabel);
            }
            using (new EditorGUILayout.HorizontalScope())
            {
                if (request.receipt.stage != "completed" && request.ownedWrites != null && request.ownedWrites.Count > 0 && !request.cleanupDone)
                    if (GUILayout.Button(labels.Text("cleanup"))
                        && EditorUtility.DisplayDialog(labels.Text("cleanup"), labels.Text("cleanupConfirm"), labels.Text("cleanup"), labels.Text("cancel")))
                    {
                        int[] counts = BridgeRuntime.CleanupLastWrites(request);
                        cleanupSummary = labels.Format("cleanupCounts", counts[0], counts[1]);
                    }
                GUILayout.FlexibleSpace();
                if (applied + skipped > 0 && GUILayout.Button(labels.Text("showInProject"))) ShowInProject();
                if (GUILayout.Button(labels.Text("close"))) Close();
            }
        }

        private void DrawResult(BridgeFileResult result)
        {
            using (new EditorGUILayout.HorizontalScope())
            {
                EditorGUILayout.SelectableLabel(result.path, GUILayout.Height(EditorGUIUtility.singleLineHeight));
                GUILayout.Label(labels.Text(result.state), GUILayout.Width(65));
            }
            if (!string.IsNullOrEmpty(result.message)) GUILayout.Label(result.message, EditorStyles.wordWrappedMiniLabel);
        }

        // Highlights the deepest folder holding everything imported, or else each top-level item.
        private void ShowInProject()
        {
            List<string[]> paths = request.receipt.results
                .Where(result => result.state != "failed" && !string.IsNullOrEmpty(result.path)
                    && !result.path.EndsWith(".meta", StringComparison.OrdinalIgnoreCase))
                .Select(result => result.path.Split('/')).ToList();
            if (paths.Count == 0) return;
            int common = paths[0].Length;
            foreach (string[] path in paths)
            {
                int same = 0;
                while (same < common && same < path.Length && path[same] == paths[0][same]) same++;
                common = same;
            }
            IEnumerable<string> targets = common >= 2 ? new[] { string.Join("/", paths[0].Take(common)) }
                : paths.Select(path => string.Join("/", path.Take(Math.Min(2, path.Length)))).Distinct();
            UnityEngine.Object[] found = targets.Select(AssetDatabase.LoadAssetAtPath<UnityEngine.Object>)
                .Where(target => target != null).ToArray();
            if (found.Length == 0) return;
            Selection.objects = found;
            EditorUtility.FocusProjectWindow();
            EditorGUIUtility.PingObject(found[0]);
        }
    }
}
