using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using UnityEditor;
using UnityEngine;

namespace NekoNekoAsset.Bridge
{
    [InitializeOnLoad]
    public static partial class BridgeRuntime
    {
        private const string MenuPath = "Tools/Yan-K/NekoNekoAsset Bridge";
        private const string SessionKey = "NekoNekoAsset.Bridge.ProcessSession";
        private static readonly ConcurrentQueue<Action> MainActions = new ConcurrentQueue<Action>();
        private static readonly Dictionary<string, StoredBridgeRequest> Requests = new Dictionary<string, StoredBridgeRequest>(StringComparer.OrdinalIgnoreCase);
        private static readonly HashSet<string> PendingTerminalSaves = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        // Finished receipts stay on disk for same-ID queries and load only when one arrives.
        private static readonly HashSet<string> FinishedIds = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        private const string FinishedMarker = "finished";
        private static readonly byte[] Cleaned = { (byte)'c' };
        // What the listener thread may answer while the main thread is held by a long import.
        private sealed class PublishedHello { internal string capability, instanceId, sessionId, json; }
        private sealed class PublishedStatus { internal string requestId, fingerprint, sessionId, json; }
        private static volatile PublishedHello publishedHello;
        private static volatile PublishedStatus publishedStatus;
        // True only while writing files or refreshing holds the main thread for a long time.
        private static volatile bool holdingMainThread;
        private static readonly object RegistryGate = new object();
        private static BridgeRegistry publishedRegistry;
        private static double lastMainPublish;
        private static Timer heartbeat;
        private static BridgeRegistry registry;
        private static BridgeServer server;
        private static StoredBridgeRequest active;
        private static ImportReviewWindow review;
        private static string registryPath;
        private static string requestsRoot;
        private static string enabledPreference;
        private static int mainThread;
        private static bool stopped;
        private static double nextHeartbeat;

        static BridgeRuntime()
        {
            mainThread = Thread.CurrentThread.ManagedThreadId;
            // A one-shot delayCall can be lost during Editor startup/reload.
            EditorApplication.update += InitializeWhenReady;
        }

        private static void InitializeWhenReady()
        {
            if (EditorApplication.isCompiling || EditorApplication.isUpdating) return;
            EditorApplication.update -= InitializeWhenReady;
            Initialize();
        }

        [MenuItem(MenuPath)]
        private static void Toggle()
        {
            if (server == null)
            {
                if (!EditorApplication.isCompiling && !EditorApplication.isUpdating) Initialize();
                return;
            }
            registry.enabled = !registry.enabled;
            EditorPrefs.SetBool(enabledPreference, registry.enabled);
            Menu.SetChecked(MenuPath, registry.enabled);
            if (!registry.enabled && active != null && !active.receipt.writesStarted) CancelRequest(active);
            Publish();
        }

        [MenuItem(MenuPath, true)]
        private static bool ValidateMenu()
        {
            Menu.SetChecked(MenuPath, server != null && registry != null && registry.enabled);
            // Keep retry reachable if startup did not run or initialization failed.
            return !stopped;
        }

        private static void Initialize()
        {
            if (server != null || stopped) return;
            try
            {
                string root = Path.GetFullPath(Path.GetDirectoryName(Application.dataPath));
                string normalized = root.Replace('\\', '/').TrimEnd('/');
                if (Application.platform == RuntimePlatform.WindowsEditor) normalized = normalized.ToLowerInvariant();
                string instance = BridgeFiles.Hash(BridgeFiles.Utf8.GetBytes(normalized));
                string session = SessionState.GetString(SessionKey, "");
                if (!BridgeFiles.IsUuid(session))
                {
                    session = Guid.NewGuid().ToString("D");
                    SessionState.SetString(SessionKey, session);
                }
                enabledPreference = "NekoNekoAsset.Bridge.Enabled." + instance;
                registry = new BridgeRegistry {
                    instanceId = instance, sessionId = session,
                    processId = Process.GetCurrentProcess().Id,
                    projectName = Path.GetFileName(root), projectRoot = root,
                    unityVersion = Application.unityVersion,
                    bridgeVersion = UnityEditor.PackageManager.PackageInfo.FindForAssembly(typeof(BridgeRuntime).Assembly)?.version,
                    capability = BridgeFiles.RandomCapability(), enabled = EditorPrefs.GetBool(enabledPreference, true)
                };
                string localRoot = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
                // Authored batch verification keeps test discovery wholly within its owned workspace.
                string testRegistry = Environment.GetEnvironmentVariable("NEKONEKOASSET_BRIDGE_TEST_REGISTRY");
                if (Application.isBatchMode && !string.IsNullOrEmpty(testRegistry)) localRoot = Path.GetFullPath(testRegistry);
                BridgeFiles.ConfigureTrustedRoots(root, localRoot);
                registryPath = BridgeFiles.Under(Path.Combine(localRoot, "NekoNekoAsset", "UnityBridge", "instances"), session + ".json");
                requestsRoot = BridgeFiles.Under(root, "Library/NekoNekoAssetBridge/requests");
                Directory.CreateDirectory(requestsRoot);
                RestoreRequests();
                server = new BridgeServer(BeginFromListener,
                    (submission, success, error) => OnMain(() => FinishTransfer(submission, success, error), true));
                registry.port = server.Port;
                EditorApplication.update += Update;
                EditorApplication.quitting += Shutdown;
                AssemblyReloadEvents.beforeAssemblyReload += Shutdown;
                Menu.SetChecked(MenuPath, registry.enabled);
                Publish();
                heartbeat = new Timer(HeartbeatWhileBusy, null, 2000, 2000);
                RestoreImportResult();
            }
            catch (Exception error)
            {
                server?.Dispose();
                server = null;
                BridgeFiles.ClearTrustedRoots();
                UnityEngine.Debug.LogError("NekoNekoAsset Bridge could not initialize its local storage/listener (" + error.GetType().Name + ").");
            }
        }

        private static T OnMain<T>(Func<T> work, bool durableTransferFinish = false, int wait = 10000)
        {
            if (Thread.CurrentThread.ManagedThreadId == mainThread) return work();
            var completion = new TaskCompletionSource<T>();
            int ownership = 0; // pending=0, main-thread-owned=1, abandoned-before-start=2
            MainActions.Enqueue(() => {
                if (Interlocked.CompareExchange(ref ownership, 1, 0) != 0 || stopped) { completion.TrySetCanceled(); return; }
                try { completion.TrySetResult(work()); }
                catch (Exception error) { completion.TrySetException(error); }
            });
            if (!completion.Task.Wait(wait))
            {
                if (!durableTransferFinish && Interlocked.CompareExchange(ref ownership, 2, 0) == 0)
                    throw new TimeoutException("Editor is busy.");
                // A verified owned transfer must still checkpoint after a busy Editor recovers;
                // losing its TCP ACK never abandons that already reserved request.
                // Once the main thread owns a command, its caller receives the result even
                // if the peer timed out. Failed response IO then releases the reserved request.
            }
            return completion.Task.GetAwaiter().GetResult();
        }

        private static void Update()
        {
            while (MainActions.TryDequeue(out Action action)) action();
            if (EditorApplication.timeSinceStartup >= nextHeartbeat)
            {
                nextHeartbeat = EditorApplication.timeSinceStartup + 2;
                foreach (string id in PendingTerminalSaves.ToArray())
                    if (Requests.TryGetValue(id, out StoredBridgeRequest pending) && SaveTerminal(pending))
                        FinishImportResult(pending);
                try { Publish(); }
                catch (IOException) { /* Discovery remains stale; never claim connection success. */ }
                catch (UnauthorizedAccessException) { }
            }
            ObserveRefresh();
            if (active != null && active.receipt.stage == "staging" && EditorApplication.timeSinceStartup >= active.nextPreparationPublish)
            {
                active.nextPreparationPublish = EditorApplication.timeSinceStartup + .5;
                ReportProgress(active, new BridgePreparationProgress { phase = "receive",
                    bytes = Interlocked.Read(ref active.receivedBytes), totalBytes = active.expectedBytes });
            }
            if (active != null && active.receipt.stage == "preparing")
            {
                if (!active.preparingReview) ShowReview(active);
                if (active != null && active.receipt.stage == "preparing") PublishPreparationProgress(active);
            }
        }

        // Unity's own Import Unity Package window still waits for the user. Another package
        // would replace its contents, so the Editor stays busy until that window closes.
        private static bool Busy => active != null || Resources.FindObjectsOfTypeAll<EditorWindow>()
            .Any(window => window != null && window.GetType().FullName == "UnityEditor.PackageImport");

        private static void Publish()
        {
            if (registry == null || string.IsNullOrEmpty(registryPath)) return;
            lock (RegistryGate)
            {
                try
                {
                    registry.busy = Busy;
                    registry.heartbeatUtc = BridgeFiles.Now();
                    if (registry.enabled && !stopped) BridgeFiles.AtomicWrite(registryPath, JsonUtility.ToJson(registry));
                    else if (File.Exists(registryPath)) { BridgeFiles.RejectLinks(registryPath); File.Delete(registryPath); }
                }
                catch (IOException) { /* Discovery failure never changes durable import status. */ }
                catch (UnauthorizedAccessException) { }
                lastMainPublish = CheckpointClock.Elapsed.TotalSeconds;
                publishedRegistry = registry.enabled && !stopped ? JsonUtility.FromJson<BridgeRegistry>(JsonUtility.ToJson(registry)) : null;
                BridgeResponse hello = Response(registry.enabled ? "ready" : "disabled", registry.enabled);
                hello.projectName = registry.projectName;
                hello.projectRoot = registry.projectRoot;
                hello.unityVersion = registry.unityVersion;
                hello.bridgeVersion = registry.bridgeVersion;
                hello.busy = true; // Only answered while the main thread does not respond.
                publishedHello = new PublishedHello { capability = registry.capability, instanceId = registry.instanceId,
                    sessionId = registry.sessionId, json = SerializeResponse(hello) };
            }
        }

        // A long import or refresh holds the main thread; keep discovery fresh and marked busy.
        private static void HeartbeatWhileBusy(object state)
        {
            lock (RegistryGate)
            {
                BridgeRegistry copy = publishedRegistry;
                if (stopped || !holdingMainThread || copy == null || CheckpointClock.Elapsed.TotalSeconds - lastMainPublish < 4) return;
                copy.busy = true;
                copy.heartbeatUtc = BridgeFiles.Now();
                try { BridgeFiles.AtomicWrite(registryPath, JsonUtility.ToJson(copy)); }
                catch (IOException) { }
                catch (UnauthorizedAccessException) { }
            }
        }

        // Most commands run on the main thread. While it stays busy, hello and status answer from
        // what it last published, so the App keeps real progress and the Editor stays listed as busy.
        private static BridgeSubmission BeginFromListener(string json)
        {
            BridgeCommand command = null;
            try { command = JsonUtility.FromJson<BridgeCommand>(json); }
            catch (Exception) { }
            if (command == null || (command.command != "hello" && command.command != "status") || !holdingMainThread)
                return OnMain(() => BeginCommand(json));
            try { return OnMain(() => BeginCommand(json), wait: 2000); }
            catch (TimeoutException)
            {
                string reply = PublishedReply(command);
                if (reply == null) throw;
                return new BridgeSubmission { ResponseJson = reply };
            }
        }

        private static string PublishedReply(BridgeCommand command)
        {
            PublishedHello hello = publishedHello;
            if (hello == null || command.version != 1 || !BridgeFiles.EqualCapability(command.capability, hello.capability)
                || command.instanceId != hello.instanceId) return null;
            if (command.command == "hello") return command.sessionId == hello.sessionId ? hello.json : null;
            PublishedStatus status = publishedStatus;
            if (status == null || command.requestId != status.requestId || command.fingerprint != status.fingerprint
                || (command.sessionId != status.sessionId && command.sessionId != hello.sessionId)) return null;
            return status.json;
        }

        private static string RequestDirectory(string requestId) => BridgeFiles.Under(requestsRoot, requestId);
        private static string InputsDirectory(StoredBridgeRequest request) => BridgeFiles.Under(RequestDirectory(request.receipt.requestId), "inputs");

        private static void Save(StoredBridgeRequest request)
        {
            request.receipt.updatedUtc = BridgeFiles.Now();
            string path = BridgeFiles.Under(RequestDirectory(request.receipt.requestId), "receipt.json");
            string json = JsonUtility.ToJson(request);
            if (BridgeFiles.Utf8.GetByteCount(json) > BridgeFiles.MaximumStoredBytes)
                throw new InvalidDataException("Local receipt exceeds storage bound.");
            BridgeFiles.AtomicWrite(path, json);
            request.lastDurableJson = json;
            BridgeResponse status = Response("status", true, request.receipt.requestId);
            status.receipt = DurableReceipt(json);
            status.busy = true; // Only answered while the main thread does not respond.
            publishedStatus = new PublishedStatus { requestId = request.receipt.requestId, fingerprint = request.receipt.fingerprint,
                sessionId = request.receipt.sessionId, json = SerializeResponse(status) };
        }

        private static bool SaveTerminal(StoredBridgeRequest request)
        {
            try
            {
                Save(request);
                PendingTerminalSaves.Remove(request.receipt.requestId);
                MarkFinished(request.receipt.requestId);
                return true;
            }
            catch (IOException) { PendingTerminalSaves.Add(request.receipt.requestId); return false; }
            catch (UnauthorizedAccessException) { PendingTerminalSaves.Add(request.receipt.requestId); return false; }
            finally { if (active == request) active = null; }
        }

        private static bool Terminal(string stage) => stage == "completed" || stage == "cancelled" || stage == "failed" || stage == "interrupted";

        private static void RestoreRequests()
        {
            foreach (string folder in Directory.GetDirectories(requestsRoot))
            {
                string id = Path.GetFileName(folder);
                if (!BridgeFiles.IsUuid(id)) continue;
                try
                {
                    // A read-only check per request; a marker that is no longer empty records finished cleanup.
                    var marker = new FileInfo(Path.Combine(folder, FinishedMarker));
                    if (marker.Exists)
                    {
                        FinishedIds.Add(id);
                        if (marker.Length == 0) TryCleanupTerminal(id);
                        continue;
                    }
                    StoredBridgeRequest request = LoadRequest(id);
                    if (request == null) continue;
                    if (Terminal(request.receipt.stage))
                    {
                        // Receipts from older Bridges have no marker yet; later reloads skip parsing them.
                        MarkFinished(id);
                        FinishedIds.Add(id);
                        TryCleanupTerminal(request);
                        continue;
                    }
                    Requests[id] = request;
                    bool resumable = request.receipt.sessionId == registry.sessionId && request.receipt.staged
                        && (request.receipt.stage == "preparing" || request.receipt.stage == "waitingForUser" || request.receipt.stage == "refreshing" || request.receipt.stage == "compiling");
                    bool unconfirmed = request.receipt.stage == "preparing" || request.receipt.stage == "waitingForUser";
                    if (resumable && active == null && (registry.enabled || !unconfirmed))
                    {
                        active = request;
                        // Domain reload discards the prepared review/window. The update loop
                        // resumes preparation without relying on a one-shot delay callback.
                        if (request.receipt.stage == "waitingForUser")
                        {
                            request.receipt.stage = "preparing";
                            request.receipt.preparationPhase = null;
                            request.receipt.preparationWaitingForEditor = false;
                            request.receipt.preparationBytes = 0;
                            request.receipt.preparationEntries = 0;
                            Save(request);
                        }
                        // Older v1 receivers waited for compilation here. Resume only
                        // the import refresh/output checks with the existing checkpoint.
                        if (request.receipt.stage == "compiling")
                        {
                            request.receipt.stage = "refreshing";
                            Save(request);
                        }
                    }
                    else
                    {
                        request.receipt.stage = "interrupted";
                        request.receipt.error = Labels(request).Message("Editor restarted or staging was interrupted; no automatic replay.");
                        if (SaveTerminal(request)) TryCleanupTerminal(request);
                        ShowImportResult(request);
                    }
                }
                catch (Exception) { /* Existing corrupt receipt is never overwritten by a duplicate submit. */ }
            }
        }

        private static StoredBridgeRequest LoadRequest(string id)
        {
            try
            {
                string path = BridgeFiles.Under(RequestDirectory(id), "receipt.json");
                if (!File.Exists(path) || new FileInfo(path).Length > BridgeFiles.MaximumStoredBytes) return null;
                string durableJson = File.ReadAllText(path, BridgeFiles.Utf8);
                var request = JsonUtility.FromJson<StoredBridgeRequest>(durableJson);
                if (request?.receipt == null || request.version != 1 || request.receipt.requestId != id
                    || request.receipt.instanceId != registry.instanceId || !BridgeFiles.IsHash(request.receipt.fingerprint)) return null;
                request.lastDurableJson = durableJson;
                return request;
            }
            catch (Exception) { return null; }
        }

        // A finished receipt loads on first use: a same-ID query, or its result window after a reload.
        private static bool TryGetRequest(string id, out StoredBridgeRequest request)
        {
            if (Requests.TryGetValue(id ?? string.Empty, out request)) return true;
            if (string.IsNullOrEmpty(id) || !FinishedIds.Contains(id)) return false;
            request = LoadRequest(id);
            if (request == null) return false;
            Requests[id] = request;
            return true;
        }

        private static void MarkFinished(string requestId)
        {
            try
            {
                string marker = BridgeFiles.Under(RequestDirectory(requestId), FinishedMarker);
                if (!File.Exists(marker)) File.WriteAllBytes(marker, Array.Empty<byte>());
            }
            catch (IOException) { }
            catch (UnauthorizedAccessException) { }
        }

        private static BridgeResponse Response(string code, bool ok, string requestId = null, BridgeReceipt receipt = null)
            => new BridgeResponse { ok = ok, code = code, instanceId = registry.instanceId,
                sessionId = registry.sessionId, requestId = requestId, receipt = DurableReceipt(receipt),
                enabled = registry.enabled, busy = Busy };

        private static BridgeReceipt DurableReceipt(BridgeReceipt receipt)
        {
            if (receipt == null || !Requests.TryGetValue(receipt.requestId, out StoredBridgeRequest request)
                || string.IsNullOrEmpty(request.lastDurableJson)) return null;
            return DurableReceipt(request.lastDurableJson);
        }

        private static BridgeReceipt DurableReceipt(string durableJson)
        {
            // A failed disk commit must never expose a merely mutated staged=true receipt.
            var durable = JsonUtility.FromJson<StoredBridgeRequest>(durableJson);
            BridgeReceipt result = durable.receipt;
            // Preserve historical stored evidence while returning only import facts.
            result.compileErrors = new List<string>();
            if (!durable.appliedFailed && LegacyCompilationError(result.error)) result.error = null;
            if (result.stage == "compiling") result.stage = "refreshing";
            return result;
        }

        internal static bool LegacyCompilationError(string error)
            => error == "Files imported; Unity compilation reported errors."
                || error == "Files imported; Unity compilation/reload could not be confirmed. Applied files and backups remain.";

        private static string SerializeResponse(BridgeResponse response)
        {
            string json = JsonUtility.ToJson(response);
            return response.receipt == null ? json
                : json.Substring(0, json.Length - 1) + ",\"receipt\":" + JsonUtility.ToJson(response.receipt) + "}";
        }

        private static BridgeSubmission Reply(BridgeResponse response) => new BridgeSubmission { ResponseJson = SerializeResponse(response) };

        private static BridgeSubmission BeginCommand(string json)
        {
            BridgeCommand command;
            try { command = JsonUtility.FromJson<BridgeCommand>(json); }
            catch (Exception) { return Reply(Response("invalidRequest", false)); }
            if (command == null || command.version != 1) return Reply(Response("versionMismatch", false));
            if (!BridgeFiles.EqualCapability(command.capability, registry.capability)) return Reply(Response("unauthorized", false));
            if (command.instanceId != registry.instanceId) return Reply(Response("invalidRequest", false));
            if (command.command == "hello")
            {
                if (command.sessionId != registry.sessionId) return Reply(Response("invalidRequest", false));
                BridgeResponse hello = Response(registry.enabled ? "ready" : "disabled", registry.enabled);
                hello.projectName = registry.projectName;
                hello.projectRoot = registry.projectRoot;
                hello.unityVersion = registry.unityVersion;
                hello.bridgeVersion = registry.bridgeVersion;
                return Reply(hello);
            }
            if (command.command == "nativeImport") return BeginNativeImport(command);
            if (command.command != "submit" && command.command != "status" && command.command != "cancel")
                return Reply(Response("invalidRequest", false));
            if (!BridgeFiles.IsUuid(command.requestId) || !BridgeFiles.IsHash(command.fingerprint))
                return Reply(Response("invalidRequest", false));
            if (TryGetRequest(command.requestId, out StoredBridgeRequest existing))
            {
                if (existing.receipt.fingerprint != command.fingerprint) return Reply(Response("fingerprintMismatch", false, command.requestId));
                if (command.sessionId != existing.receipt.sessionId && command.sessionId != registry.sessionId)
                    return Reply(Response("invalidRequest", false, command.requestId));
                if (command.command == "cancel" && !existing.receipt.writesStarted && !Terminal(existing.receipt.stage)) CancelRequest(existing);
                return Reply(Response("status", true, command.requestId, existing.receipt));
            }
            if (command.command != "submit") return Reply(Response("notFound", false, command.requestId));
            if (Directory.Exists(RequestDirectory(command.requestId))) return Reply(Response("failed", false, command.requestId));
            if (!registry.enabled) return Reply(Response("disabled", false, command.requestId));
            if (Busy) return Reply(Response("busy", false, command.requestId));
            if (command.sessionId != registry.sessionId) return Reply(Response("invalidRequest", false, command.requestId));
            try
            {
                BridgeManifest manifest = ValidateManifest(command);
                var request = new StoredBridgeRequest {
                    manifestJson = command.manifestJson,
                    receipt = new BridgeReceipt { requestId = command.requestId, fingerprint = command.fingerprint,
                        instanceId = registry.instanceId, sessionId = registry.sessionId, stage = "staging" },
                    expectedBytes = manifest.entries.Sum(entry => entry.bytes)
                };
                Save(request);
                active = request;
                Requests.Add(command.requestId, request);
                Publish();
                return new BridgeSubmission { ResponseJson = SerializeResponse(Response("accepted", true, command.requestId, request.receipt)),
                    Request = request, Manifest = manifest, InputsDirectory = InputsDirectory(request) };
            }
            catch (Exception)
            {
                return Reply(Response("invalidRequest", false, command.requestId));
            }
        }

        // One package the App selected alone opens in Unity's own Import Unity Package
        // window, which reads the file in place and asks for confirmation itself. There is
        // no request, receipt or transfer; anything this refuses uses submit instead.
        private static BridgeSubmission BeginNativeImport(BridgeCommand command)
        {
            if (command.sessionId != registry.sessionId) return Reply(Response("invalidRequest", false));
            if (!registry.enabled) return Reply(Response("disabled", false));
            if (Busy) return Reply(Response("busy", false));
            string package = command.path;
            try
            {
                // Unity's importer cannot decompress a package whose file name is not ASCII
                // ("Couldn't decompress package"), and may not open a path past MAX_PATH.
                if (string.IsNullOrEmpty(package) || package.Length >= 260 || !Path.IsPathRooted(package)
                    || !package.EndsWith(".unitypackage", StringComparison.OrdinalIgnoreCase)
                    || Path.GetFileName(package).Any(c => c < ' ' || c > '~'))
                    return Reply(Response("invalidRequest", false));
                if (!File.Exists(package)) return Reply(Response("notFound", false));
                var header = new byte[1024];
                int length = 0;
                using (var stream = new FileStream(package, FileMode.Open, FileAccess.Read, FileShare.Read))
                    for (int read; length < header.Length && (read = stream.Read(header, length, header.Length - length)) > 0;) length += read;
                // Older Apps then send the package through submit, as the current App does itself.
                if (!NativeImportReads(header, length)) return Reply(Response("invalidRequest", false));
            }
            catch (Exception) { return Reply(Response("invalidRequest", false)); }
            // Runs next in this main-thread queue while the listener thread sends the reply.
            MainActions.Enqueue(() =>
            {
                // Obsolete from Unity 6.6, whose replacement (AssetPackage.Package.Import) 2022.3 lacks.
#pragma warning disable 618
                try { AssetDatabase.ImportPackage(package, true); }
#pragma warning restore 618
                catch (Exception error)
                {
                    UnityEngine.Debug.LogError("NekoNekoAsset Bridge could not open Unity's package import (" + error.GetType().Name + ").");
                }
            });
            return Reply(Response("accepted", true));
        }

        // The App's rule: Unity 2022.3 imports nothing, without an error, from a package whose gzip
        // header names a file with an extension other than .tar. Unity's own exporter names archtemp.tar.
        internal static bool NativeImportReads(byte[] header, int length)
        {
            if (length < 4 || header[0] != 0x1f || header[1] != 0x8b || header[2] != 8) return false;
            if ((header[3] & 0x08) == 0) return length >= 10;
            int start = 10;
            if ((header[3] & 0x04) != 0)
            {
                if (length < 12) return false;
                start = 12 + (header[10] | header[11] << 8);
            }
            int end = start < length ? Array.IndexOf(header, (byte)0, start, length - start) : -1;
            if (end < 0) return false;
            var name = new char[end - start];
            for (int i = start; i < end; i++) name[i - start] = (char)header[i];
            string file = new string(name);
            file = file.Substring(Math.Max(file.LastIndexOf('/'), file.LastIndexOf('\\')) + 1);
            return file.EndsWith(".tar", StringComparison.OrdinalIgnoreCase) || file.IndexOf('.') < 0;
        }

        private static BridgeManifest ValidateManifest(BridgeCommand command)
        {
            if (string.IsNullOrEmpty(command.manifestJson)
                || BridgeFiles.Hash(BridgeFiles.Utf8.GetBytes(command.manifestJson)) != command.fingerprint)
                throw new InvalidDataException("Manifest fingerprint mismatch.");
            var manifest = JsonUtility.FromJson<BridgeManifest>(command.manifestJson);
            if (manifest == null || manifest.version != 1 || manifest.instanceId != registry.instanceId
                || manifest.sessionId != registry.sessionId || (!string.IsNullOrEmpty(manifest.locale)
                    && manifest.locale != "en" && manifest.locale != "ja" && manifest.locale != "ko"
                    && manifest.locale != "zh-Hans" && manifest.locale != "zh-Hant") || manifest.entries == null || manifest.entries.Count == 0)
                throw new InvalidDataException("Invalid manifest.");
            var ids = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
            long total = 0;
            foreach (BridgeEntry entry in manifest.entries)
            {
                if (entry == null || !BridgeFiles.IsUuid(entry.entryId) || !ids.Add(entry.entryId)
                    || (entry.kind != "unitypackage" && entry.kind != "file") || entry.bytes < 0 || !BridgeFiles.IsHash(entry.sha256)
                    || string.IsNullOrEmpty(entry.displayName) || entry.displayName.Length > 1024
                    || string.IsNullOrEmpty(entry.productId) || string.IsNullOrEmpty(entry.fileId) || string.IsNullOrEmpty(entry.revision)
                    || entry.productId.Length > 256 || entry.fileId.Length > 256 || entry.revision.Length > 1024)
                    throw new InvalidDataException("Invalid selected file.");
                BridgeFiles.SafeRelative(entry.relativePath);
                if (!string.IsNullOrEmpty(entry.memberPath)) BridgeFiles.SafeRelative(entry.memberPath);
                total = checked(total + entry.bytes);
            }
            var drive = new DriveInfo(Path.GetPathRoot(requestsRoot));
            if (drive.IsReady && total > drive.AvailableFreeSpace) throw new IOException("Insufficient staging space.");
            return manifest;
        }

        private static string FinishTransfer(BridgeSubmission submission, bool success, string error)
        {
            StoredBridgeRequest request = submission.Request;
            if (request.receipt.stage == "cancelled")
            {
                TryCleanupTerminal(request);
                return SerializeResponse(Response("cancelled", true, request.receipt.requestId, request.receipt));
            }
            if (!success)
            {
                Fail(request, Labels(request).Message(error));
                return SerializeResponse(Response("failed", false, request.receipt.requestId, request.receipt));
            }
            request.receipt.staged = true;
            request.receipt.stage = "preparing";
            request.receipt.preparationPhase = null;
            request.receipt.preparationWaitingForEditor = false;
            request.receipt.preparationTotalBytes = submission.Manifest.entries.Sum(entry => entry.bytes);
            request.receipt.preparationTotalEntries = submission.Manifest.entries.Count;
            Save(request);
            return SerializeResponse(Response("staged", true, request.receipt.requestId, request.receipt));
        }

        private static void CancelRequest(StoredBridgeRequest request)
        {
            if (request.receipt.writesStarted || Terminal(request.receipt.stage)) return;
            request.cancelTransfer = true;
            request.preparedPackages = null;
            request.preparedSnapshot = null;
            request.preparedPreview = null;
            request.receipt.stage = "cancelled";
            EndProgress(request);
            bool durable = SaveTerminal(request);
            server?.CancelTransfer(request.receipt.requestId);
            if (review != null && active == null)
            {
                active = null;
                var pending = review;
                review = null;
                pending?.CancelPending();
            }
            if (durable && request.receipt.staged) TryCleanupTerminal(request);
            Publish();
        }

        private static void Fail(StoredBridgeRequest request, string message)
        {
            request.preparedPackages = null;
            request.preparedSnapshot = null;
            request.preparedPreview = null;
            request.receipt.error = BridgeFiles.Diagnostic(message, 256);
            request.receipt.stage = "failed";
            EndProgress(request);
            bool durable = SaveTerminal(request);
            // The receiver has closed its payload handles; preparation failures arrive
            // after the worker exits. Retain only durable dedupe and any applied-write recovery.
            if (durable && !request.preparingReview) TryCleanupTerminal(request);
            if (durable) ShowImportResult(request);
            Publish();
        }

        private static void Shutdown()
        {
            if (stopped) return;
            stopped = true;
            heartbeat?.Dispose();
            heartbeat = null;
            // Stop owned preparation promptly; its durable pre-write state can resume after domain reload.
            foreach (StoredBridgeRequest request in Requests.Values)
            {
                if (request.preparingReview) request.cancelTransfer = true;
                EndProgress(request);
            }
            // A utility window survives hot reload while its delegates/packages do not.
            // Retire this window; the durable waiting receipt opens a fresh review later.
            var pendingReview = review;
            review = null;
            if (pendingReview != null)
            {
                pendingReview.CancelPending();
                if (pendingReview != null) pendingReview.Close();
            }
            ImportConflictWindow.CancelActive();
            server?.Dispose();
            while (MainActions.TryDequeue(out Action pending)) pending();
            EditorApplication.update -= InitializeWhenReady;
            EditorApplication.update -= Update;
            try { Publish(); } catch (Exception) { }
            // Do not cancel applied work on domain reload. The durable receipt resumes observation.
        }
    }
}
