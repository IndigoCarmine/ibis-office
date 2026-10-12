# ibis-bench（Windows）

Windows Graphics Captureによるローカルの画面収録、フレーム解析、PDF変換の計測をまとめるRust CLI。4製品で共通のツールを使う。2026-10-02に42件（起動6、初回表示12、末尾12、PDF12）の正式計測・通常終了・画像レビューを完了した。結果は `tasks/benchmark/benchmark.md`。

リポジトリのルートからreleaseビルドする。

```powershell
cargo build --release --offline -p ibis-bench
target\release\ibis-bench.exe doctor
target\release\ibis-bench.exe run ibisoffice firstview pptx
target\release\ibis-bench.exe run ibisoffice end docx
target\release\ibis-bench.exe pdf ibisoffice
target\release\ibis-bench.exe pdf msoffice
target\release\ibis-bench.exe pdf libreoffice
target\release\ibis-bench.exe suite --plan
target\release\ibis-bench.exe suite ibisoffice pdf
target\release\ibis-bench.exe report
```

`run`は4製品の起動・静止待ち・ファイルを開く操作・計測・通常終了を管理する。既存の対象アプリやRustビルドがあれば停止し、ユーザーのプロセスは閉じない。ONLYOFFICEは起動用DesktopEditors.exeと実際のeditors.exeを区別する。`measure <hwnd> <app> <firstview|end> <kind>`は開いている確認用ウィンドウを使う開発用のコマンド。入力は毎回対象プロセスのフォーカスを確認する。XLSX末尾はibisOfficeのCtrl+G、ExcelのF5、LibreOffice・ONLYOFFICEの名前ボックスを接続済み。ExcelはUI AutomationでR213を設定・読み返す。LibreOfficeは初期値A1のEdit、ONLYOFFICEはID ce-cell-nameのEditを所有PID・位置・非遮蔽も確認して選び、A193:R213／A203をキーボードで入力して読み返す。いずれも画像による位置の確認が必要。

PowerPointはActivePresentationのパスとスライド数をCOMで確認するまで読み込み完了としない。公開pptxで発生する「埋め込みフォントのインストール」通知は、同じプロセスのUI Automationで「フォントのインストール失敗」と唯一のOKボタンを確認できる場合だけ閉じ、時刻をnav_logに記録する。フォントのインストール操作は行わない。通知と読み込みの時間は計測に含む。別の通知や手動介入を含む試行は正式値に使わない。

Officeの通常終了はCOMを優先する。ROT未登録（0x800401E3）の場合だけ、起動したPIDとウィンドウが一致することを再確認し、通常のWM_CLOSEを送る。保存通知の承認や強制終了は行わず、プロセスが10秒以内に終了しない場合は正式suite失敗とする。

`startup <app>`は起動前から画面全体を収録する。ウィンドウの収録と異なり、他のアプリが写る可能性があるため、そのPNG・格子はローカルのgit対象外ディレクトリにだけ保存する。

起動したプロセスの大きなウィンドウが1個だけ見つかった時点で、位置20,20・物理1440×900に揃えて読み返す。この移動・サイズ変更の時間も起動計測に含む。`review-before-end.png`・`review-end.png`・`review-last.png`は同じ収録フレームからその矩形だけを切り出した確認用画像。画面全体のPNGは比較資料へ転載しない。

LibreOfficeの「今日のヒント:」は同一PIDのダイアログ内の唯一のOKだけをUIAで確認して閉じる。保存確認では計測コピーのファイル名を含む厳密な文言と唯一の「保存しない(N)」を確認して閉じる。文書本文の全要素を読み出さず、ダイアログのサブツリーだけを調べる。設定やヒント表示のチェックボックスは変更しない。

`suite [app] [startup|firstview|end|pdf] [--plan]`は条件に合う計測を順番に実行する。引数なしで42件（起動6、ファーストビュー12、末尾12、PDF12）。`--plan`は一覧を表示するだけで起動・入力しない。各コマンドの終了、結果ファイルの更新、ビルドがないことを確認し、失敗で停止する。ログと以前のJSONの控えは`target/benchmark-windows/suite-<Unix秒>/`に残す。環境情報は個人・端末名・シリアルを含めず採取する。

`report`は`benchmark.md`のWindowsの印の間だけを更新する。既存のMacの表・手書きの注記を保持する。`formal=true`・`suite_exit_ok=true`・`reviewed=true`をすべて満たす結果だけを掲載する。`reviewed`は自動でtrueにしない。終了前後のPNG、末尾位置、PDF生成とページ数を確認した後に、理由とともに結果JSONに記録する。検証値・未確認値は「計測・確認待ち」と表示する。

`pdf-review pdf-<app>-<kind>`は、その結果JSONのPDFを開き、先頭・末尾のページを作業ディレクトリの `review-first.png` と `review-last.png` にレンダリングする。ページ数と画像の確認を補助するが、reviewedを自動でtrueにしない。

`windows`でウィンドウ一覧、`record <hwnd> <seconds> <directory>`で指定ウィンドウだけの収録、`analyze <samples.json> <t0> <startup|docx|xlsx|pptx|end-pptx>`で保存済み格子の再解析ができる。

動画ファイルは作らず、フレームのQPC時刻と192×120格子を保存する。各格子は4×4点のBGRA輝度平均で、輝度差24超のます目が0.2%超変わったフレームを変化とする。文書領域の最後の変化が完了時刻。変更なし・上限超過は成功としない。確認用のbaseline・before-end・end・lastのPNGを保存する。APIの空キュー（S_OK/null）はフレーム待ちとして扱う。

JSONは`tasks/benchmark/runs-windows/`、PNG・格子・変換ログ・PDF・入力コピーはgit対象外の`target/benchmark-windows/`に残す。原本は`test/benchmark/`の公開データのみ。OSの表示倍率・セキュリティ設定・PowerShellの実行ポリシーは変更しない。DPI Awarenessはツールのプロセスだけに設定する。ネットワークへの送信やpushは行わない。

ファーストビューの現実装は、起動済みアプリの「開く」ダイアログにコピーのパスをUI Automationで設定・読み返した後、Openを送る直前をt0にする。Macの`open -a`とは開始操作が異なるため正式レポートに記載する。MS Office PDFはRustからCOMを直接呼び、作成したインスタンスの文書だけを保存せずに閉じて終了する。LibreOfficeは作業ディレクトリ内の専用プロフィールを使う。変換は開始からプロセス終了まで測り、ページ数はWindows.Data.Pdfで取得する。

XLSXはMacと同じ中央の格子点でRGBの最大値−最小値が60を超える点を数え、0.5%超まで待つ。末尾ジャンプは、色が付いたらすぐ（静止を待たず）送る（Macの nav-still 0 と同じ）。CPUは対象ウィンドウのプロセスのみ（ヘルパーは除外）を記録する。

ONLYOFFICEの表計算では名前ボックスのUIA ID `ce-cell-name`を実機で確認した。UIAのSetValueのみではアプリの移動が確定しなかったため、所有ウィンドウ内の名前ボックス位置を確認してキーボードでA203を入力し、値を読み返してEnterを送る。プレゼンテーションはUIAの「スライド 1/31」を待ち、読み込み中の静止画を除外する。LibreOfficeは最後のスライドのキャンバスを確認した後にEnd入力を止める。LibreOfficeの名前ボックスはName A1とEdit型の両条件で特定する。これらは正式suite・前後画像で検証済み。

検証範囲：各1回の採用値。初回表示は総ページ数の確定より早い場合があり、末尾のグラフは画面の右端・下端で一部切れる。PDFは先頭・末尾の確認で、全ページの品質比較ではない。XLSXのPDF出力範囲は製品間で一致していない。初期24件は環境JSONのsuite内複製前で、当該回の環境スナップショットは残っていない。最新環境・各回の終了ログ・結果・確認画像は保持する。検証中や失敗した試行は正式値に含めない。
