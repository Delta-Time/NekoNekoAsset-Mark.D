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
        internal const string PerFileFailures = "Some files were not imported; inspect the per-file results.";

        private static void ShowImportResult(StoredBridgeRequest request)
        {
            // A clean import opens nothing; the App's Activity row still shows its counts.
            if (Application.isBatchMode || stopped || request == null || !Terminal(request.receipt.stage)
                || request.receipt.stage == "cancelled" || BridgeImportResultWindow.Clean(request)) return;
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

        // Imported, skipped and failed files as the review counts them: a result whose path holds
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
        private List<BridgeFileResult> failures;
        internal static void Show(StoredBridgeRequest request, string locale)
        {
            var window = CreateInstance<BridgeImportResultWindow>();
            window.request = request;
            window.locale = locale;
            window.labels = new BridgeLocalization(locale);
            window.titleContent = new GUIContent(window.labels.Text("resultTitle"));
            window.minSize = new Vector2(440, 280);
            window.ShowUtility();
        }

        // Completed with no failed file and no error. A Bridge before 0.2.6 stored a compiler note
        // once every file was imported; Unity's Console owns those, so that is clean too.
        internal static bool Clean(StoredBridgeRequest request)
            => !request.receipt.results.Any(result => result.state == "failed")
                && ((request.receipt.stage == "completed" && string.IsNullOrEmpty(request.receipt.error))
                    || (!request.appliedFailed && BridgeRuntime.LegacyCompilationError(request.receipt.error)));

        // What failed, once: a loose file's .meta row fails with its file and repeats its reason.
        internal static List<BridgeFileResult> Failures(List<BridgeFileResult> results)
        {
            List<BridgeFileResult> failed = results.Where(result => result.state == "failed").ToList();
            var paths = new HashSet<string>(failed.Select(result => result.path ?? ""), StringComparer.OrdinalIgnoreCase);
            return failed.Where(result => !(result.path ?? "").EndsWith(".meta", StringComparison.OrdinalIgnoreCase)
                || !paths.Contains(result.path.Substring(0, result.path.Length - 5))).ToList();
        }

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
            if (failures == null)
            {
                counts = BridgeRuntime.FileCounts(request.receipt.results);
                failures = Failures(request.receipt.results);
            }
            int imported = counts[0], skipped = counts[1];
            GUILayout.Label(labels.Text(imported > 0 ? "partlyImported" : "failed"), EditorStyles.boldLabel);
            // Failed counts the rows listed below, a failed folder included, so the two always agree.
            if (request.receipt.results.Count > 0) GUILayout.Label(labels.Format("resultCounts", imported, skipped, failures.Count));
            string error = request.receipt.error;
            // The listed failures already say what the generic per-file sentence says.
            if (!string.IsNullOrEmpty(error) && !(failures.Count > 0 && error == labels.Message(BridgeRuntime.PerFileFailures))
                && (request.appliedFailed || !BridgeRuntime.LegacyCompilationError(error)))
                EditorGUILayout.HelpBox(error, MessageType.Warning);
            // Only what failed, and why; the counts cover the rest.
            scroll = EditorGUILayout.BeginScrollView(scroll);
            foreach (BridgeFileResult result in failures) DrawFailure(result);
            EditorGUILayout.EndScrollView();
            if (!string.IsNullOrEmpty(cleanupSummary)) GUILayout.Label(cleanupSummary, EditorStyles.wordWrappedMiniLabel);
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
                if (imported + skipped > 0 && GUILayout.Button(labels.Text("showInProject"))) ShowInProject();
                if (GUILayout.Button(labels.Text("close"))) Close();
            }
        }

        private static void DrawFailure(BridgeFileResult result)
        {
            EditorGUILayout.SelectableLabel(result.path, GUILayout.Height(EditorGUIUtility.singleLineHeight));
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
