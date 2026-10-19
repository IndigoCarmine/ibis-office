# Reporting Security Issues

[English](#reporting-security-issues) | [日本語](#セキュリティの問題の報告日本語)

If you find a security issue (vulnerability) in ibisOffice, **do not post it in a public issue or pull request**. Please report it privately through one of the following:

1. **GitHub private vulnerability reporting** (recommended): use "Report a vulnerability" on this repository's [Security](https://github.com/ibisjp/ibis-office/security) tab.
2. **ibis inc. contact form**: <https://ssl.ibis.ne.jp/support/Entry?svid=30> (please put "ibisOffice security" in the subject)

## What to include

- What the issue is, and its possible impact
- Steps to reproduce (and the file that triggers it, if any)
- The ibisOffice version, and your OS or browser and its version

Please make sure the file that triggers the issue contains no personal or confidential information.

## Scope

- The latest release, and the latest code on `main`
- Issues triggered by opening document files (.docx, .xlsx, .pptx, .docm, .xlsm, .pptm and so on): arbitrary code execution, memory corruption, reading or writing files outside the app, files that crash the app, and the like
- In the web edition, issues that affect other sites or other users' data

For issues in components we depend on (such as GPUI), please also notify their authors.

## How we respond

- We will acknowledge your report and look into it.
- Please do not disclose the issue until a release with the fix is out.
- When we publish the fix, we will credit you by name (unless you prefer not to be named).

## About macros

ibisOffice never runs macros in macro-enabled files (.docm, .xlsm, .pptm). It only keeps them as they are when opening and saving.

---

# セキュリティの問題の報告（日本語）

ibisOffice のセキュリティの問題（脆弱性）を見つけたときは、**公開の Issue やプル リクエストには書かず**、次のどちらかで非公開で知らせてください。

1. **GitHub の非公開の報告**（おすすめ）：このリポジトリの [Security](https://github.com/ibisjp/ibis-office/security) タブの「Report a vulnerability」から送ってください。
2. **株式会社アイビスのお問い合わせ窓口**：<https://ssl.ibis.ne.jp/support/Entry?svid=30>（件名に「ibisOffice セキュリティ」と書いてください）

## 報告に書いてほしいこと

- 問題の内容と、起こりうる影響
- 再現の手順（問題の起きるファイルがあれば、それも）
- ibisOffice の版と、OS・ブラウザの種類と版

問題の起きるファイルに、個人の情報や秘密の情報が入らないように気をつけてください。

## 対象

- 最新のリリースと、`main` の最新のコード
- 文書ファイル（.docx・.xlsx・.pptx・.docm・.xlsm・.pptm など）を開いたときに起きる問題（任意のコードの実行、メモリの破壊、アプリの外のファイルの読み書き、強制終了を引き起こすファイルなど）
- Web 版で、ほかのサイトや利用者のデータに影響する問題

依存している部品（GPUI など）の問題は、その部品の作者にも知らせてください。

## 対応

- 報告を受けたら、受け取ったことを知らせ、内容を確かめます。
- 修正したリリースを出すまで、問題の内容は公開しないでください。
- 修正を公開するときに、報告者のお名前を記載します（希望されない場合は記載しません）。

## マクロについて

ibisOffice は、マクロ付きのファイル（.docm・.xlsm・.pptm）のマクロを実行しません。開いて保存するときに、マクロをそのまま残すだけです。
