// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A slide's comments (コメント): PowerPoint's threaded comments
//! (`p188:cmLst`, with answers and a resolved state) and the older ones
//! (`p:cmLst`), as one list.

use crate::{EmuPoint, ShapeId};

/// One comment on a slide, with its answers (返信).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlideComment {
    /// The file's id for it (a GUID for a threaded comment, the author's
    /// index for an older one); empty for a new one until it is saved.
    pub id: String,
    pub author: Author,
    /// When it was written, as the file writes it (`2026-10-07T09:00:00.000`).
    pub created: String,
    pub text: String,
    /// Where its mark is on the slide, if it says.
    pub position: Option<EmuPoint>,
    /// The shape it is on (a threaded comment's anchor), if any.
    pub shape: Option<ShapeId>,
    /// 解決済み.
    pub resolved: bool,
    pub replies: Vec<CommentReply>,
}

/// An answer to a comment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommentReply {
    pub id: String,
    pub author: Author,
    pub created: String,
    pub text: String,
}

/// Who wrote a comment or an answer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Author {
    pub name: String,
    pub initials: String,
}
