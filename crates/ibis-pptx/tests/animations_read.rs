// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A slide's animations (`p:timing`): the effects of its main sequence
//! read in order, by class, start, look and time, for the shapes they name.
//! The timing is written into a copy of the plan test file's first slide
//! (`ibis_pptx::test_files::pptx_with_animations`).
use ibis_doc_model::{AnimationClass, AnimationLook, AnimationSide, AnimationStart};

#[path = "../../../test/fixtures.rs"]
mod fixtures;

/// The test file, or `None` (reported as skipped) when the private test data
/// is not there.
fn test_file(name: &str) -> Option<Vec<u8>> {
    fixtures::read(&format!("test/editing/{name}"))
}

#[test]
fn animations_are_read_in_order() {
    let Some(plan) = test_file("pptx-plan.pptx") else { return };
    let data = ibis_pptx::test_files::pptx_with_animations(&plan);
    let p = ibis_pptx::read(&data).unwrap();
    let a = &p.slides[0].animations;
    assert_eq!(a.len(), 4, "{a:?}");
    assert_eq!((a[0].class, a[0].start, a[0].look), (AnimationClass::Entrance, AnimationStart::OnClick, AnimationLook::Appear));
    assert_eq!((a[1].class, a[1].start, a[1].look, a[1].duration), (AnimationClass::Entrance, AnimationStart::AfterPrevious, AnimationLook::Fade, 500));
    assert_eq!((a[2].class, a[2].start, a[2].look), (AnimationClass::Entrance, AnimationStart::WithPrevious, AnimationLook::Fly(AnimationSide::Bottom)));
    assert_eq!((a[3].class, a[3].start, a[3].look), (AnimationClass::Exit, AnimationStart::OnClick, AnimationLook::Fade));
    // Each names a shape of the slide: the title, then the subtitle.
    let names: Vec<usize> = a.iter().map(|x| p.slides[0].shapes.iter().position(|s| s.id == x.shape).unwrap()).collect();
    assert_eq!(names, vec![0, 1, 0, 1]);
    // Saved untouched, the timing stays.
    let opened = ibis_pptx::open(data).unwrap();
    let saved = opened.source.save(&opened.presentation, ibis_pptx::SaveOptions::default()).unwrap();
    assert_eq!(ibis_pptx::read(&saved).unwrap().slides[0].animations, *a);
}
