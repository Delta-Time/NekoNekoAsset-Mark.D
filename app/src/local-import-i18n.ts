import {nativeWorkflowText} from './native-workflow-i18n';
import type {Locale} from './model';

const words:Record<string,string[]>={
 dropFiles:['Drag files or folders here','ファイル・フォルダーをここにドロップ','파일이나 폴더를 여기에 놓으세요','拖放文件或文件夹到这里','拖放檔案或資料夾到這裡'],
 fetch:['Fetch','情報を取得','가져오기','抓取','抓取'],

 singleProduct:['One asset','1つのアセットにまとめる','에셋 하나','一个资产','一個資產'],
 batchProducts:['Each folder / ZIP as an asset','フォルダー・ZIPごとに分ける','폴더 / ZIP별 에셋','每个文件夹 / ZIP 为一个资产','每個資料夾 / ZIP 為一個資產'],
 addFiles:['Add files','ファイルを追加','파일 추가','添加文件','新增檔案'],
 addFolder:['Add folder','フォルダーを追加','폴더 추가','添加文件夹','新增資料夾'],
 importQueued:['Import accepted','追加を受け付けました','가져오기를 접수했습니다','已接受导入请求','已接受匯入要求'],
 importQueuedCount:['Import accepted · {count} waiting','追加を受け付けました · 待機中 {count} 件','가져오기를 접수했습니다 · 대기 중 {count}건','已接受导入请求 · {count} 项待处理','已接受匯入要求 · {count} 項待處理'],
 openImportedAsset:['Open in library','ライブラリで開く','라이브러리에서 열기','在资产库中打开','在資產庫中開啟'],
 preparing:['Scanning files','ファイルを確認中','파일 확인 중','正在扫描文件','正在掃描檔案'],
 copying:['Copying files','ファイルをコピー中','파일 복사 중','正在复制文件','正在複製檔案'],
 verifying:['Verifying and saving files','ファイルを検証・保存中','파일 검증 및 저장 중','正在验证并保存文件','正在驗證及儲存檔案'],
 extracting:['Extracting archives','書庫を展開中','압축 해제 중','正在解压归档','正在解壓縮封存檔'],
 finalizing:['Saving to the library','ライブラリに登録中','라이브러리에 등록 중','正在添加到资产库','正在加入資產庫'],
 interrupted:['Import stopped before processing resumed','インポートが中断されました','가져오기가 중단되었습니다','导入已中断','匯入已中斷'],
 importSourcesMissing:['Choose local files or folders','ローカルのファイルまたはフォルダーを選択してください','로컬 파일 또는 폴더를 선택하세요','请选择本地文件或文件夹','請選擇本地檔案或資料夾'],
 no_import_files:['No files in the selected sources','選択した場所にファイルがありません','선택한 위치에 파일이 없습니다','所选来源中没有文件','所選來源中沒有檔案'],
 local_batch_requires_folder_or_zip:['Batch import accepts folders and ZIP files','この方法で追加できるのはフォルダーとZIPのみです','일괄 가져오기는 폴더와 ZIP 파일을 지원합니다','批量导入仅接受文件夹和 ZIP','批次匯入僅接受資料夾與 ZIP'],
 local_source_overlap:['Choose a folder or its contents, not both','フォルダーとその中身を重複して選択しないでください','폴더와 그 안의 파일을 중복 선택하지 마세요','请勿同时选择文件夹及其内容','請勿同時選擇資料夾與其內容'],
 local_source_contains_library:['The selected folder contains the asset library','選択したフォルダーにライブラリが含まれています','선택한 폴더에 에셋 라이브러리가 포함되어 있습니다','所选文件夹包含资产库','所選資料夾包含資產庫'],
 local_source_changed:['Source files changed; choose them again','元のファイルが変更されました。選択し直してください','원본 파일이 변경되었습니다. 다시 선택하세요','源文件已更改，请重新选择','來源檔案已變更，請重新選擇'],
 local_import_too_many_files:['Too many files in this import','追加するファイルが多すぎます','가져올 파일이 너무 많습니다','本次导入的文件过多','此次匯入的檔案過多'],
 local_draft_changed:['This import is already queued; start a new import for changes','この追加はすでに開始しています。変更する場合は、新しく追加し直してください','이미 대기열에 있는 가져오기입니다. 변경하려면 새로 시작하세요','此次导入已排队，更改内容请新建导入','此次匯入已排入佇列，變更內容請另建匯入'],
};
export function localImportText(locale:Locale,key:string):string {
 const index=({en:0,ja:1,ko:2,'zh-Hans':3,'zh-Hant':4} as Record<string,number>)[locale]??0;
 return words[key]?.[index]??nativeWorkflowText(locale,key);
}
