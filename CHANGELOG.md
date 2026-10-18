# Changelog

All notable changes to ibisOffice are recorded in this file.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the versions follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.8.2] - 2026-10-18

### Added

#### All apps

- A desktop app for Linux (64-bit x86_64; Ubuntu 20.04, Debian 11 or later): `ibisOffice-<version>-Linux.AppImage`. It needs no installation and no FUSE: at the first start of each version it unpacks itself into `~/.cache/ibisOffice/app/`, and starts from there afterwards.
  - At its first start, ibisOffice offers to add itself to your list of apps; it then also appears under "Open With Other Application" for Word, Excel and PowerPoint documents.
  - Automatic update on Linux: the new AppImage is checked against its SHA-256 and takes the place of the one you run. The log is `~/.local/state/ibisOffice/updater.log`.
  - Exporting to PNG and PDF from the command line (`--export`) works on Linux too.
  - Where Office's fonts are not installed, fonts with the same widths are used: Liberation (Times New Roman, Arial, Courier New), Caladea (Cambria), Carlito (Calibri, bundled) and Noto CJK (Japanese).
- Pinch to zoom with two fingers on a macOS trackpad, around the pointer, in Word, Excel and PowerPoint (#2).
- Linux and Windows: the title bar has Minimize, Maximize and Close buttons, and you can drag the window by it (#7).
- The font list is built from the fonts installed on your computer and the bundled fonts. It has a "Recent" section and is faster to open (#5).
- The proofing language and the AutoCorrect options are shared by Word, Excel and PowerPoint. Spell checking, with the same panel, now works in Word and Excel too.
- Auto-recovery saves your work regularly in Word, Excel and PowerPoint.
- Mac keyboard shortcuts while editing text (⌘←, ⌘→, ⌃A, ⌃E and others) and View shortcuts for zoom.
- Split toolbar buttons where Office has them (bullets, numbering, underline, shading, borders, Merge & Center, AutoSum, New Slide).
- A command-line option, `--layout`, now also writes where the text is drawn and whether it overflows its frame (#8).
- Finder's "Open With" offers ibisOffice for .csv, .tsv, .odp, .otp, .ppt, .pps, .pot, .xls, .xlt, .ods and .ots files. ibisOffice does not take over the default app.

#### Word (docx)

- Mail Merge: address block and greeting line, rules (If…Then…Else, Compare, Skip Record If, Next Record), adding, removing and filtering recipients, preview and find, and merging to separate files.
- Equations: Word equations (OMML) are laid out and drawn, and `Eq` fields are drawn as equations.
- Shapes: draw shapes by dragging (Insert > Shapes), type text in them, elbow and curved connectors that stay attached when you move a shape, and line, gradient, shadow, glow, soft edge and reflection settings. VML shapes from older documents are drawn.
- Review: Compare Documents (tables cell by cell, and the 8 compare options), a grammar and style checker with its Review panel, open passwords, and Next/Previous Footnote and Comment.
- Index (XE, Index), Table of Authorities and Table of Contents entries (TC, TA, TOA), caption numbers with chapter numbers, and outline collapse and expand.
- Content controls: title, tag, lock and placeholder text. Document templates: Save as .dotx and File > New from Template.
- Text effects: Word 2010 outline, glow, shadow and gradient fills, shading patterns, and wavy borders. Picture bullets, a blank page, and the Dark Mode page.
- Editing: AutoFormat as you type (smart quotes, automatic bullets and borders), editing in several areas at once, and "Always create backup copy".
- Charts: Insert > Chart, editing chart data and changing the chart type.

#### Excel (xlsx)

- Open Excel Binary Workbooks (.xlsb) and password-protected Excel 97-2003 workbooks (.xls); save as Excel 97-2003 (.xls) and OpenDocument (.ods). Open and save .ods charts, shapes, pictures, conditional formats, data validation and comments.
- Charts: Waterfall, Histogram, Pareto, Funnel, Box & Whisker, Treemap and Sunburst (Insert > Chart); 3D charts; radar, surface, stock, combo and bar/pie-of-pie charts; chart sheets that follow cell edits and Undo. Charts in older .xls files are read.
- Slicers and timelines, Form Controls (10 kinds), SmartArt, and the Solver and Data Analysis tools (Data > Data Analysis…, 19 tools).
- Page Layout view with rulers, headers and footers you type directly, and column and row headings on each page.
- IMAGE function (pictures from a URL), structured references shown as `[@Column]`, FORECAST.ETS and related functions, and "Forecast Sheet".
- Links to other workbooks: a bar at startup (Update Values, Edit Links), and formulas that point to another workbook add the link.
- Text/CSV import with fixed width, a new sheet and a table preview, and a Query Editor.
- Large sheets: moving, PageDown and editing up to row 1,048,576, with a row header that widens with the number of digits. The grid grows as you scroll past the used area.
- Editing: Insert and Delete Cells in one dialog, Paste Special with options, Pick from Drop-down List, Find and Replace options, row and column size dialogs, double-click a row border to AutoFit, and more Freeze Panes options.
- Excel 2010 data bars and icon sets are read and drawn. Number formats follow the locale in `[$-…]` (month and day names in German, French, Spanish, Italian, Portuguese, Korean and Chinese).

#### PowerPoint (pptx)

- Open PowerPoint 97-2003 presentations (.ppt, also password-protected) and OpenDocument presentations (.odp, .otp), and save as .odp. Animations, shadows and cropped pictures in .ppt files are read.
- Equations: Insert > Equation… with linear UnicodeMath, laid out in 2D as in PowerPoint (fractions, roots, matrices, equation arrays), and editing inside an equation.
- Outline view: edit text, change levels, move and merge slides, and select several lines.
- Notes and Slide Master: the Notes pane has an on/off button and a close button, panes are resized by dragging, and the Notes Master and Handout Master are edited and saved. Slide thumbnails and the slide canvas have fuller right-click menus.
- Draw tab: pens, highlighters, eraser, ink to shape, and the Ruler.
- WordArt: 41 kinds (text warp presets), drawn as in PowerPoint. 3D bevels on shapes, EMF, WMF and EMF+ pictures, and curve handles in Edit Points.
- Chart types Box & Whisker, Treemap, Sunburst, Waterfall and Pareto, with data labels, axes and fills.

### Changed

- Word and PowerPoint: 100% zoom now shows 1 pt as 1 px, the same as Office for Mac.
- Excel: the formula bar works like Excel's (a cell with line breaks shows one line; ▼ expands it to four lines). The zoom percentage in the status bar is one button, and the Fit button is removed.
- Excel: the cell editing box is filled with the cell color, shows no green selection border, and keeps the cell's alignment.
- Excel: chart axes follow where they cross (`crosses`, `crossesAt`), and date axes thin out and rotate their labels as Excel does.
- PowerPoint: backgrounds and theme colors changed in Slide Master, Notes Master and Handout Master view are saved back to the file.
- Excel: more financial functions match Excel (CUMPRINC, CUMIPMT, PRICEMAT, YIELDMAT, DURATION, ODDFPRICE, ODDLPRICE, FV, AMORDEGRC, VDB).
- Printing and PDF export: text larger than 32 pt is not drawn when it is more than 1.3 times the row height, as in Excel.
- On macOS, the Start, Undo and Redo buttons no longer overlap the window buttons.

### Fixed

- Excel: a cell with many line breaks (for example in a large CSV file) is no longer drawn over the toolbar and the formula bar while you edit it, and its row stops at the maximum height.
- Excel: pressing Enter while editing confirms the cell and moves to the next one; a long pasted text stops at the maximum row height (409.5 pt); lines in the editing box stack vertically.
- Excel: the font list is no longer drawn black. Dragging a row border stops at the maximum height. Moving the caret no longer shifts the surrounding text (Word, Excel and PowerPoint).
- Excel: rows whose height is not set no longer grow for wrapped text in merged cells; CELL("filename") and CELL("address") work per workbook; AutoFilter and page breaks in .xls files are read.
- Word: borders, text boxes, glow, line spacing and numbering characters (Korean, Chinese, Hindi) are closer to Word's layout. Saving a new header with a table no longer fails.
- PowerPoint: slides pasted from other presentations no longer carry custom XML, and 3D rotation angles are saved in a range PowerPoint accepts. Picture tiles, gradient paths and text on a circle are closer to PowerPoint.
- Web: a build error in the playground is fixed; the sample date format is corrected.
- Linux: window buttons and dragging work with client-side title bars; build errors on Linux and Wayland are fixed.

## [0.8.1] - 2026-10-12

### Added

- A desktop app for Windows (64-bit): `ibisOffice-0.8.1-Windows.zip`.
- Automatic update (macOS and Windows). At startup, ibisOffice checks for a newer version, downloads it and asks whether to update. A separate program, `ibisOfficeUpdater`, replaces the app and opens the new version again. If the new version does not start, it puts the old version back.
  - The new macOS app is installed only if it is signed by ibis inc. and notarized by Apple. On Windows, the download is checked against its SHA-256.
  - If an update fails, a button opens the download page.
  - Each step of an update is written to a log (`~/Library/Logs/ibisOffice/updater.log` on macOS, `%LOCALAPPDATA%\ibisOffice\Logs\updater.log` on Windows). Help > Show Update Log opens it.
  - Settings: "Check for updates at startup" (on by default).
  - After two failed attempts at the same version, ibisOffice stops offering that version and asks you to download it again.
- Color palettes, as in Microsoft Office (theme colors and standard colors), for the text, fill, highlight and border colors in Word, Excel and PowerPoint documents.
- Split toolbar buttons for colors: the left half applies the color you used last, and the ▼ opens the palette.
- Excel: a new Format Cells pane.
  - Number, Alignment, Font, Border, Fill and Protection are groups that you open and close.
  - Number formats, horizontal and vertical alignment, text direction, line styles and colors are drop-down lists. Each color in a list is shown as a swatch.
  - The font size has − and + buttons. Bold, Italic, Underline and Strikethrough are toolbar buttons.
  - Borders work as in Excel: presets (None, Outline, Inside), a button for each edge and diagonal, and a preview cell. Click an edge of the preview to add or remove a line.
- Excel: spell checking (Tools > Spelling), with the same panel as PowerPoint.

### Changed

- PowerPoint: the right pane starts closed when you create a presentation or open a .pptx file.
- Excel: the first time you open the right pane in a workbook, it shows the Format Cells tab.
- Excel: File > New > New from Template… is now the last item.
- Excel: "Open and Repair" is now "Repair Damaged Workbook…". It is unavailable when there is no workbook that failed to open.
- The Settings and About windows open centered on the window they were opened from, on the same display.
- Menus no longer show submenus that have nothing in them. The Format menu's text, alignment, spacing, bullets and borders submenus now have the same commands as the toolbar. In PowerPoint, Align, Center on Page, Distribute and Effect Options are shown dimmed when nothing is selected.

## [0.8.0] - 2026-10-12

The first public release.

### Added

- Open, edit and save Word (.docx), Excel (.xlsx) and PowerPoint (.pptx) documents, drawn on the GPU to look as they do in Microsoft Office.
- A desktop app for macOS 11 or later (Apple silicon).
- A web version that runs in the browser with WebAssembly and WebGPU, with nothing installed and no files uploaded: [playground](https://ibisjp.github.io/ibis-office/).
- The user interface in English and Japanese.
- Dual licensing: AGPL-3.0-only, or a commercial license from ibis inc.

[Unreleased]: https://github.com/ibisjp/ibis-office/compare/v0.8.2...HEAD
[0.8.2]: https://github.com/ibisjp/ibis-office/compare/v0.8.1...v0.8.2
[0.8.1]: https://github.com/ibisjp/ibis-office/compare/v0.8.0...v0.8.1
[0.8.0]: https://github.com/ibisjp/ibis-office/releases/tag/v0.8.0
