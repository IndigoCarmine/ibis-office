// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The presentation document: slides, shapes and pictures.

use std::sync::Arc;

use crate::chart::Chart;
use crate::table::Table;
use crate::drawing::{Color, Fill, Geometry, Stroke};
use crate::geometry::{EMU_PER_INCH, Emu, EmuPoint, EmuRect, EmuSize};
use crate::text::{RunStyle, TextBody, TextStyle};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SlideId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ShapeId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MediaId(pub u64);

/// A slide layout of a presentation (see [`Layout`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LayoutId(pub u64);

/// A slide master of a presentation (see [`Master`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MasterId(pub u64);

/// A slide master (スライド マスター) the layouts follow: its name is the
/// theme's (`a:theme/@name`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Master {
    pub id: MasterId,
    pub name: String,
    /// The master this one was copied from, not yet saved (a writer makes its
    /// parts as copies of the original's, with its layouts); `None` for a
    /// master of a file.
    pub copy_of: Option<MasterId>,
}

/// A slide layout: what a new slide made from it looks like before
/// anything is put on it. Its placeholders are not shapes of the model
/// (an empty placeholder shows nothing); a writer makes them from the
/// layout's own file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    pub id: LayoutId,
    /// The layout's name, such as "タイトルとコンテンツ" (`p:cSld/@name`).
    pub name: String,
    /// The background of a slide made from it.
    pub background: Fill,
    /// The layout's and its master's own shapes, as a slide's
    /// [`Slide::backdrop`].
    pub backdrop: Vec<Shape>,
    /// The layout's placeholders, and its master's: where a slide's
    /// placeholder that takes its place from them goes.
    pub placeholders: Vec<LayoutPlaceholder>,
    pub master_placeholders: Vec<LayoutPlaceholder>,
    /// The layout this one was copied from, not yet saved (a writer makes
    /// its part as a copy of the original's); `None` for a layout of a file.
    pub copy_of: Option<LayoutId>,
    /// The master it follows (`None` for a layout brought in by a paste).
    pub master: Option<MasterId>,
}

/// Which placeholder a shape is (`p:nvPr/p:ph`): its type and index as
/// written (`None` when not written: a body, index 0).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct PlaceholderRef {
    pub kind: Option<String>,
    pub index: Option<String>,
}

impl PlaceholderRef {
    /// The type as placeholders are matched: `ctrTitle` is a title, and
    /// no type, `obj` and `body` are a body.
    pub fn normalized(&self) -> &str {
        match self.kind.as_deref() {
            Some("ctrTitle") | Some("title") => "title",
            None | Some("obj") | Some("body") => "body",
            Some(other) => other,
        }
    }
}

/// A placeholder of a layout or master, and where it is (frame, rotation,
/// flips), if it says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutPlaceholder {
    pub placeholder: PlaceholderRef,
    pub placement: Option<(EmuRect, i32, bool, bool)>,
    /// Its own prompt for slides (`p:ph/@hasCustomPrompt`: its text).
    pub prompt: Option<String>,
}

/// A placeholder shape of a slide: which it is, and whether it has its own
/// place (else it takes its layout's, and a new layout moves it).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlidePlaceholder {
    pub placeholder: PlaceholderRef,
    pub own_frame: bool,
}

impl Layout {
    /// The layout's placeholder a slide's placeholder `ph` follows: by
    /// index, then by type (a subtitle, picture, chart… falls back to a
    /// body), as PowerPoint matches them.
    fn matching<'a>(list: &'a [LayoutPlaceholder], ph: &PlaceholderRef, by_index: bool) -> Option<&'a LayoutPlaceholder> {
        if by_index
            && let Some(idx) = &ph.index
            && let Some(found) = list.iter().find(|p| p.placeholder.index.as_ref() == Some(idx))
        {
            return Some(found);
        }
        let kind = ph.normalized();
        if let Some(found) = list.iter().find(|p| p.placeholder.normalized() == kind) {
            return Some(found);
        }
        match kind {
            "subTitle" | "pic" | "chart" | "tbl" | "dgm" | "media" | "clipArt" => list.iter().find(|p| p.placeholder.normalized() == "body"),
            _ => None,
        }
    }

    /// What the editor shows in a slide's empty placeholder `ph` (案内文字),
    /// as PowerPoint does: its layout placeholder's own prompt, else the
    /// usual one for its type; nothing for those PowerPoint shows no text
    /// in (dates, footers, slide numbers; pictures, charts, tables… show
    /// icons).
    pub fn prompt_for(&self, ph: &PlaceholderRef) -> Option<&str> {
        if let Some(p) = Self::matching(&self.placeholders, ph, true).and_then(|l| l.prompt.as_deref()) {
            return Some(p);
        }
        match ph.kind.as_deref() {
            Some("ctrTitle") | Some("title") => Some("クリックしてタイトルを入力"),
            Some("subTitle") => Some("クリックしてサブタイトルを入力"),
            None | Some("obj") | Some("body") => Some("クリックしてテキストを入力"),
            _ => None,
        }
    }

    /// Where a slide's placeholder `ph` with no place of its own goes on a
    /// slide of this layout: its layout placeholder's place, else its
    /// master placeholder's. `None` if neither says.
    pub fn placement_for(&self, ph: &PlaceholderRef) -> Option<(EmuRect, i32, bool, bool)> {
        let layout = Self::matching(&self.placeholders, ph, true);
        if let Some(p) = layout.and_then(|l| l.placement) {
            return Some(p);
        }
        let key = layout.map_or(ph, |l| &l.placeholder);
        Self::matching(&self.master_placeholders, key, false).and_then(|m| m.placement)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MediaFormat {
    Png,
    Jpeg,
    Gif,
    Bmp,
    Tiff,
    Webp,
    Svg,
    /// Windows metafiles; kept for saving, not drawn yet.
    Emf,
    Wmf,
    /// Video and audio files (動画・音声): kept as the file has them.
    Mp4,
    M4v,
    Mov,
    Wmv,
    Avi,
    Mpeg,
    Mp3,
    M4a,
    Wav,
    Wma,
    Other,
}

impl MediaFormat {
    pub fn is_image(self) -> bool {
        matches!(self, Self::Png | Self::Jpeg | Self::Gif | Self::Bmp | Self::Tiff | Self::Webp | Self::Svg | Self::Emf | Self::Wmf)
    }

    pub fn is_video(self) -> bool {
        matches!(self, Self::Mp4 | Self::M4v | Self::Mov | Self::Wmv | Self::Avi | Self::Mpeg)
    }

    pub fn is_audio(self) -> bool {
        matches!(self, Self::Mp3 | Self::M4a | Self::Wav | Self::Wma)
    }

    /// The file extension and content type of a video or audio format.
    pub fn av_type(self) -> Option<(&'static str, &'static str)> {
        Some(match self {
            Self::Mp4 => ("mp4", "video/mp4"),
            Self::M4v => ("m4v", "video/mp4"),
            Self::Mov => ("mov", "video/quicktime"),
            Self::Wmv => ("wmv", "video/x-ms-wmv"),
            Self::Avi => ("avi", "video/avi"),
            Self::Mpeg => ("mpg", "video/mpeg"),
            Self::Mp3 => ("mp3", "audio/mpeg"),
            Self::M4a => ("m4a", "audio/mp4"),
            Self::Wav => ("wav", "audio/wav"),
            Self::Wma => ("wma", "audio/x-ms-wma"),
            _ => return None,
        })
    }

    /// The format of a part named `path`, from its extension.
    pub fn from_path(path: &str) -> Self {
        let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        match ext.as_str() {
            "png" => Self::Png,
            "jpg" | "jpeg" | "jpe" => Self::Jpeg,
            "gif" => Self::Gif,
            "bmp" => Self::Bmp,
            "tif" | "tiff" => Self::Tiff,
            "webp" => Self::Webp,
            "svg" => Self::Svg,
            "emf" => Self::Emf,
            "wmf" => Self::Wmf,
            "mp4" => Self::Mp4,
            "m4v" => Self::M4v,
            "mov" => Self::Mov,
            "wmv" => Self::Wmv,
            "avi" => Self::Avi,
            "mpg" | "mpeg" => Self::Mpeg,
            "mp3" => Self::Mp3,
            "m4a" => Self::M4a,
            "wav" => Self::Wav,
            "wma" => Self::Wma,
            _ => Self::Other,
        }
    }
}

/// A picture file stored in the presentation, shared by the shapes using it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Media {
    pub id: MediaId,
    pub format: MediaFormat,
    pub data: Arc<[u8]>,
}

/// A font file embedded in the presentation, as stored (for PowerPoint, an
/// Embedded OpenType file). It may only be used to show and edit this
/// presentation, as its embedding permission allows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmbeddedFont {
    /// The typeface name text refers to, such as "Poppins SemiBold".
    pub typeface: String,
    pub bold: bool,
    pub italic: bool,
    pub data: Arc<[u8]>,
}

/// The part of a picture that is shown, as fractions cut from each side in
/// 100 000ths (OOXML's `a:srcRect`). Negative values add empty space.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Crop {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutoShape {
    pub geometry: Geometry,
    pub fill: Fill,
    pub stroke: Option<Stroke>,
    pub text: Option<TextBody>,
    /// The shape seen tilted in 3D (`a:scene3d`), its text kept flat.
    pub tilt: Option<Tilt>,
    pub shadow: Option<crate::Shadow>,
    /// A soft glow around the shape (`a:glow`): colour and radius in EMU.
    pub glow: Option<(crate::Color, i64)>,
    /// A shadow inside the shape (`a:innerShdw`).
    pub inner_shadow: Option<crate::Shadow>,
    /// Soft edges (`a:softEdge`): the shape fades out over this radius (EMU)
    /// inside its edge.
    pub soft_edge: Option<i64>,
    /// A reflection under the shape (`a:reflection`).
    pub reflection: Option<crate::Reflection>,
    /// A 3D bevel effect on the shape (`a:sp3d/a:bevelT`).
    pub bevel: Option<crate::drawing::Bevel>,
}

impl AutoShape {
    /// A rectangle holding `text`, with `fill` and no outline: a text box
    /// as a presentation stores it.
    pub fn text_box(fill: Fill, text: TextBody) -> Self {
        Self { geometry: Geometry::Rect, fill, stroke: None, text: Some(text), tilt: None, shadow: None, glow: None, inner_shadow: None, soft_edge: None, reflection: None, bevel: None }
    }

    /// A shape of `geometry` with `fill` and `stroke`, and no text.
    pub fn new(geometry: Geometry, fill: Fill, stroke: Option<Stroke>) -> Self {
        Self { geometry, fill, stroke, text: None, tilt: None, shadow: None, glow: None, inner_shadow: None, soft_edge: None, reflection: None, bevel: None }
    }
}

/// A 3D view of a flat shape through an orthographic camera: turned about
/// the horizontal axis by `lat` and the vertical one by `lon` (60 000ths of
/// a degree), and lifted `depth` EMU off the slide (`a:sp3d/@z`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tilt {
    pub lat: i32,
    pub lon: i32,
    pub depth: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    pub media: MediaId,
    pub crop: Crop,
    /// Outline around the picture (`p:spPr/a:ln`).
    pub stroke: Option<crate::Stroke>,
    /// Corner radius of a rounded-rectangle frame, in 100 000ths of the
    /// shorter side; 0 for square corners.
    pub corner: u32,
    /// Recoloured from dark to light between two colours (`a:duotone`).
    pub duotone: Option<(crate::Rgb, crate::Rgb)>,
    /// A colour made transparent (`a:clrChange` to a clear colour, Office's
    /// "set transparent colour").
    pub clear: Option<crate::Rgb>,
    /// Luminance adjustment (`a:lum`): bright and contrast in 100 000ths (-100 000 to 100 000).
    pub luminance: Option<(i32, i32)>,
    /// A video or an audio clip this picture is the poster frame (or the
    /// speaker icon) of (`a:videoFile`, `a:audioFile`).
    pub clip: Option<Clip>,
    /// Shadow effect on the picture (`a:effectLst/a:outerShdw`).
    pub shadow: Option<crate::Shadow>,
    /// Reflection effect under the picture (`a:effectLst/a:reflection`).
    pub reflection: Option<crate::Reflection>,
    /// Soft edge blur radius in EMU (`a:effectLst/a:softEdge`).
    pub soft_edge: Option<i64>,
    /// Alternative text (`wp:docPr/@descr`, `@title`).
    pub alt_text: AltText,
    /// Whether the picture is marked as decorative (`a1611:decorative/@val="1"`).
    pub decorative: bool,
    /// Object name (`wp:docPr/@name`).
    pub name: String,
}

impl Picture {
    pub fn new(media: MediaId) -> Self {
        Self {
            media,
            crop: Crop::default(),
            stroke: None,
            corner: 0,
            duotone: None,
            clear: None,
            luminance: None,
            clip: None,
            shadow: None,
            reflection: None,
            soft_edge: None,
            alt_text: AltText::default(),
            decorative: false,
            name: String::new(),
        }
    }
}

/// How a media clip starts playing in a slide show.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MediaStart {
    #[default]
    OnClick,
    Automatically,
}

/// The playback volume of a media clip.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MediaVolume {
    Low,
    Medium,
    #[default]
    High,
    Mute,
}

/// A bookmark within a media clip (PPTX `<p14:bmk name="..." time="..."/>`).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct MediaBookmark {
    pub name: String,
    pub time_ms: u32,
}

impl MediaBookmark {
    pub fn new(name: impl Into<String>, time_ms: u32) -> Self {
        Self {
            name: name.into(),
            time_ms,
        }
    }
}

/// A closed caption track for a video or audio clip (PPTX `<p15:captionTrack>`).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct MediaCaptionTrack {
    /// 字幕の表示ラベル（例: "日本語", "English"）
    pub label: String,
    /// 言語コード（例: "ja-JP", "en-US"）
    pub lang: String,
    /// WebVTT 形式の字幕データまたはパス
    pub vtt: String,
}

impl MediaCaptionTrack {
    pub fn new(label: impl Into<String>, lang: impl Into<String>, vtt: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            lang: lang.into(),
            vtt: vtt.into(),
        }
    }
}

/// Playback options for a video or audio clip.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ClipOptions {
    pub start: MediaStart,
    pub volume: MediaVolume,
    pub loop_until_stopped: bool,
    pub rewind: bool,
    /// 全画面再生（スライドショー実行時に全画面で再生: `fullScreen="1"`）
    pub full_screen: bool,
    /// 再生していない時は非表示（`showWhenStopped="0"`）
    pub hide_while_not_playing: bool,
    /// スライドショー実行中に非表示（音声のアイコン非表示: `isNarration="1"`）
    pub hide_during_show: bool,
    /// スライドをまたいで再生（`numSlds="999"` / `playAcrossSlides="1"`）
    pub play_across_slides: bool,
    /// トリム開始時間（ミリ秒: `<p14:trim st="...">`）
    pub trim_start_ms: u32,
    /// トリム終了時間（ミリ秒: `<p14:trim end="...">`、0 はメディア終端まで）
    pub trim_end_ms: u32,
    /// フェードイン時間（ミリ秒: `<p14:fade in="...">`）
    pub fade_in_ms: u32,
    /// フェードアウト時間（ミリ秒: `<p14:fade out="...">`）
    pub fade_out_ms: u32,
    /// 再生ブックマーク一覧（`<p14:bmkLst>`）
    pub bookmarks: Vec<MediaBookmark>,
    /// 字幕・キャプショントラック一覧（`<p15:captionTracks>`）
    pub captions: Vec<MediaCaptionTrack>,
}

impl ClipOptions {
    /// トリミングされているか（開始または終了が設定されているか）
    pub fn is_trimmed(&self) -> bool {
        self.trim_start_ms > 0 || self.trim_end_ms > 0
    }

    /// 音量フェード（インまたはアウト）が設定されているか
    pub fn is_faded(&self) -> bool {
        self.fade_in_ms > 0 || self.fade_out_ms > 0
    }

    /// ブックマークを追加（時間昇順で挿入）。挿入位置インデックスを返す
    pub fn add_bookmark(&mut self, time_ms: u32, name: Option<String>) -> usize {
        let name = name.unwrap_or_else(|| {
            let n = self.bookmarks.len() + 1;
            format!("Bookmark {n}")
        });
        let idx = self.bookmarks.partition_point(|b| b.time_ms <= time_ms);
        self.bookmarks.insert(idx, MediaBookmark { name, time_ms });
        idx
    }

    /// 指定インデックスのブックマークを削除
    pub fn remove_bookmark(&mut self, index: usize) -> Option<MediaBookmark> {
        if index < self.bookmarks.len() {
            Some(self.bookmarks.remove(index))
        } else {
            None
        }
    }

    /// 指定ミリ秒のブックマークを削除
    pub fn remove_bookmark_at(&mut self, time_ms: u32) -> bool {
        let len_before = self.bookmarks.len();
        self.bookmarks.retain(|b| b.time_ms != time_ms);
        self.bookmarks.len() != len_before
    }

    /// 全ブックマークを消去
    pub fn clear_bookmarks(&mut self) {
        self.bookmarks.clear();
    }

    /// 字幕が設定されているか
    pub fn has_captions(&self) -> bool {
        !self.captions.is_empty()
    }

    /// 字幕トラックを追加
    pub fn add_caption(&mut self, label: impl Into<String>, lang: impl Into<String>, vtt: impl Into<String>) {
        self.captions.push(MediaCaptionTrack::new(label, lang, vtt));
    }

    /// 指定インデックスの字幕トラックを削除
    pub fn remove_caption(&mut self, index: usize) -> Option<MediaCaptionTrack> {
        if index < self.captions.len() {
            Some(self.captions.remove(index))
        } else {
            None
        }
    }

    /// 全字幕トラックを消去
    pub fn clear_captions(&mut self) {
        self.captions.clear();
    }

    /// バックグラウンド再生の条件（スライドをまたいで再生、ループ、スライドショー中非表示、自動再生）を満たしているか
    pub fn is_background_play(&self) -> bool {
        self.play_across_slides
            && self.loop_until_stopped
            && self.hide_during_show
            && self.start == MediaStart::Automatically
    }

    /// バックグラウンド再生プリセットの適用・解除
    pub fn set_background_play(&mut self, enabled: bool) {
        if enabled {
            self.play_across_slides = true;
            self.loop_until_stopped = true;
            self.hide_during_show = true;
            self.start = MediaStart::Automatically;
        } else {
            self.play_across_slides = false;
            self.loop_until_stopped = false;
            self.hide_during_show = false;
            self.start = MediaStart::OnClick;
        }
    }
}

/// A video or audio file on a slide (挿入 > ビデオ・オーディオ): its bytes
/// are a [`Media`] of the presentation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Clip {
    pub media: MediaId,
    pub audio: bool,
    pub options: ClipOptions,
    /// The file it plays when it is linked, not kept in the presentation
    /// (`a:videoFile/@r:link` to an external file), as written: a path or a
    /// `file:` URL. `media` is then an empty medium of the file's format.
    pub link: Option<String>,
}

impl Clip {
    pub fn new(media: MediaId, audio: bool) -> Self {
        Self {
            media,
            audio,
            options: ClipOptions::default(),
            link: None,
        }
    }
}

/// Content that is kept in place but not drawn yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Placeholder {
    /// A chart that could not be read.
    Chart,
    /// A table that could not be read.
    Table,
    /// A picture in a format that is not drawn, such as EMF.
    Picture(MediaFormat),
    /// A SmartArt graphic's frame: its shapes are read from the drawing
    /// PowerPoint keeps of it (`dsp:drawing`) and attached to it; the frame
    /// draws nothing itself.
    Diagram,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShapeKind {
    TextBox(TextBody),
    Auto(AutoShape),
    Picture(Picture),
    /// A straight line across the frame, from its top-left to its
    /// bottom-right corner (other corners with [`Shape::flip_h`] / `flip_v`).
    Line(Stroke),
    Chart(Box<Chart>),
    Table(Box<Table>),
    /// A worksheet's cells.
    Sheet(std::sync::Arc<crate::sheet::Sheet>),
    /// A page of a flowing document, as the viewer lays it out.
    Page { document: std::sync::Arc<crate::flow::FlowDocument>, index: usize },
    /// A slicer (スライサー) over a table or pivot table.
    Slicer(std::sync::Arc<crate::slicer::Slicer>),
    Placeholder(Placeholder),
}

/// WordprocessingML floating object anchor (`wp:anchor`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FloatingAnchor {
    pub offset_x: Emu,
    pub offset_y: Emu,
    pub align_h: Option<String>,
    pub align_v: Option<String>,
    pub relative_h: String,
    pub relative_v: String,
    pub wrap: String,
    pub behind_doc: bool,
    pub dist_t: Emu,
    pub dist_b: Emu,
    pub dist_l: Emu,
    pub dist_r: Emu,
    pub wrap_text: Option<String>,
    pub locked: bool,
    pub allow_overlap: bool,
    pub layout_in_cell: bool,
    pub relative_height: u32,
}

impl FloatingAnchor {
    /// Standard default relativeHeight value (Word uses 251658240).
    pub const DEFAULT_RELATIVE_HEIGHT: u32 = 251_658_240;

    /// Returns true if the object moves with text (relative to paragraph or line).
    pub fn moves_with_text(&self) -> bool {
        self.relative_v == "paragraph" || self.relative_v == "line"
    }
}

impl Default for FloatingAnchor {
    fn default() -> Self {
        Self {
            offset_x: 0,
            offset_y: 0,
            align_h: None,
            align_v: None,
            relative_h: "column".to_string(),
            relative_v: "paragraph".to_string(),
            wrap: "square".to_string(),
            behind_doc: false,
            dist_t: 0,
            dist_b: 0,
            dist_l: 114_300, // 9 pt
            dist_r: 114_300, // 9 pt
            wrap_text: Some("bothSides".to_string()),
            locked: false,
            allow_overlap: true,
            layout_in_cell: true,
            relative_height: Self::DEFAULT_RELATIVE_HEIGHT,
        }
    }
}

/// Text wrapping style for an inline or floating picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PictureWrapType {
    /// In line with text (`wp:inline`).
    Inline,
    /// Square text wrapping around the picture (`wp:wrapSquare`).
    Square,
    /// Tight text wrapping around the picture (`wp:wrapTight`).
    Tight,
    /// Through text wrapping (`wp:wrapThrough`).
    Through,
    /// Top and bottom text wrapping (`wp:wrapTopAndBottom`).
    TopAndBottom,
    /// Behind text (`wp:wrapNone` + `behindDoc="1"`).
    BehindText,
    /// In front of text (`wp:wrapNone` + `behindDoc="0"`).
    InFrontOfText,
}

impl PictureWrapType {
    /// The wrapping style corresponding to DrawingML's floating anchor.
    pub fn from_anchor(anchor: &FloatingAnchor) -> Self {
        match anchor.wrap.as_str() {
            "square" => Self::Square,
            "tight" => Self::Tight,
            "through" => Self::Through,
            "topAndBottom" => Self::TopAndBottom,
            "none" if anchor.behind_doc => Self::BehindText,
            "none" => Self::InFrontOfText,
            _ => Self::Square,
        }
    }

    /// The anchor wrap name for DrawingML (`"square"`, `"tight"`, `"through"`, `"topAndBottom"`, `"none"`).
    pub fn to_anchor_wrap(self) -> &'static str {
        match self {
            Self::Inline => "none",
            Self::Square => "square",
            Self::Tight => "tight",
            Self::Through => "through",
            Self::TopAndBottom => "topAndBottom",
            Self::BehindText | Self::InFrontOfText => "none",
        }
    }
}


#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shape {
    pub id: ShapeId,
    pub frame: EmuRect,
    /// Clockwise rotation about the frame's centre, in 60 000ths of a degree.
    pub rotation: i32,
    pub flip_h: bool,
    pub flip_v: bool,
    pub kind: ShapeKind,
    pub floating: Option<FloatingAnchor>,
}

impl Shape {
    pub fn new(id: ShapeId, frame: EmuRect, kind: ShapeKind) -> Self {
        Self { id, frame, rotation: 0, flip_h: false, flip_v: false, kind, floating: None }
    }

    pub fn text_box(id: ShapeId, frame: EmuRect, text: impl Into<String>) -> Self {
        let body = TextBody::plain(&text.into(), TextStyle::default());
        Self::new(id, frame, ShapeKind::TextBox(body))
    }

    pub fn text_body(&self) -> Option<&TextBody> {
        match &self.kind {
            ShapeKind::TextBox(body) => Some(body),
            ShapeKind::Auto(shape) => shape.text.as_ref(),
            _ => None,
        }
    }

    pub fn text_body_mut(&mut self) -> Option<&mut TextBody> {
        match &mut self.kind {
            ShapeKind::TextBox(body) => Some(body),
            ShapeKind::Auto(shape) => shape.text.as_mut(),
            _ => None,
        }
    }
}

/// A slide. Shapes are stored back to front: the last one is on top.
/// The original PowerPoint group hierarchy; leaf shapes stay in slide coordinates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShapeGroup {
    /// A native hidden/unread child or MC carrier keeps the group alive.
    pub has_unmodelled_children:bool,
    pub native_id:String,
    pub name:String,
    pub children:Vec<GroupMember>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GroupMember {Shape(ShapeId),Group(Box<ShapeGroup>)}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slide {
    pub id: SlideId,
    pub shapes: Vec<Shape>,
    pub groups: Vec<ShapeGroup>,
    pub background: Fill,
    /// Shapes of the slide's layout and master, drawn behind `shapes`. They
    /// cannot be selected or edited on the slide.
    pub backdrop: Vec<Shape>,
    /// The page's own size (a worksheet's used area); `None` uses the
    /// presentation's slide size.
    pub size: Option<EmuSize>,
    /// The page's name (a worksheet's tab).
    pub name: Option<String>,
    /// The layout the slide follows (presentations only; `None` for pages
    /// of other documents, or a new slide a writer gives a layout).
    pub layout: Option<LayoutId>,
    /// Shapes that are part of another shape and follow its frame, by that
    /// shape: the shapes drawn over a chart (`c:userShapes`), which come
    /// right after it in `shapes`. They are not selected or edited on their
    /// own. Entries of removed shapes may stay (ids are not reused).
    pub attached: std::collections::HashMap<ShapeId, Vec<ShapeId>>,
    /// The slide's placeholder shapes (presentations).
    pub placeholders: std::collections::HashMap<ShapeId, SlidePlaceholder>,
    /// Skipped in the slide show (`p:sld/@show="0"`).
    pub hidden: bool,
    /// The speaker notes' text (the notes page's body placeholder),
    /// paragraphs joined by `\n`; empty when there are none.
    pub notes: String,
    /// The shapes of the slide's notes page other than its placeholders (a
    /// picture or a shape put on it), as read; the notes page view draws them.
    pub notes_shapes: Vec<Shape>,
    /// How the slide comes in during the show (画面切り替え), if set.
    pub transition: Option<Transition>,
    /// Its animations' effects in order (read from the file, played in the
    /// slide show; the file's timing is saved as it was).
    pub animations: Vec<crate::Animation>,
    /// Its comments (コメント), in the file's order.
    pub comments: Vec<crate::SlideComment>,
    /// The shapes' names (選択ウィンドウ; `cNvPr/@name`), those it has.
    pub shape_names: std::collections::HashMap<ShapeId, String>,
    /// Shapes not shown (選択ウィンドウ's eye; `cNvPr/@hidden`): kept in
    /// `shapes`, not drawn nor hit.
    pub hidden_shapes: std::collections::HashSet<ShapeId>,
    /// 背景グラフィックを表示しない (`p:sld/@showMasterSp="0"`): the
    /// backdrop is kept, not drawn.
    pub hide_backdrop: bool,
    /// Shapes that are hyperlinks (図形のリンク, `cNvPr/a:hlinkClick`).
    pub shape_links: std::collections::HashMap<ShapeId, crate::text::SlideLink>,
    /// Tooltips / ScreenTips of shape hyperlinks (`cNvPr/a:hlinkClick/@tooltip`).
    pub shape_link_tooltips: std::collections::HashMap<ShapeId, String>,
    /// Shapes whose proportions are kept when they are resized (縦横比を
    /// 固定, the `noChangeAspect` of their locks).
    pub aspect_locked: std::collections::HashSet<ShapeId>,
    /// Alternative text for shapes (代替テキスト: `cNvPr/@descr`, `@title`).
    pub shape_alt_texts: std::collections::HashMap<ShapeId, AltText>,
    /// Shapes that are marked as decorative (装飾用オブジェクト: `cNvPr/a:extLst/a:ext[@uri='{C183D7F6-B498-43B3-948B-1728B52AA6E4}']/a1611:decorative/@val="1"`).
    pub decorative_shapes: std::collections::HashSet<ShapeId>,
    /// How shapes are anchored to cells in spreadsheets (セルに対する配置方式: `editAs`).
    pub shape_placements: std::collections::HashMap<ShapeId, ObjectPlacement>,
    /// Whether shapes are printed with the sheet (オブジェクトを印刷する: `fPrintsWithSheet`).
    /// Shapes not present are printed (default `true`).
    pub shape_prints: std::collections::HashMap<ShapeId, bool>,
    /// Formulas linked to shapes (セルの値を表示する数式連動: `xdr:f`).
    pub shape_formulas: std::collections::HashMap<ShapeId, String>,
    /// Shapes that return to the zoom source slide after viewing (ズーム後に元へ戻る: `p16:zoom/@returnToZoom="1"`).
    pub shape_return_to_zoom: std::collections::HashSet<ShapeId>,
    /// Whether zoom shapes show the slide background (ズームの背景: `p16:zoom/@showBg`).
    /// Shapes not present show background (default `true`). `false` makes the background transparent.
    pub shape_zoom_show_bg: std::collections::HashMap<ShapeId, bool>,
    /// Transition duration for zoom shapes, in milliseconds (ズームの継続時間: `p16:zoom/@dur`).
    /// Default is 1000ms.
    pub shape_zoom_durations: std::collections::HashMap<ShapeId, u32>,
}

/// オブジェクト（図形・画像・グラフ等）のセルに対する配置方式 (`editAs`).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum ObjectPlacement {
    /// セルに合わせて移動やサイズ変更をする (`editAs="twoCell"`)
    #[default]
    MoveAndSize,
    /// セルに合わせて移動するがサイズ変更はしない (`editAs="oneCell"`)
    MoveOnly,
    /// セルに合わせて移動やサイズ変更をしない (`editAs="absolute"`)
    Free,
}

/// Alternative text for accessibility (`cNvPr/@descr`, `@title`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AltText {
    /// The title of the alternative text (`cNvPr/@title`).
    pub title: String,
    /// The detailed description (`cNvPr/@descr`).
    pub description: String,
}

/// A sound played during a slide transition (`p:sndAc`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransitionSound {
    /// Play a named sound effect (`p:stSnd/p:snd`).
    Play { name: String, loop_sound: bool },
    /// Stop any previously playing transition sound (`p:endSnd`).
    Stop,
}

/// A slide's transition (`p:transition`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transition {
    /// The effect, by its element's name (`fade`, `push`, `wipe`, and the
    /// PowerPoint 2010 ones such as `vortex`); empty for none (only the timing).
    pub effect: String,
    /// The effect's direction (`dir`: `l`, `r`, `u`, `d`, `horz`, `in`, …).
    pub direction: Option<String>,
    /// How long it takes, milliseconds.
    pub duration: u32,
    /// A click moves on to the next slide (`advClick`).
    pub on_click: bool,
    /// The show moves on by itself after this long (`advTm`), milliseconds.
    pub after: Option<u32>,
    /// Sound played with the transition (`p:sndAc`).
    pub sound: Option<TransitionSound>,
}

impl Transition {
    /// The effects of the base schema (the others are PowerPoint 2010's, `p14:`).
    pub const BASE_EFFECTS: [&'static str; 21] = [
        "blinds", "checker", "circle", "comb", "cover", "cut", "diamond", "dissolve", "fade", "newsflash", "plus", "pull", "push", "random",
        "randomBar", "split", "strips", "wedge", "wheel", "wipe", "zoom",
    ];

    /// A transition of `effect` taking PowerPoint's usual time.
    pub fn new(effect: &str) -> Self {
        Self { effect: effect.to_string(), direction: None, duration: 700, on_click: true, after: None, sound: None }
    }
}

impl Slide {
    pub fn new(id: SlideId) -> Self {
        Self { id, shapes: Vec::new(), groups: Vec::new(), background: Fill::Solid(Color::WHITE), backdrop: Vec::new(), size: None, name: None, layout: None, attached: Default::default(), placeholders: Default::default(), hidden: false, notes: String::new(), notes_shapes: Vec::new(), transition: None, animations: Vec::new(), comments: Vec::new(), shape_names: Default::default(), hidden_shapes: Default::default(), hide_backdrop: false, shape_links: Default::default(), shape_link_tooltips: Default::default(), aspect_locked: Default::default(), shape_alt_texts: Default::default(), decorative_shapes: Default::default(), shape_placements: Default::default(), shape_prints: Default::default(), shape_formulas: Default::default(), shape_return_to_zoom: Default::default(), shape_zoom_show_bg: Default::default(), shape_zoom_durations: Default::default() }
    }

    /// A new empty slide following `layout`, as PowerPoint's "New Slide"
    /// makes it (its empty placeholders are not shapes of the model).
    pub fn from_layout(id: SlideId, layout: &Layout) -> Self {
        Self { background: layout.background.clone(), backdrop: layout.backdrop.clone(), layout: Some(layout.id), ..Self::new(id) }
    }

    /// Remove deleted leaves and groups made empty by that deletion.
    /// Original empty groups are retained; this does not change child order.
    pub fn live_groups(&self,groups:&[ShapeGroup])->Vec<ShapeGroup> {
        fn keep(slide:&Slide,g:&ShapeGroup)->Option<ShapeGroup> {
            let mut out=g.clone();out.children=g.children.iter().filter_map(|c|match c {GroupMember::Shape(id)=>slide.shape(*id).map(|_|c.clone()),GroupMember::Group(inner)=>keep(slide,inner).map(|g|GroupMember::Group(Box::new(g)))}).collect();
            if !g.children.is_empty()&&out.children.is_empty()&&!g.has_unmodelled_children {None} else {Some(out)}
        }
        groups.iter().filter_map(|g|keep(self,g)).collect()
    }

    /// A hierarchy names each live leaf once; empty original groups may exist.
    pub fn valid_groups(&self,groups:&[ShapeGroup])->bool {
        fn visit(slide:&Slide,group:&ShapeGroup,ids:&mut std::collections::HashSet<u32>,leaves:&mut std::collections::HashSet<ShapeId>)->bool {
            let Some(id)=group.native_id.parse::<u32>().ok().filter(|&id|id>0) else {return false};
            if !ids.insert(id) || !group.name.chars().all(|c|matches!(c,'\t'|'\n'|'\r') || (c>=' ' && c!='\u{fffe}' && c!='\u{ffff}')) {return false;}
            group.children.iter().all(|child|match child {GroupMember::Shape(id)=>slide.shape(*id).is_some()&&!slide.is_attached(*id)&&leaves.insert(*id),GroupMember::Group(inner)=>visit(slide,inner,ids,leaves)})
        }
        let mut ids=std::collections::HashSet::new();let mut leaves=std::collections::HashSet::new();groups.iter().all(|g|visit(self,g,&mut ids,&mut leaves))
    }

    pub fn shape(&self, id: ShapeId) -> Option<&Shape> {
        self.shapes.iter().find(|s| s.id == id)
    }

    /// The backdrop drawn: none when hidden ([`Slide::hide_backdrop`]).
    pub fn shown_backdrop(&self) -> &[Shape] {
        if self.hide_backdrop { &[] } else { &self.backdrop }
    }

    /// The shapes drawn: all but those hidden ([`Slide::hidden_shapes`]).
    pub fn shown_shapes(&self) -> impl Iterator<Item = &Shape> {
        self.shapes.iter().filter(|s| !self.hidden_shapes.contains(&s.id))
    }

    pub fn shape_index(&self, id: ShapeId) -> Option<usize> {
        self.shapes.iter().position(|s| s.id == id)
    }

    /// The topmost shape under `p` (a shape attached to another one is
    /// part of it: see [`Slide::attached`]).
    pub fn hit_test(&self, p: EmuPoint) -> Option<ShapeId> {
        self.shown_shapes().collect::<Vec<_>>().into_iter().rev().filter(|s| !self.is_attached(s.id)).find(|s| s.frame.contains(p)).map(|s| s.id)
    }

    /// Whether `id` is part of another shape (see [`Slide::attached`]).
    pub fn is_attached(&self, id: ShapeId) -> bool {
        self.attached.values().any(|ids| ids.contains(&id))
    }

    /// The shapes attached to `id` that are on the slide.
    pub fn attached_to(&self, id: ShapeId) -> Vec<ShapeId> {
        self.attached.get(&id).map(|ids| ids.iter().copied().filter(|a| self.shape(*a).is_some()).collect()).unwrap_or_default()
    }
}

/// A PowerPoint section, with members in presentation order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    pub id: String,
    pub name: String,
    pub slides: Vec<SlideId>,
}

/// 書き込みパスワード (`p:modifyVerifier`): how the password was hashed and the hash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModifyVerifier {
    /// The hash's number (`cryptAlgorithmSid`: 12 SHA-256, 13 SHA-384, 14 SHA-512; 4, SHA-1, is read but not checked).
    pub algorithm_sid: u32,
    pub spin_count: u32,
    /// Base64, as in the file (`saltData`, `hashData`).
    pub salt: String,
    pub hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Presentation {
    pub slide_size: EmuSize,
    pub slides: Vec<Slide>,
    pub sections: Vec<Section>,
    pub media: Vec<Media>,
    pub fonts: Vec<EmbeddedFont>,
    /// A flowing document (Word), laid out into the slides as pages by the
    /// viewer.
    pub flow: Option<std::sync::Arc<crate::flow::FlowDocument>>,
    /// A workbook's (Excel's) own things beside its worksheets.
    pub workbook: Option<std::sync::Arc<crate::sheet::Workbook>>,
    /// The slide masters the layouts follow.
    pub masters: Vec<Master>,
    /// The notes page's size (`p:notesSz`).
    pub notes_size: EmuSize,
    /// What the notes pages carry in their header, date, footer and number.
    pub notes_marks: NotesMarks,
    /// Where the notes master puts those four (`hdr`, `dt`, `ftr`, `sldNum`).
    pub notes_frames: Vec<(String, EmuRect)>,
    /// The slide layouts a presentation's slides can follow.
    pub layouts: Vec<Layout>,
    /// The theme's fonts, for text with no typeface of its own (`None`:
    /// the app's default font).
    pub theme_fonts: Option<crate::text::ThemeFonts>,
    /// The theme's colour scheme (`dk1`, `lt1`, `dk2`, `lt2`, `accent1`…
    /// `accent6`, `hlink`, `folHlink`; presentations).
    pub theme_colors: Vec<(String, crate::drawing::Rgb)>,
    /// The number of the first slide, for slide number fields
    /// (`p:presentation/@firstSlideNum`).
    pub first_slide_number: usize,
    /// ヘッダーとフッター's marks for the whole presentation.
    pub header_footer: HeaderFooterMarks,
    /// How the layouts and masters in the model differ from their parts in
    /// the file (スライドのサイズを拡大縮小したとき): a writer scales the
    /// parts the same way.
    pub layout_scale: LayoutScale,
    /// スライド ショーの設定 (`p:showPr`).
    pub show_properties: ShowProperties,
    /// 目的別スライド ショーのリスト (`p:custShowLst`).
    pub custom_shows: Vec<CustomShow>,
    /// 最終版としてマーク（PowerPoint の「最終版にする」: `docProps/custom.xml` の `_MarkAsFinal`）。
    pub mark_as_final: bool,
    /// 読み取り専用を推奨（PowerPoint の「読み取り専用を推奨」: `p1710:readonlyRecommended`）。
    pub read_only_recommended: bool,
    /// 書き込みパスワード（`p:modifyVerifier`）: the hash of the password to modify the file.
    pub modify_verifier: Option<ModifyVerifier>,
    /// ドキュメント プロパティ（タイトル・作成者・カスタムプロパティ等: `docProps/core.xml`, `docProps/app.xml`, `docProps/custom.xml`）.
    pub properties: crate::flow::DocProperties,
    /// 既定の図形・線の書式（PowerPoint の「既定の図形に設定」「既定の線に設定」: `ppt/theme/theme1.xml` の `<a:objectDefaults>`）。
    pub object_defaults: ObjectDefaults,
    next_id: u64,
}

/// 既定の図形・線の書式設定（`<a:objectDefaults>`）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ObjectDefaults {
    pub shape: Option<ShapeDefault>,
    pub line: Option<Stroke>,
}

/// 既定の図形の書式（`<a:spDef>`）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShapeDefault {
    pub fill: Fill,
    pub stroke: Option<Stroke>,
    pub text: TextStyle,
    pub shadow: Option<crate::Shadow>,
    pub glow: Option<(Color, i64)>,
    pub reflection: Option<crate::Reflection>,
    pub soft_edge: Option<i64>,
}

/// A custom slide show (目的別スライド ショー, `<p:custShow>`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustomShow {
    /// The unique numeric identifier (`id="..."`).
    pub id: u32,
    /// The display name of this custom show (`name="..."`).
    pub name: String,
    /// The slides included in this custom show, in presentation order.
    pub slides: Vec<SlideId>,
}

/// The presentation show type (スライド ショーの種類).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ShowType {
    /// 発表者として使用する (フル スクリーン / 発表者ツール, <p:present/>).
    #[default]
    Presenter,
    /// 個人として閲覧する (ウィンドウ表示, <p:browse/>).
    Browse,
    /// キオスク端末で表示する (フル スクリーン・自動リスタート, <p:kiosk/>).
    Kiosk {
        /// 自動再起動までの時間（ミリ秒、既定は 300,000 = 5分）。None の場合は属性なしまたは既定値。
        restart: Option<u32>,
    },
}

/// The slide range to show (表示するスライド).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ShowSlideRange {
    /// すべてのスライド (<p:sldAll/>).
    #[default]
    All,
    /// 指定範囲 (<p:sldRg st="..." end="..."/>, 1-based index).
    Range {
        start: usize,
        end: usize,
    },
    /// 目的別スライド ショー (<p:custShow id="..."/>).
    Custom(u32),
}

/// Presentation show properties (`p:showPr`, スライド ショーの設定).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShowProperties {
    /// スライド ショーの種類 (<p:present/>, <p:browse/>, <p:kiosk/>).
    pub show_type: ShowType,
    /// Esc キーが押されるまで繰り返す (`loop="1"`).
    pub loop_continuously: bool,
    /// ナレーションなしで再生する (`showNarration="0"` なら false).
    pub show_narration: bool,
    /// アニメーションなしで再生する (`showAnimation="0"` なら false).
    pub show_animation: bool,
    /// タイミングを使用する (`useTimings="0"` なら false).
    pub use_timings: bool,
    /// 表示するスライド (<p:sldAll/>, <p:sldRg/>, <p:custShow/>).
    pub slide_range: ShowSlideRange,
    /// ペンの色 (<p:penClr>).
    pub pen_color: Option<crate::drawing::Rgb>,
}

impl Default for ShowProperties {
    fn default() -> Self {
        Self {
            show_type: ShowType::Presenter,
            loop_continuously: false,
            show_narration: true,
            show_animation: true,
            use_timings: true,
            slide_range: ShowSlideRange::All,
            pen_color: None,
        }
    }
}

/// A scaling of the slide canvas's contents: `x' = x * ppm / 1 000 000 +
/// dx` (and the same for `y`), sizes by `ppm` only. The identity is the
/// default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayoutScale {
    pub ppm: i64,
    pub dx: i64,
    pub dy: i64,
}

impl Default for LayoutScale {
    fn default() -> Self {
        Self { ppm: 1_000_000, dx: 0, dy: 0 }
    }
}

impl LayoutScale {
    pub fn is_identity(&self) -> bool {
        *self == Self::default()
    }
    fn scale(&self, v: i64) -> i64 {
        (v as i128 * self.ppm as i128 / 1_000_000) as i64
    }
    pub fn rect(&self, r: EmuRect) -> EmuRect {
        EmuRect { x: self.scale(r.x) + self.dx, y: self.scale(r.y) + self.dy, width: self.scale(r.width), height: self.scale(r.height) }
    }
    /// This scaling, then `next`.
    pub fn then(&self, next: LayoutScale) -> LayoutScale {
        LayoutScale { ppm: (self.ppm as i128 * next.ppm as i128 / 1_000_000) as i64, dx: next.scale(self.dx) + next.dx, dy: next.scale(self.dy) + next.dy }
    }
    /// The scaling that fits (`maximize`: fills) `from` into `to`, centred.
    pub fn between(from: EmuSize, to: EmuSize, maximize: bool) -> LayoutScale {
        let (sx, sy) = (to.width as f64 / from.width as f64, to.height as f64 / from.height as f64);
        let k = if maximize { sx.max(sy) } else { sx.min(sy) };
        let ppm = (k * 1_000_000.0).round() as i64;
        let dx = ((to.width as f64 - from.width as f64 * k) / 2.0).round() as i64;
        let dy = ((to.height as f64 - from.height as f64 * k) / 2.0).round() as i64;
        LayoutScale { ppm, dx, dy }
    }
}

impl Slide {
    /// The slide with its shapes and backdrop scaled.
    pub fn scaled(&self, by: LayoutScale) -> Slide {
        let mut s = self.clone();
        for shape in s.shapes.iter_mut().chain(s.backdrop.iter_mut()) {
            shape.frame = by.rect(shape.frame);
        }
        s
    }
}

impl Layout {
    pub fn scaled(&self, by: LayoutScale) -> Layout {
        let mut l = self.clone();
        for shape in &mut l.backdrop {
            shape.frame = by.rect(shape.frame);
        }
        for p in l.placeholders.iter_mut().chain(l.master_placeholders.iter_mut()) {
            if let Some(pl) = &mut p.placement {
                pl.0 = by.rect(pl.0);
            }
        }
        l
    }
}

/// ヘッダーとフッター's marks for the whole presentation (what PowerPoint's
/// すべてに適用 writes): whether title slides show the date, footer and
/// slide number (`p:presentation/@showSpecialPlsOnTitleSld`), and which of
/// them the masters show (their `p:hf`). They decide nothing drawn; the
/// slides' own placeholders do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeaderFooterMarks {
    pub on_title_slides: bool,
    pub header: bool,
    pub date: bool,
    pub footer: bool,
    pub number: bool,
}

/// The date on the notes pages (ノートと配布資料のヘッダーとフッター).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotesDate {
    /// 自動的に更新: a field, with the text it showed when it was made.
    Auto(String),
    /// 固定: the text typed.
    Fixed(String),
}

/// What the notes pages carry in their header, date, footer and page
/// number placeholders (ヘッダーとフッター > ノートと配布資料).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NotesMarks {
    pub header: Option<String>,
    pub date: Option<NotesDate>,
    pub footer: Option<String>,
    pub number: bool,
}

impl NotesMarks {
    /// Whether the notes pages carry nothing.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

impl Default for HeaderFooterMarks {
    fn default() -> Self {
        Self { on_title_slides: true, header: true, date: true, footer: true, number: true }
    }
}

impl Presentation {
    /// The size of `slide`: its own, or the presentation's.
    pub fn size_of(&self, slide: &Slide) -> EmuSize {
        slide.size.unwrap_or(self.slide_size)
    }

    /// Sections cover the slide list in order; empty sections are permitted.
    pub fn valid_sections(&self, sections: &[Section]) -> bool {
        if sections.is_empty() {return true;}
        let mut ids=std::collections::HashSet::new();
        sections.iter().all(|section| {
            let id=section.id.as_bytes();
            id.len()==38 && id[0]==b'{' && id[37]==b'}'
                && id[1..37].iter().enumerate().all(|(i,b)|if [8,13,18,23].contains(&i) {*b==b'-'} else {b.is_ascii_hexdigit()})
                && ids.insert(section.id.to_ascii_uppercase())
                && section.name.chars().all(|c|matches!(c,'\t'|'\n'|'\r') || (c>=' ' && c!='\u{fffe}' && c!='\u{ffff}'))
        }) && sections.iter().flat_map(|section|section.slides.iter().copied()).eq(self.slides.iter().map(|slide|slide.id))
    }

    /// Follow the existing section boundaries when the slide list changes.
    pub(crate) fn reconcile_sections(&mut self) {
        if self.sections.is_empty() {return;}
        let membership=self.sections.iter().enumerate().flat_map(|(i,s)|s.slides.iter().map(move |&id|(id,i))).collect::<std::collections::HashMap<_,_>>();
        for section in &mut self.sections {section.slides.clear();}
        let mut at=0;for slide in &self.slides {if let Some(&i)=membership.get(&slide.id) {at=at.max(i);}self.sections[at].slides.push(slide.id);}
    }

    /// Whether any sheet in the presentation contains an Excel table (`<table>`).
    pub fn has_tables(&self) -> bool {
        self.slides.iter().any(|s| {
            s.shapes.iter().any(|sh| {
                if let ShapeKind::Sheet(sheet) = &sh.kind {
                    !sheet.tables.is_empty()
                } else {
                    false
                }
            })
        }) || self.workbook.as_ref().map_or(false, |wb| wb.has_tables())
    }

    /// Whether the workbook's window is hidden (`<bookViews><workbookView visibility="hidden"/>`).
    pub fn is_workbook_window_hidden(&self) -> bool {
        self.workbook.as_ref().map_or(false, |wb| wb.window_hidden)
    }

    /// Whether the workbook's windows are locked by workbook protection (`<workbookProtection lockWindows="1"/>`).
    pub fn is_workbook_windows_locked(&self) -> bool {
        self.workbook.as_ref().and_then(|wb| wb.protection.as_ref()).is_some_and(|p| p.lock_windows)
    }

    /// Whether the workbook's structure is locked by workbook protection (`<workbookProtection lockStructure="1"/>`).
    pub fn is_workbook_structure_locked(&self) -> bool {
        self.workbook.as_ref().and_then(|wb| wb.protection.as_ref()).is_some_and(|p| p.lock_structure)
    }

    /// A widescreen presentation with one empty slide.
    pub fn new() -> Self {
        let mut p = Self::empty(EmuSize::WIDESCREEN);
        let id = p.new_slide_id();
        p.slides.push(Slide::new(id));
        p
    }

    /// A presentation with no slides yet, for a reader to fill. It must end up
    /// with at least one slide.
    pub fn empty(slide_size: EmuSize) -> Self {
        Self { slide_size, slides: Vec::new(), sections: Vec::new(), media: Vec::new(), fonts: Vec::new(), flow: None, workbook: None, notes_size: EmuSize::new(6_858_000, 9_144_000), notes_marks: NotesMarks::default(), notes_frames: Vec::new(), masters: Vec::new(), layouts: Vec::new(), theme_fonts: None, theme_colors: Vec::new(), first_slide_number: 1, header_footer: HeaderFooterMarks::default(), layout_scale: LayoutScale::default(), show_properties: ShowProperties::default(), custom_shows: Vec::new(), mark_as_final: false, read_only_recommended: false, modify_verifier: None, properties: crate::flow::DocProperties::default(), object_defaults: ObjectDefaults::default(), next_id: 1 }
    }

    /// Look up a custom show by its numeric identifier.
    pub fn custom_show(&self, id: u32) -> Option<&CustomShow> {
        self.custom_shows.iter().find(|s| s.id == id)
    }

    /// Look up a custom show by its name.
    pub fn custom_show_by_name(&self, name: &str) -> Option<&CustomShow> {
        self.custom_shows.iter().find(|s| s.name == name)
    }

    /// Generate an unused custom show id.
    pub fn new_custom_show_id(&self) -> u32 {
        let mut id = 0;
        while self.custom_shows.iter().any(|s| s.id == id) {
            id += 1;
        }
        id
    }

    /// Allocate a slide id. Ids are never reused within a document.
    ///
    /// In collaborative editing each client will need its own id space (for
    /// example a client id in the high bits); this counter is the local part.
    pub fn new_slide_id(&mut self) -> SlideId {
        SlideId(self.next_id())
    }

    pub fn new_shape_id(&mut self) -> ShapeId {
        ShapeId(self.next_id())
    }

    pub fn new_master_id(&mut self) -> MasterId {
        MasterId(self.next_id())
    }

    pub fn new_layout_id(&mut self) -> LayoutId {
        LayoutId(self.next_id())
    }

    /// Set the text of every slide number field (on the slides and their
    /// backdrops) to the number of its slide, after slides are added,
    /// removed or moved.
    /// Date fields (日付と時刻, 自動的に更新) set to `t`, as PowerPoint
    /// shows them when it opens the file; whether any changed.
    pub fn refresh_dates(&mut self, t: &crate::date_field::LocalTime, segments: &dyn Fn(&str) -> Vec<(String, bool)>) -> bool {
        let mut changed = false;
        for slide in &mut self.slides {
            for shape in slide.shapes.iter_mut().chain(slide.backdrop.iter_mut()) {
                if let Some(body) = shape.text_body_mut() {
                    changed |= body.set_dates(t, segments);
                }
                if let ShapeKind::Table(table) = &mut shape.kind {
                    for cell in table.rows.iter_mut().flat_map(|r| r.cells.iter_mut()) {
                        changed |= cell.text.set_dates(t, segments);
                    }
                }
            }
        }
        changed
    }

    pub fn renumber_slides(&mut self) {
        let first = self.first_slide_number;
        for (i, slide) in self.slides.iter_mut().enumerate() {
            renumber(slide, i + first);
        }
    }

    /// A new empty slide `id` following the layout of slide `like` (as
    /// PowerPoint's "New Slide" does), or a plain one if it has none.
    pub fn new_slide_like(&self, id: SlideId, like: SlideId) -> Slide {
        match self.slide(like).and_then(|s| s.layout).and_then(|l| self.layout(l)) {
            Some(layout) => Slide::from_layout(id, layout),
            None => Slide::new(id),
        }
    }

    /// A copy of slide `slide` as slide `id`. Its shapes keep their ids
    /// (ids are unique on a slide), which tells a writer the copy's
    /// original.
    pub fn duplicate_slide(&self, slide: SlideId, id: SlideId) -> Option<Slide> {
        self.slide(slide).map(|s| Slide { id, ..s.clone() })
    }

    /// Slide `slide` following layout `layout` instead (レイアウトの適用):
    /// the layout's backdrop, its background if the slide had its old
    /// layout's, and its placeholders' places for the slide's placeholders
    /// that take them (one the new layout has no place for keeps its own).
    pub fn with_layout(&self, slide: SlideId, layout: LayoutId) -> Option<Slide> {
        let old = self.slide(slide)?;
        let new_layout = self.layout(layout)?;
        let mut out = old.clone();
        out.layout = Some(layout);
        // A slide that hides its layout's and master's shapes keeps hiding them.
        let old_layout = old.layout.and_then(|l| self.layout(l));
        let hidden = old.backdrop.is_empty() && old_layout.is_some_and(|l| !l.backdrop.is_empty());
        if old.layout != Some(layout) {
            out.backdrop = if hidden { Vec::new() } else { new_layout.backdrop.clone() };
        }
        let old_background = old_layout.map(|l| &l.background);
        if old_background.is_none_or(|b| *b == old.background) {
            out.background = new_layout.background.clone();
        }
        for shape in &mut out.shapes {
            let Some(ph) = out.placeholders.get_mut(&shape.id) else { continue };
            if ph.own_frame {
                continue;
            }
            match new_layout.placement_for(&ph.placeholder) {
                Some((frame, rotation, flip_h, flip_v)) => {
                    shape.frame = frame;
                    shape.rotation = rotation;
                    shape.flip_h = flip_h;
                    shape.flip_v = flip_v;
                }
                None => ph.own_frame = true,
            }
        }
        let index = self.slide_index(slide)?;
        renumber(&mut out, index + self.first_slide_number);
        Some(out)
    }

    /// Reset only placeholders' placement to their current layout/master.
    /// Text, character formats and unrelated shapes are kept.
    pub fn reset_slide_frames(&self, id: SlideId) -> Option<Slide> {
        let mut slide = self.slide(id)?.clone();
        let layout = self.layout(slide.layout?)?;
        for shape in &mut slide.shapes {
            let Some(ph) = slide.placeholders.get_mut(&shape.id) else { continue };
            let Some((frame, rotation, flip_h, flip_v)) = layout.placement_for(&ph.placeholder) else { continue };
            shape.frame = frame;
            shape.rotation = rotation;
            shape.flip_h = flip_h;
            shape.flip_v = flip_v;
            ph.own_frame = false;
        }
        Some(slide)
    }

    /// Reset placeholders' placement, shape styling, and text formatting to their layout defaults (スライドのリセット).
    /// Text content and unrelated user-added shapes are preserved.
    pub fn reset_slide(&self, id: SlideId) -> Option<Slide> {
        let mut slide = self.slide(id)?.clone();
        let layout = self.layout(slide.layout?)?;
        for shape in &mut slide.shapes {
            let Some(ph) = slide.placeholders.get_mut(&shape.id) else { continue };
            if let Some((frame, rotation, flip_h, flip_v)) = layout.placement_for(&ph.placeholder) {
                shape.frame = frame;
                shape.rotation = rotation;
                shape.flip_h = flip_h;
                shape.flip_v = flip_v;
                ph.own_frame = false;
            }
            if let ShapeKind::Auto(auto) = &mut shape.kind {
                if let Some(text) = &mut auto.text {
                    for para in &mut text.paragraphs {
                        for run in &mut para.runs {
                            run.style = RunStyle::default();
                        }
                    }
                }
                auto.shadow = None;
                auto.glow = None;
                auto.inner_shadow = None;
                auto.soft_edge = None;
                auto.reflection = None;
                auto.bevel = None;
            }
        }
        Some(slide)
    }

    pub fn master(&self, id: MasterId) -> Option<&Master> {
        self.masters.iter().find(|m| m.id == id)
    }

    pub fn layout(&self, id: LayoutId) -> Option<&Layout> {
        self.layouts.iter().find(|l| l.id == id)
    }

    fn next_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Store a picture file and return its id.
    pub fn add_media(&mut self, format: MediaFormat, data: impl Into<Arc<[u8]>>) -> MediaId {
        let id = MediaId(self.next_id());
        self.media.push(Media { id, format, data: data.into() });
        id
    }

    pub fn media(&self, id: MediaId) -> Option<&Media> {
        self.media.iter().find(|m| m.id == id)
    }

    pub fn slide(&self, id: SlideId) -> Option<&Slide> {
        self.slides.iter().find(|s| s.id == id)
    }

    pub fn slide_mut(&mut self, id: SlideId) -> Option<&mut Slide> {
        self.slides.iter_mut().find(|s| s.id == id)
    }

    pub fn slide_index(&self, id: SlideId) -> Option<usize> {
        self.slides.iter().position(|s| s.id == id)
    }

    /// A text box frame the size PowerPoint gives a click-inserted text box,
    /// centred on `at` and kept inside the slide.
    pub fn default_text_box_frame(&self, at: EmuPoint) -> EmuRect {
        let width = EMU_PER_INCH * 3;
        let height = EMU_PER_INCH / 2;
        let x = (at.x - width / 2).clamp(0, (self.slide_size.width - width).max(0));
        let y = (at.y - height / 2).clamp(0, (self.slide_size.height - height).max(0));
        EmuRect::new(x, y, width, height)
    }
}

/// Set slide `slide`'s slide number fields to `number`.
fn renumber(slide: &mut Slide, number: usize) {
    for shape in slide.shapes.iter_mut().chain(slide.backdrop.iter_mut()) {
        if let Some(body) = shape.text_body_mut() { body.set_slide_number(number); }
        if let ShapeKind::Table(table) = &mut shape.kind {
            for row in &mut table.rows { for cell in &mut row.cells { cell.text.set_slide_number(number); } }
        }
    }
}

impl Default for Presentation {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_test_returns_topmost() {
        let mut p = Presentation::new();
        let a = p.new_shape_id();
        let b = p.new_shape_id();
        let slide = &mut p.slides[0];
        slide.shapes.push(Shape::text_box(a, EmuRect::new(0, 0, 100, 100), "a"));
        slide.shapes.push(Shape::text_box(b, EmuRect::new(50, 50, 100, 100), "b"));
        assert_eq!(slide.hit_test(EmuPoint::new(60, 60)), Some(b));
        assert_eq!(slide.hit_test(EmuPoint::new(10, 10)), Some(a));
        assert_eq!(slide.hit_test(EmuPoint::new(500, 500)), None);
    }

    #[test]
    fn default_text_box_stays_on_slide() {
        let p = Presentation::new();
        let r = p.default_text_box_frame(EmuPoint::new(0, 0));
        assert_eq!((r.x, r.y), (0, 0));
        let r = p.default_text_box_frame(EmuPoint::new(p.slide_size.width, p.slide_size.height));
        assert_eq!(r.right(), p.slide_size.width);
        assert_eq!(r.bottom(), p.slide_size.height);
    }

    #[test]
    fn media_ids_do_not_clash_with_shapes() {
        let mut p = Presentation::new();
        let media = p.add_media(MediaFormat::Png, vec![1, 2, 3]);
        let shape = p.new_shape_id();
        assert_ne!(media.0, shape.0);
        assert_eq!(p.media(media).map(|m| &*m.data), Some(&[1u8, 2, 3][..]));
    }

    #[test]
    fn media_format_from_extension() {
        assert_eq!(MediaFormat::from_path("ppt/media/image1.JPEG"), MediaFormat::Jpeg);
        assert_eq!(MediaFormat::from_path("ppt/media/image2.emf"), MediaFormat::Emf);
    }
}
