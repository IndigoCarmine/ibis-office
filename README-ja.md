<p align="center"><a href="README.md">English</a> | <b>日本語</b></p>

<p align="center">
  <img src="asset/AcaudaOffice_logo.svg" alt="Acauda Office" width="480">
</p>

<p align="center">
  <b>Rust と GPU で作ったオフィス スイート ibisOffice の、非公式のフォークです。</b><br>
  Word（.docx）・Excel（.xlsx）・PowerPoint（.pptx）。<br>
  Powered by <a href="https://github.com/ibisjp/ibis-office">ibisOffice</a>
</p>

---

## Acauda Office とは

Acauda Office は、株式会社アイビスの [ibisOffice](https://github.com/ibisjp/ibis-office) のフォークです。ibisOffice の更新を取り込みながら、独自の機能を加えています。

**アイビスの公式の製品ではありません。アイビスが承認・後援したものでもありません。** Acauda Office の不具合は、アイビスではなく、[このリポジトリの Issues](https://github.com/IndigoCarmine/ibis-office/issues) に報告してください。

名前は、シマエナガ（学名 *<b>A</b>egithalos <b>cauda</b>tus*）から取りました。トキ（ibis）のそばにいる小さな鳥、という意味を込めています。

## このフォークで加えたもの

- **埋め込みの OLE オブジェクトを、それぞれのエディターで編集**：埋め込みのオブジェクト（ChemDraw の構造式など）を、外部のエディターで開く・貼り付ける・コピーする・表示する。仕組みは [`crates/ibis-embed-protocol`](crates/ibis-embed-protocol/README.md) にあります。
- **透明な画像**：画像全体の透過、EMF のアルファ、塗りつぶしのない OLE の図形。
- **クイック アクセス ツール バー**。
- **大きなスライドでも、ウィンドウの移動とサイズの変更が遅れない**。
- **Windows のインストーラー**：ユーザーごとにインストールする Inno Setup のインストーラー（配布用の zip と SHA-256 も）。

ibisOffice そのものの機能・互換性・速さは、[ibisOffice の README](https://github.com/ibisjp/ibis-office#readme) と [実装済み機能](docs/implemented-feature-ja.md) をご覧ください。

## ダウンロード

ビルドしたものは [Releases](https://github.com/IndigoCarmine/ibis-office/releases) に置きます。

- **Windows**：`AcaudaOffice-<版>-Windows.zip` を展開して、`AcaudaOffice` フォルダの `AcaudaOffice.exe` を起動します。または、`AcaudaOffice-<版>-Windows-Setup.exe` でインストールします（使っているユーザーだけに入ります。管理者の権限は要りません）。
- **macOS**：`AcaudaOffice-<版>-macOS.zip` を展開して、`AcaudaOffice.app` をアプリケーション フォルダに移します。
- **Linux**：`AcaudaOffice-<版>-Linux.AppImage` をダウンロードし、実行権限を付けて（`chmod +x`）、ダブルクリックします。

Acauda Office の設定は、ibisOffice とは別のフォルダ（`AcaudaOffice`）に保存します。そのため、両方を同時にインストールできます。自動更新は、このリポジトリのリリースだけを見ます。

コマンドラインの使い方は、[コマンドラインオプション](docs/cli-options-ja.md) をご覧ください。

## ビルド

```bash
git clone --recurse-submodules https://github.com/IndigoCarmine/ibis-office.git
cd ibis-office
cargo build --release
```

- 最新の安定版の Rust が必要です。
- macOS では、`scripts/bundle-macos.sh` で `target/release/AcaudaOffice.app` を作ります。
- Windows では、`pwsh scripts/bundle-windows.ps1` で、配布用の zip とインストーラーを `target/dist` に作ります（インストーラーには [Inno Setup 6](https://jrsoftware.org/isdl.php) が要ります）。
- Linux では、`scripts/bundle-linux.sh` で AppImage を `target/dist` に作ります。
- Web 版は、`scripts/build-web.sh --release` でビルドし、`www/` を配信します（WebGPU に対応したブラウザ）。

## ライセンス

Acauda Office は **AGPL-3.0-only**（[LICENSE](LICENSE)）で公開しています。

- ibisOffice から来たコードの著作権は、株式会社アイビスにあります。[NOTICE](NOTICE) にある AGPL-3.0 第 7 条(b) の追加の条件に従い、Acauda Office は「このアプリについて」の画面に「Powered by ibisOffice」と表示し、https://github.com/ibisjp/ibis-office へのリンクを付けています。
- アイビスは、ibisOffice を商用ライセンスでも提供しています（[COMMERCIAL-LICENSE.md](COMMERCIAL-LICENSE.md)）。このライセンスの対象は、アイビスが配る ibisOffice だけです。このフォークは AGPL-3.0 だけで提供します。
- 第三者の部品とその表示は、[THIRD-PARTY-NOTICES.md](docs/THIRD-PARTY-NOTICES.md) にあります。

## 貢献について

このリポジトリでは、Issues もプル リクエストも受け付けます。[CONTRIBUTING.md](docs/CONTRIBUTING.md) をご覧ください。セキュリティの問題は、[SECURITY.md](docs/SECURITY.md) の方法で知らせてください。

## 商標について

「ibisOffice」の名前とロゴは、株式会社アイビスの商標です（[TRADEMARK.md](docs/TRADEMARK.md)）。Acauda Office は別の名前で、ibisOffice のロゴは使っていません。

Microsoft、Microsoft Office、Word、Excel、PowerPoint は、米国 Microsoft Corporation の米国およびその他の国における登録商標または商標です。その他の会社名・製品名は、各社の商標です。Acauda Office は、これらの会社と提携・承認・後援の関係にありません。
