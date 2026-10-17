# ibis-embed-protocol

How ibisOffice and external editors edit embedded objects together. An
object is a standard OLE object in the document (`oleObjectN.bin`): ibisOffice
keeps the container, the editor edits the content streams and sends a picture
of them. There is no COM, so it works the same on Windows, macOS and Linux.

## Making an editor (Rust)

Add the crate (without the `host` feature) and call `editor::start` at launch:

```rust
use ibis_embed_protocol::editor::{self, Created, EditorEvent, Headless, Start};
use ibis_embed_protocol::{OleContent, Preview};

struct Requests;

// Requests without a window. Leave out the ones you don't support.
impl Headless for Requests {
    fn new_object(&mut self) -> Result<Created, String> { /* an empty object */ todo!() }
    fn render(&mut self, content: OleContent) -> Result<(Preview, Option<(i64, i64)>), String> { todo!() }
}

fn main() {
    match editor::start("com.example.editor", Requests).expect("ibisOffice") {
        Start::NotStarted => { /* started by the user: run as usual */ }
        Start::Done => {} // the request was answered, or the object went to the window already open
        Start::Interactive(editor) => {
            // Open the window. In its loop (a timer, an idle callback):
            //   while let Some(event) = editor.try_next() { … }
            //     EditorEvent::Opened { object, content, title, locale } → show it
            //     EditorEvent::Closed { object } → forget it (quit when none is left)
            // At each change the user makes:
            //   editor.update(object, new_content, Preview::svg(svg), Some((w_emu, h_emu)))
            // Dropping `editor` (closing the window) tells ibisOffice.
        }
    }
}
```

What the library takes care of:

- Arguments: ibisOffice starts the editor with `--ibis-embed <socket> --token <token> --mode interactive|headless`.
- Headless requests: `New`, `Import`, `Export` and `Render` are answered through `Headless`, and the process then exits.
- One window for every object: a second launch hands its object to the running instance and exits. The id you pass to `start` names the instance.
- One connection per object, each on its own thread. `editor.handle()` gives a `Send` handle for sending from other threads.

What the editor does itself:

- **The content.** `OleContent` is the object's class (`clsid`, `prog_id`, `user_type`) and its streams. Give back the class and the streams you don't change as they came. The container's own streams (`\x01CompObj`, `\x01Ole`, …) are ibisOffice's and never reach you.
- **The picture.** Send PNG and/or SVG at each change. ibisOffice draws an SVG to PNG for Office itself.

The mock in `tools/ibis-embed-mock-app` is a complete example: a GPUI window
whose `CONTENTS` stream is the text typed into it.

## Registering an editor

Put a manifest in ibisOffice's `embed-handlers/` folder (設定 > 外部エディター > フォルダーを開く):

```toml
id = "com.example.editor"
command = "/Applications/Example.app/Contents/MacOS/example"
prog_ids = ["Example.Document"]   # this prefix and those below it: Example.Document.6.0 …
new_object = true                  # shown in Insert ▸ Object
import_extensions = ["txt"]
export_extensions = ["txt"]

[name]
ja = "例のエディター"
en = "Example Editor"
```

## The wire, for editors not in Rust

- **Transport.** A local socket: a named pipe on Windows, an abstract or file Unix socket elsewhere. Its name is the `--ibis-embed` argument (a path when it contains `/` or `\`).
- **Frames.** A 4-byte little-endian length, then JSON. Each message has a `"type"` field. Binary fields are base64. A CLSID is 32 hex digits in the byte order of the compound file.
- **First message.** The editor sends `{"type":"Hello","token":"<--token>","protocol":1}`.
- **Interactive mode.**
  - ibisOffice sends `Open {content, title, locale}`.
  - The editor sends `Update {content, preview:{png?,svg?}, size_emu?}` whenever the object changes, and `Closed` when it closes it.
  - ibisOffice sends `Close` when it closes the object.
- **Headless mode.** ibisOffice sends one of `New`, `Import {extension, data}`, `Export {content, extension}` or `Render {content}`. The editor answers with `Created`, `Exported {data}`, `Rendered` or `Error {message}`, then exits.
- **One window for every object.** A launch that hands its object to a running instance should exit with status 0. ibisOffice then keeps waiting for the connection.

The message types are in `src/message.rs`.
