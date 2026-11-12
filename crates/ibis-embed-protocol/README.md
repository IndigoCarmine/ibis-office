# ibis-embed-protocol

How ibisOffice edits embedded objects in external editors. The protocol, the
library for making an editor, the manifests and the wire format are in
[acauda-embed](https://github.com/IndigoCarmine/acauda-embed), which this
crate re-exports. Start there to make an editor.

What is here is ibisOffice's own side, behind the `host` feature: an object's
compound file (`oleObjectN.bin`) to the `OleContent` an editor is given and
back, and an editor's preview to the PNG Office shows.

`tools/ibis-embed-mock` and `tools/ibis-embed-mock-app` are mock editors for
trying it out; `tests/` runs them against ibisOffice's end.
