// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A document's embedded objects (`w:object`): the inline pictures showing
//! them, found and replaced for `shell/embed.rs`.

use gpui_kit::*;
use ibis_doc_model::{Command, EmuSize, FlowCommand, FlowPos, FlowRunKind, MediaFormat, MediaId, OleId, Picture, PictureLook};

use crate::shell::editor::SlideEditor;

impl SlideEditor {
    /// The inline picture at `at`: its media and look.
    fn inline_at(&self, at: FlowPos) -> Option<(MediaId, PictureLook)> {
        let mut offset = 0;
        for run in &self.paragraph_of(at.para)?.runs {
            if offset == at.offset
                && let FlowRunKind::Picture { media, look, .. } = &run.kind
            {
                return Some((*media, look.clone()));
            }
            offset += run.len();
        }
        None
    }

    /// The inline picture at `at`, as a picture (its media and the object it shows).
    pub(crate) fn inline_picture_at(&self, at: FlowPos) -> Option<Picture> {
        let (media, look) = self.inline_at(at)?;
        let mut picture = Picture::new(media);
        picture.ole = look.ole;
        Some(picture)
    }

    /// The inline object a click at `pos` is on: the picture just after or just before it.
    pub(crate) fn inline_object_near(&self, pos: FlowPos) -> Option<FlowPos> {
        [Some(pos.offset), pos.offset.checked_sub(1)]
            .into_iter()
            .flatten()
            .map(|offset| FlowPos::new(pos.para, offset))
            .find(|&at| self.inline_at(at).is_some_and(|(_, look)| look.ole.is_some()))
    }

    /// The object at `at` edited: its new picture (and size) and object, as one undoable edit.
    pub(crate) fn replace_inline_object(&mut self, at: FlowPos, media: MediaId, ole: OleId, size: Option<EmuSize>, cx: &mut Context<Self>) {
        let Some((_, mut look)) = self.inline_at(at) else { return };
        look.ole = Some(ole);
        self.run_flow(vec![FlowCommand::ReplacePicture { at, media, size }, FlowCommand::SetPictureLook { at, look }], cx);
    }

    /// A new object put inline at the caret (its picture `png`); where it is.
    pub(crate) fn insert_inline_object(&mut self, png: Vec<u8>, ole: OleId, size: Option<EmuSize>, cx: &mut Context<Self>) -> Option<FlowPos> {
        let (_, at) = self.text_selection()?;
        self.doc_insert_picture(png, MediaFormat::Png, cx);
        let (_, mut look) = self.inline_at(at)?;
        look.ole = Some(ole);
        let mut commands = vec![Command::Flow(FlowCommand::SetPictureLook { at, look })];
        if let Some(size) = size {
            commands.insert(0, Command::Flow(FlowCommand::SetPictureSize { at, size }));
        }
        // One undo step with the insertion.
        if let Err(e) = self.document.amend_last(commands) {
            log::warn!("embedded object: {e}");
        }
        cx.notify();
        Some(at)
    }
}
