using System;
using System.Collections.Generic;

namespace NekoNekoAsset.Bridge
{
    /// <summary>Labels for the native review UI; the receiver selects its system/editor locale.</summary>
    public sealed class BridgeLocalization
    {
        private readonly int language;

        public BridgeLocalization(string locale)
        {
            switch (locale)
            {
                case "ja": language = 1; break;
                case "ko": language = 2; break;
                case "zh-Hans": language = 3; break;
                case "zh-Hant": language = 4; break;
                default: language = 0; break;
            }
        }

        public string Text(string key)
        {
            return Labels.TryGetValue(key, out string[] labels) ? labels[language] : key;
        }

        public string Format(string key, params object[] values)
        {
            return string.Format(Text(key), values);
        }

        // Diagnostics are written in English; receipts and windows show them in this language.
        // A row ending in a space keeps what follows it (a path, GUID or nested reason).
        public string Message(string text)
        {
            if (string.IsNullOrEmpty(text) || language == 0) return text;
            string[] row = Row(text);
            if (row == null) return text;
            return row[0].EndsWith(" ", StringComparison.Ordinal) ? row[language] + Message(text.Substring(row[0].Length)) : row[language];
        }

        // An exception's text: a known diagnostic, or the system's own words (an OS IOException)
        // after a sentence in this language.
        public string Reason(string text)
            => string.IsNullOrEmpty(text) || Row(text) != null ? Message(text) : Text("fileError") + text;

        private static string[] Row(string text)
        {
            foreach (string[] row in Messages)
                if (row[0].EndsWith(" ", StringComparison.Ordinal) ? text.StartsWith(row[0], StringComparison.Ordinal) : text == row[0]) return row;
            return null;
        }

        internal string Kind(SmartPackage.ImportConflictKind kind)
        {
            switch (kind)
            {
                case SmartPackage.ImportConflictKind.New: return Text("new");
                case SmartPackage.ImportConflictKind.Duplicate: return Text("identical");
                case SmartPackage.ImportConflictKind.Update: return Text("update");
                case SmartPackage.ImportConflictKind.GuidConflict: return Text("guidConflict");
                case SmartPackage.ImportConflictKind.Blocked: return Text("blocked");
                default: return Text("pathTaken");
            }
        }

        private static readonly Dictionary<string, string[]> Labels = new Dictionary<string, string[]>(StringComparer.Ordinal)
        {
            ["resultTitle"] = new[] { "Import results", "インポート結果", "가져오기 결과", "导入结果", "匯入結果" },
            ["failed"] = new[] { "Failed", "失敗", "실패", "失败", "失敗" },
            ["close"] = new[] { "Close", "閉じる", "닫기", "关闭", "關閉" },
            ["resultCounts"] = new[] { "Imported: {0}  Skipped: {1}  Failed: {2}", "インポート: {0} 件  スキップ: {1} 件  失敗: {2} 件", "가져옴: {0}  건너뜀: {1}  실패: {2}", "已导入: {0}  已跳过: {1}  失败: {2}", "已匯入: {0}  已略過: {1}  失敗: {2}" },
            ["partlyImported"] = new[] { "Partly imported", "一部のみインポート", "일부만 가져옴", "部分已导入", "部分已匯入" },
            ["showInProject"] = new[] { "Show in Project", "プロジェクトで表示", "프로젝트에서 보기", "在项目中显示", "在專案中顯示" },
            ["receiving"] = new[] { "Receiving files", "ファイルを受信中", "파일 받는 중", "正在接收文件", "正在接收檔案" },
            ["verifying"] = new[] { "Verifying files", "ファイルを検証中", "파일 확인 중", "正在验证文件", "正在驗證檔案" },
            ["readingPackage"] = new[] { "Reading package", "パッケージを読み込み中", "패키지 읽는 중", "正在读取包", "正在讀取套件" },
            ["waitingForEditor"] = new[] { "Waiting for Unity", "Unityの準備ができるまで待機中", "Unity를 사용할 수 있을 때까지 대기 중", "等待 Unity 就绪", "等待 Unity 就緒" },
            ["cleanup"] = new[] { "Clean up new files", "今回追加したファイルを削除", "새 파일 정리", "清理新文件", "清理新增檔案" },
            ["cleanupConfirm"] = new[] { "Remove this import's unchanged new files? Preexisting and changed files are kept.", "このインポートで追加されたファイルのうち、変更されていないものを削除しますか？既存のファイルと変更済みのファイルは残ります。", "이번에 추가된 변경되지 않은 파일을 삭제할까요? 기존 파일과 수정된 파일은 유지됩니다.", "删除本次导入后未修改的新文件？保留原有和已修改的文件。", "刪除本次匯入後未修改的新增檔案？保留原有與已修改的檔案。" },
            ["cleanupCounts"] = new[] { "Removed: {0}  Retained: {1}", "削除 {0} 件、残したファイル {1} 件", "삭제: {0}  유지: {1}", "已删除: {0}  已保留: {1}", "已刪除: {0}  已保留: {1}" },
            ["title"] = new[] { "NekoNekoAsset Import", "アセットのインポート (NekoNekoAsset)", "NekoNekoAsset 가져오기", "NekoNekoAsset 导入", "NekoNekoAsset 匯入" },
            ["importing"] = new[] { "Importing", "インポート中", "가져오는 중", "正在导入", "正在匯入" },
            ["checkingTargets"] = new[] { "Checking targets…", "インポート先を確認中…", "대상 확인 중…", "正在检查目标…", "正在檢查目標…" },
            ["import"] = new[] { "Import", "インポート", "가져오기", "导入", "匯入" },
            ["cancel"] = new[] { "Cancel", "キャンセル", "취소", "取消", "取消" },
            ["selectAll"] = new[] { "Select All", "すべて選択", "모두 선택", "全选", "全選" },
            ["selectNone"] = new[] { "Select None", "すべての選択を解除", "선택 해제", "全不选", "全不選" },
            ["expandAll"] = new[] { "Expand All", "すべて展開", "모두 펼치기", "全部展开", "全部展開" },
            ["foldAll"] = new[] { "Collapse All", "すべて折りたたむ", "모두 접기", "全部折叠", "全部摺疊" },
            ["conflictPolicy"] = new[] { "Conflicts", "競合時の処理", "충돌", "冲突", "衝突" },
            ["ask"] = new[] { "Ask", "競合ごとに確認", "확인", "询问", "詢問" },
            ["overwrite"] = new[] { "Overwrite", "上書き", "덮어쓰기", "覆盖", "覆寫" },
            ["skip"] = new[] { "Skip", "スキップ", "건너뛰기", "跳过", "略過" },
            ["overwriteAll"] = new[] { "Overwrite All", "すべて上書き", "모두 덮어쓰기", "全部覆盖", "全部覆寫" },
            ["skipAll"] = new[] { "Skip All", "すべてスキップ", "모두 건너뛰기", "全部跳过", "全部略過" },
            ["new"] = new[] { "New", "新規", "새 파일", "新增", "新增" },
            ["identical"] = new[] { "Identical", "同一ファイル", "동일", "相同", "相同" },
            ["update"] = new[] { "Update", "更新", "업데이트", "更新", "更新" },
            ["guidConflict"] = new[] { "GUID conflict", "GUID の競合", "GUID 충돌", "GUID 冲突", "GUID 衝突" },
            ["pathTaken"] = new[] { "Path taken", "パス競合", "경로 사용 중", "路径已占用", "路徑已占用" },
            ["blocked"] = new[] { "Blocked", "インポート不可", "가져올 수 없음", "无法导入", "無法匯入" },
            ["blockedCount"] = new[] { "{0} Blocked", "インポート不可 {0}件", "가져올 수 없음 {0}", "{0} 无法导入", "{0} 無法匯入" },
            ["updateCount"] = new[] { "{0} Update", "更新 {0}件", "업데이트 {0}", "{0} 更新", "{0} 更新" },
            ["guidConflictCount"] = new[] { "{0} GUID conflict", "GUID競合 {0}件", "GUID 충돌 {0}", "{0} GUID 冲突", "{0} GUID 衝突" },
            ["pathTakenCount"] = new[] { "{0} Path taken", "パス競合 {0}件", "경로 사용 중 {0}", "{0} 路径已占用", "{0} 路徑已占用" },
            ["identicalCount"] = new[] { "{0} Identical", "同一ファイル {0} 件", "동일 {0}", "{0} 相同", "{0} 相同" },
            ["selectedCount"] = new[] { "{0} / {1} selected", "{0} / {1} 件を選択済み", "{0} / {1} 선택", "已选 {0} / {1}", "已選 {0} / {1}" },
            ["destination"] = new[] { "Destination", "インポート先", "대상 폴더", "目标路径", "目標路徑" },
            ["choose"] = new[] { "Choose…", "選択…", "선택…", "选择…", "選擇…" },
            ["chooseDestination"] = new[] { "Choose Assets folder", "Assets フォルダー内のインポート先を選択", "Assets 폴더 선택", "选择 Assets 文件夹", "選擇 Assets 資料夾" },
            ["invalidDestination"] = new[] { "Choose a path inside Assets.", "Assets フォルダー内のインポート先を選択してください。", "Assets 안의 경로를 선택하세요.", "请选择 Assets 内的路径。", "請選擇 Assets 內的路徑。" },
            ["unified"] = new[] { "Unified location", "共通のインポート先", "통합 위치", "统一位置", "統一位置" },
            ["individual"] = new[] { "Individual locations", "個別のインポート先", "개별 위치", "独立位置", "獨立位置" },
            ["assetPackages"] = new[] { ".unitypackage", "アセットパッケージ (.unitypackage)", ".unitypackage", ".unitypackage", ".unitypackage" },
            ["looseFiles"] = new[] { "Files", "ファイル", "파일", "文件", "檔案" },
            ["empty"] = new[] { "No files selected", "ファイルが選択されていません", "선택한 파일 없음", "未选择文件", "未選擇檔案" },
            ["fileError"] = new[] { "File operation failed: ", "ファイル操作に失敗しました: ", "파일 작업에 실패했습니다: ", "文件操作失败: ", "檔案操作失敗: " },
            ["invalidInput"] = new[] { "The import could not be prepared.", "インポートを準備できませんでした。", "가져오기를 준비할 수 없습니다.", "无法准备导入。", "無法準備匯入。" },
            ["planChanged"] = new[] { "The import targets changed. Review the updated selection.", "インポート先が変更されました。更新後の選択内容を確認してください。", "가져올 대상이 변경되었습니다. 선택을 다시 확인하세요.", "导入目标已改变，请检查更新后的选择。", "匯入目標已改變，請檢查更新後的選擇。" },
            ["receiptTooLarge"] = new[] { "The result exceeds the transfer limit. Select fewer files.", "インポート結果が転送できるサイズの上限を超えています。選択するファイルを減らしてください。", "결과가 전송 크기 제한을 초과합니다. 선택한 파일 수를 줄이세요.", "结果超过传输大小上限，请减少所选文件。", "結果超過傳輸大小上限，請減少選取的檔案。" },
            ["conflictTitle"] = new[] { "NekoNekoAsset Conflict", "インポートの競合 (NekoNekoAsset)", "NekoNekoAsset 충돌", "NekoNekoAsset 冲突", "NekoNekoAsset 衝突" },
            ["conflictHeading"] = new[] { "Import conflict", "インポートの競合", "가져오기 충돌", "导入冲突", "匯入衝突" },
            ["thisConflict"] = new[] { "This conflict", "この競合のみ", "이 충돌", "此冲突", "此衝突" },
            ["remainingConflicts"] = new[] { "This and all remaining conflicts", "この競合と残りの競合すべて", "이 충돌과 남은 모든 충돌", "此冲突及后续所有冲突", "此衝突及後續所有衝突" },
            ["pathManaged"] = new[] { "(path-managed)", "（ファイルパスで管理）", "(경로 관리)", "（按路径管理）", "（按路徑管理）" },
            ["none"] = new[] { "(none)", "(なし)", "(없음)", "（无）", "（無）" },
            ["conflictDetails"] = new[]
            {
                "Kind: {0}\nIncoming: {1}\nIncoming GUID: {2}\nTarget: {3}\nExisting GUID: {4}",
                "種類: {0}\nインポート元: {1}\nインポート元の GUID: {2}\nインポート先: {3}\n既存アセットの GUID: {4}",
                "종류: {0}\n받는 경로: {1}\n받는 GUID: {2}\n대상: {3}\n기존 GUID: {4}",
                "类型：{0}\n传入路径：{1}\n传入 GUID：{2}\n目标：{3}\n现有 GUID：{4}",
                "類型：{0}\n傳入路徑：{1}\n傳入 GUID：{2}\n目標：{3}\n現有 GUID：{4}"
            }
        };

        private static readonly string[][] Messages =
        {
            new[] { "Some files were not imported; inspect the per-file results.", "一部のファイルをインポートできませんでした。ファイルごとの結果を確認してください。", "일부 파일을 가져오지 못했습니다. 파일별 결과를 확인하세요.", "部分文件未能导入，请查看每个文件的结果。", "部分檔案未能匯入，請查看每個檔案的結果。" },
            new[] { "Import interrupted; imported files remain and backups are retained.", "インポートが中断されました。インポート済みのファイルとバックアップは残っています。", "가져오기가 중단되었습니다. 가져온 파일과 백업은 유지됩니다.", "导入已中断，已导入的文件和备份已保留。", "匯入已中斷，已匯入的檔案和備份已保留。" },
            new[] { "Not attempted after an earlier failure.", "前のファイルが失敗したため実行されませんでした。", "앞선 실패로 인해 시도하지 않았습니다.", "因先前的失败而未执行。", "因先前的失敗而未執行。" },
            new[] { "Could not refresh or verify the imported files; they and their backups remain.", "インポートしたファイルを更新・確認できませんでした。ファイルとバックアップは残っています。", "가져온 파일을 새로 고치거나 확인하지 못했습니다. 파일과 백업은 유지됩니다.", "无法刷新或验证导入的文件，文件和备份已保留。", "無法重新整理或驗證匯入的檔案，檔案和備份已保留。" },
            new[] { "Missing after Unity refreshed the project.", "Unityのプロジェクト更新後に見つかりません。", "Unity가 프로젝트를 새로 고친 후 찾을 수 없습니다.", "Unity 刷新项目后找不到此文件。", "Unity 重新整理專案後找不到此檔案。" },
            new[] { "Changed while Unity refreshed the project.", "Unityのプロジェクト更新中に変更されました。", "Unity가 프로젝트를 새로 고치는 동안 변경되었습니다.", "此文件在 Unity 刷新项目时被更改。", "此檔案在 Unity 重新整理專案時被變更。" },
            new[] { "Its GUID changed while Unity refreshed the project.", "Unityのプロジェクト更新中にGUIDが変更されました。", "Unity가 프로젝트를 새로 고치는 동안 GUID가 변경되었습니다.", "此文件的 GUID 在 Unity 刷新项目时被更改。", "此檔案的 GUID 在 Unity 重新整理專案時被變更。" },
            new[] { "File operation failed.", "ファイル操作に失敗しました。", "파일 작업에 실패했습니다.", "文件操作失败。", "檔案操作失敗。" },
            new[] { "Editor restarted or staging was interrupted; no automatic replay.", "エディターの再起動などで中断されました。自動ではやり直しません。", "에디터 재시작 등으로 중단되었습니다. 자동으로 다시 실행하지 않습니다.", "编辑器重启或暂存被中断，不会自动重试。", "編輯器重新啟動或暫存被中斷，不會自動重試。" },
            new[] { "Transfer interrupted or failed verification.", "転送が中断されたか、検証に失敗しました。", "전송이 중단되었거나 확인에 실패했습니다.", "传输中断或验证失败。", "傳輸中斷或驗證失敗。" },
            new[] { "Could not prepare verified package/file review.", "インポートを準備できませんでした。", "가져오기를 준비할 수 없습니다.", "无法准备导入。", "無法準備匯入。" },
            new[] { "The file could not be read.", "ファイルを読み込めませんでした。", "파일을 읽을 수 없습니다.", "无法读取文件。", "無法讀取檔案。" },
            new[] { "Not a valid .unitypackage: ", ".unitypackageとして読み込めません: ", "올바른 .unitypackage가 아닙니다: ", "不是有效的 .unitypackage: ", "不是有效的 .unitypackage: " },
            new[] { "Corrupted data.", "データが壊れています。", "데이터가 손상되었습니다.", "数据已损坏。", "資料已損毀。" },
            new[] { "Unity asset entry is missing asset.meta: ", "asset.metaがありません: ", "asset.meta가 없습니다: ", "缺少 asset.meta: ", "缺少 asset.meta: " },
            new[] { "Selected loose paths overlap; choose one version.", "選択したファイルのパスが重複しています。どちらか一方を選んでください。", "선택한 파일 경로가 겹칩니다. 한 버전만 선택하세요.", "所选文件路径重复，请只选择一个版本。", "所選檔案路徑重複，請只選擇一個版本。" },
            new[] { "An explicit meta file needs its selected asset.", ".metaファイルには対応するアセットも選択してください。", ".meta 파일은 해당 에셋도 함께 선택해야 합니다.", ".meta 文件需要同时选择对应的资源。", ".meta 檔案需要同時選擇對應的資源。" },
            new[] { "Verified staged input changed or is missing.", "受信したファイルが変更されたか、見つかりません。", "받은 파일이 변경되었거나 없습니다.", "已接收的文件已更改或缺失。", "已接收的檔案已變更或遺失。" },
            new[] { "Verified staged input changed during preparation.", "受信したファイルが変更されたか、見つかりません。", "받은 파일이 변경되었거나 없습니다.", "已接收的文件已更改或缺失。", "已接收的檔案已變更或遺失。" },
            new[] { "The target changed after confirmation: ", "確認後に保存先が変更されました: ", "확인 후 대상이 변경되었습니다: ", "确认后目标已更改: ", "確認後目標已變更: " },
            new[] { "The identical target changed after confirmation.", "確認後に同一だったファイルが変更されました。", "확인 후 동일했던 파일이 변경되었습니다.", "确认后原本相同的文件已更改。", "確認後原本相同的檔案已變更。" },
            new[] { "The committed target could not be verified.", "書き込んだファイルを確認できませんでした。", "쓴 파일을 확인할 수 없습니다.", "无法验证已写入的文件。", "無法驗證已寫入的檔案。" },
            new[] { "The staged asset differs from its reviewed payload.", "受信したファイルが確認時と異なります。", "받은 파일이 확인 시와 다릅니다.", "已接收的文件与确认时不同。", "已接收的檔案與確認時不同。" },
            new[] { "The staged meta differs from its reviewed payload.", "受信したファイルが確認時と異なります。", "받은 파일이 확인 시와 다릅니다.", "已接收的文件与确认时不同。", "已接收的檔案與確認時不同。" },
            new[] { "A parent changed after confirmation: ", "確認後に親フォルダーが変更されました: ", "확인 후 상위 폴더가 변경되었습니다: ", "确认后上级文件夹已更改: ", "確認後上層資料夾已變更: " },
            new[] { "The existing GUID target is not writable inside this project (it may come from PackageCache): ", "同じGUIDの既存アセットはこのプロジェクト内で書き込めません(PackageCacheの可能性があります): ", "같은 GUID의 기존 에셋은 이 프로젝트에서 쓸 수 없습니다(PackageCache일 수 있음): ", "相同 GUID 的现有资源无法在此项目中写入（可能来自 PackageCache）: ", "相同 GUID 的現有資源無法在此專案中寫入（可能來自 PackageCache）: " },
            new[] { "Import would replace a file with a folder or a folder with a file: ", "ファイルとフォルダーが入れ替わるためインポートできません: ", "파일과 폴더가 서로 바뀌므로 가져올 수 없습니다: ", "文件和文件夹类型冲突，无法导入: ", "檔案和資料夾類型衝突，無法匯入: " },
            new[] { "The GUID target changes between a file and a folder: ", "ファイルとフォルダーが入れ替わるためインポートできません: ", "파일과 폴더가 서로 바뀌므로 가져올 수 없습니다: ", "文件和文件夹类型冲突，无法导入: ", "檔案和資料夾類型衝突，無法匯入: " },
            new[] { "A file already occupies an ancestor of the import target: ", "保存先の親フォルダーの位置にファイルがあります: ", "대상의 상위 폴더 위치에 파일이 있습니다: ", "目标的上级文件夹位置已有文件: ", "目標的上層資料夾位置已有檔案: " },
            new[] { "A project file already occupies an ancestor of the import target: ", "保存先の親フォルダーの位置にファイルがあります: ", "대상의 상위 폴더 위치에 파일이 있습니다: ", "目标的上级文件夹位置已有文件: ", "目標的上層資料夾位置已有檔案: " },
            new[] { "The import target is already used as a directory by ", "保存先はすでにフォルダーとして使われています: ", "대상이 이미 폴더로 쓰이고 있습니다: ", "目标已被用作文件夹: ", "目標已被用作資料夾: " },
            new[] { "GUID target is not a writable project path: ", "同じGUIDの既存アセットの場所には書き込めません: ", "같은 GUID의 기존 에셋 위치에는 쓸 수 없습니다: ", "无法写入相同 GUID 的现有资源位置: ", "無法寫入相同 GUID 的現有資源位置: " },
            new[] { "A loose file GUID resolves outside Assets: ", "同じGUIDの既存アセットがAssetsの外にあります: ", "같은 GUID의 기존 에셋이 Assets 밖에 있습니다: ", "相同 GUID 的现有资源位于 Assets 之外: ", "相同 GUID 的現有資源位於 Assets 之外: " },
            new[] { "The package pathname is empty.", "パスが空です。", "경로가 비어 있습니다.", "路径为空。", "路徑為空。" },
            new[] { "Rooted package paths are not allowed: ", "絶対パスは使えません: ", "절대 경로는 사용할 수 없습니다: ", "不能使用绝对路径: ", "不能使用絕對路徑: " },
            new[] { "Import paths must start with Assets, Packages or ProjectSettings: ", "Assets、Packages、ProjectSettings以外にはインポートできません: ", "Assets, Packages, ProjectSettings 밖으로는 가져올 수 없습니다: ", "只能导入到 Assets、Packages 或 ProjectSettings: ", "只能匯入到 Assets、Packages 或 ProjectSettings: " },
            new[] { "Empty path segments are not allowed: ", "パスに空の部分があります: ", "경로에 빈 부분이 있습니다: ", "路径中有空的部分: ", "路徑中有空的部分: " },
            new[] { "Relative path segments are not allowed: ", "パスに「.」や「..」は使えません: ", "경로에 . 이나 .. 은 쓸 수 없습니다: ", "路径中不能有 . 或 .. 部分: ", "路徑中不能有 . 或 .. 部分: " },
            new[] { "Path segments may not end with a space or period: ", "フォルダー名やファイル名の末尾に空白やピリオドは使えません: ", "폴더나 파일 이름 끝에 공백이나 마침표는 쓸 수 없습니다: ", "文件夹名或文件名不能以空格或句点结尾: ", "資料夾名稱或檔案名稱不能以空格或句點結尾: " },
            new[] { "The package pathname contains invalid filename characters: ", "パスに使えない文字が含まれています: ", "경로에 사용할 수 없는 문자가 있습니다: ", "路径包含无效字符: ", "路徑包含無效字元: " },
            new[] { "The package pathname contains control characters.", "パスに制御文字が含まれています。", "경로에 제어 문자가 있습니다.", "路径包含控制字符。", "路徑包含控制字元。" },
            new[] { "The package pathname uses a reserved filename: ", "Windowsで使えないファイル名です: ", "Windows에서 사용할 수 없는 파일 이름입니다: ", "使用了 Windows 保留的文件名: ", "使用了 Windows 保留的檔案名稱: " },
            new[] { "A package pathname may not directly target a .meta file: ", ".metaファイルは直接インポートできません: ", ".meta 파일은 직접 가져올 수 없습니다: ", "不能直接导入 .meta 文件: ", "不能直接匯入 .meta 檔案: " },
            new[] { "The Unity project root is unavailable.", "Unityプロジェクトのフォルダーが見つかりません。", "Unity 프로젝트 폴더를 찾을 수 없습니다.", "找不到 Unity 项目文件夹。", "找不到 Unity 專案資料夾。" },
            new[] { "The package pathname resolves outside the Unity project: ", "パスがUnityプロジェクトの外を指しています: ", "경로가 Unity 프로젝트 밖을 가리킵니다: ", "路径指向 Unity 项目之外: ", "路徑指向 Unity 專案之外: " },
            new[] { "Symbolic links and junctions are not valid package targets: ", "シンボリックリンクやジャンクションにはインポートできません: ", "심볼릭 링크나 정션에는 가져올 수 없습니다: ", "不能导入到符号链接或联接点: ", "不能匯入到符號連結或連接點: " },
            new[] { "The staged replacement changed before commit.", "書き込む前に受信したファイルが変更されました。", "쓰기 전에 받은 파일이 변경되었습니다.", "写入前已接收的文件已更改。", "寫入前已接收的檔案已變更。" },
            new[] { "The staged package changed since review.", "確認後にパッケージが変更されました。", "확인 후 패키지가 변경되었습니다.", "确认后包已更改。", "確認後套件已變更。" },
            new[] { "The staged package changed while reading payloads.", "読み込み中にパッケージが変更されました。", "읽는 중에 패키지가 변경되었습니다.", "读取时包已更改。", "讀取時套件已變更。" },
            new[] { "Verified staged input size changed during preparation.", "受信したファイルが変更されたか、見つかりません。", "받은 파일이 변경되었거나 없습니다.", "已接收的文件已更改或缺失。", "已接收的檔案已變更或遺失。" },
            new[] { "Verified staged package changed during its metadata scan.", "受信したファイルが変更されたか、見つかりません。", "받은 파일이 변경되었거나 없습니다.", "已接收的文件已更改或缺失。", "已接收的檔案已變更或遺失。" },
            new[] { "The staged package changed during its metadata scan.", "受信したファイルが変更されたか、見つかりません。", "받은 파일이 변경되었거나 없습니다.", "已接收的文件已更改或缺失。", "已接收的檔案已變更或遺失。" },
            new[] { "A selected payload is missing from staging.", "受信したファイルが見つかりません。", "받은 파일을 찾을 수 없습니다.", "找不到已接收的文件。", "找不到已接收的檔案。" },
            new[] { "A loose file is missing its staged source.", "受信したファイルが見つかりません。", "받은 파일을 찾을 수 없습니다.", "找不到已接收的文件。", "找不到已接收的檔案。" },
            new[] { "The staged loose file is missing.", "受信したファイルが見つかりません。", "받은 파일을 찾을 수 없습니다.", "找不到已接收的文件。", "找不到已接收的檔案。" },
            new[] { "An asset payload is required.", "アセットのデータがありません。", "에셋 데이터가 없습니다.", "缺少资源数据。", "缺少資源資料。" },
            new[] { "The staged meta identity or asset shape changed.", "受信したファイルが確認時と異なります。", "받은 파일이 확인 시와 다릅니다.", "已接收的文件与确认时不同。", "已接收的檔案與確認時不同。" },
            new[] { "A new parent acquired unexpected metadata: ", "新しく作った親フォルダーに予期しない.metaがあります: ", "새로 만든 상위 폴더에 예기치 않은 .meta가 있습니다: ", "新建的上级文件夹出现了意外的 .meta: ", "新建的上層資料夾出現了意外的 .meta: " },
            new[] { "A file occupies an import parent: ", "親フォルダーの位置にファイルがあります: ", "상위 폴더 위치에 파일이 있습니다: ", "上级文件夹位置已有文件: ", "上層資料夾位置已有檔案: " },
            new[] { "A file occupies the incoming folder.", "フォルダーの位置にファイルがあります。", "폴더 위치에 파일이 있습니다.", "文件夹位置已有文件。", "資料夾位置已有檔案。" },
            new[] { "A directory occupies the incoming file.", "ファイルの位置にフォルダーがあります。", "파일 위치에 폴더가 있습니다.", "文件位置已有文件夹。", "檔案位置已有資料夾。" },
            new[] { "A directory occupies a file/meta target: ", "保存先の位置にフォルダーがあります: ", "대상 위치에 폴더가 있습니다: ", "目标位置已有文件夹: ", "目標位置已有資料夾: " },
            new[] { "Existing meta is too large.", "既存の.metaファイルが大きすぎます。", "기존 .meta 파일이 너무 큽니다.", "现有 .meta 文件过大。", "現有 .meta 檔案過大。" },
            new[] { "Linked paths are not allowed.", "シンボリックリンクやジャンクションは使えません。", "심볼릭 링크나 정션은 사용할 수 없습니다.", "不能使用符号链接或联接点。", "不能使用符號連結或連接點。" },
            new[] { "Path leaves its owned directory.", "作業フォルダーの外のパスは使えません。", "작업 폴더 밖의 경로는 사용할 수 없습니다.", "不能使用工作文件夹以外的路径。", "不能使用工作資料夾以外的路徑。" },
            new[] { "The trusted directory changed.", "作業フォルダーを確認できませんでした。", "작업 폴더를 확인할 수 없습니다.", "无法验证工作文件夹。", "無法驗證工作資料夾。" },
            new[] { "Could not verify the trusted directory.", "作業フォルダーを確認できませんでした。", "작업 폴더를 확인할 수 없습니다.", "无法验证工作文件夹。", "無法驗證工作資料夾。" },
            new[] { "Could not resolve the trusted directory.", "作業フォルダーを確認できませんでした。", "작업 폴더를 확인할 수 없습니다.", "无法验证工作文件夹。", "無法驗證工作資料夾。" },
            new[] { "TAR member is too large to buffer.", "パッケージ内のデータが大きすぎて読み込めません。", "패키지 안의 데이터가 너무 커서 읽을 수 없습니다.", "包内数据过大，无法读取。", "套件內資料過大，無法讀取。" },
            new[] { "TAR member exceeds the allowed size.", "パッケージ内のデータが大きすぎます。", "패키지 안의 데이터가 너무 큽니다.", "包内数据过大。", "套件內資料過大。" },
            new[] { "Truncated TAR header.", "パッケージが途中で切れています。", "패키지가 중간에 잘려 있습니다.", "包不完整。", "套件不完整。" },
            new[] { "Invalid octal value in TAR header.", "データが壊れています。", "데이터가 손상되었습니다.", "数据已损坏。", "資料已損毀。" },
            new[] { "Octal value overflows a TAR header field.", "データが壊れています。", "데이터가 손상되었습니다.", "数据已损坏。", "資料已損毀。" },
            new[] { "Duplicate pathname member for GUID ", "同じGUIDの項目が重複しています: ", "같은 GUID의 항목이 중복되었습니다: ", "相同 GUID 的条目重复: ", "相同 GUID 的項目重複: " },
            new[] { "Duplicate asset.meta member for GUID ", "同じGUIDの項目が重複しています: ", "같은 GUID의 항목이 중복되었습니다: ", "相同 GUID 的条目重复: ", "相同 GUID 的項目重複: " },
            new[] { "Duplicate asset member for GUID ", "同じGUIDの項目が重複しています: ", "같은 GUID의 항목이 중복되었습니다: ", "相同 GUID 的条目重复: ", "相同 GUID 的項目重複: " },
            new[] { "Duplicate selected TAR member for ", "同じGUIDの項目が重複しています: ", "같은 GUID의 항목이 중복되었습니다: ", "相同 GUID 的条目重复: ", "相同 GUID 的項目重複: " },
            new[] { "Invalid selected archive GUID.", "GUIDが正しくありません。", "GUID가 올바르지 않습니다.", "GUID 无效。", "GUID 無效。" },
            new[] { "Package entry is missing a pathname.", "パスのない項目があります。", "경로가 없는 항목이 있습니다.", "有条目缺少路径。", "有項目缺少路徑。" },
            new[] { "Entry has an asset.meta without a valid GUID: ", "asset.metaに有効なGUIDがありません: ", "asset.meta에 유효한 GUID가 없습니다: ", "asset.meta 缺少有效的 GUID: ", "asset.meta 缺少有效的 GUID: " },
            new[] { "Archive GUID does not match asset.meta GUID for ", "GUIDがasset.metaと一致しません: ", "GUID가 asset.meta와 일치하지 않습니다: ", "GUID 与 asset.meta 不一致: ", "GUID 與 asset.meta 不一致: " },
            new[] { "Case-colliding package paths: ", "大文字と小文字だけが異なるパスがあります: ", "대소문자만 다른 경로가 있습니다: ", "存在仅大小写不同的路径: ", "存在僅大小寫不同的路徑: " },
            new[] { "A package file is also used as a directory: ", "同じパスがファイルとフォルダーの両方に使われています: ", "같은 경로가 파일과 폴더에 모두 쓰였습니다: ", "同一路径同时用作文件和文件夹: ", "同一路徑同時用作檔案和資料夾: " },
            new[] { "A package pathname is both a file and a directory: ", "同じパスがファイルとフォルダーの両方に使われています: ", "같은 경로가 파일과 폴더에 모두 쓰였습니다: ", "同一路径同时用作文件和文件夹: ", "同一路徑同時用作檔案和資料夾: " },
            new[] { "Duplicate package pathname: ", "パスが重複しています: ", "경로가 중복되었습니다: ", "路径重复: ", "路徑重複: " },
            new[] { "The explicitly supplied meta is missing or too large.", ".metaファイルが見つからないか、大きすぎます。", ".meta 파일이 없거나 너무 큽니다.", ".meta 文件缺失或过大。", ".meta 檔案遺失或過大。" },
            new[] { "A loose file's explicit meta needs a valid file GUID.", ".metaファイルに有効なファイルのGUIDがありません。", ".meta 파일에 유효한 파일 GUID가 없습니다.", ".meta 文件缺少有效的文件 GUID。", ".meta 檔案缺少有效的檔案 GUID。" },
        };
    }
}
