#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (c) ibis inc.
# Additional terms under AGPL-3.0 section 7(b): see NOTICE.
"""Remove what identifies people and machines from an Office file (.pptx,
.xlsx, .docx), and from the workbooks embedded in it, keeping everything
that is drawn.

    scripts/benchmark-data/scrub_ooxml.py [--drop-chart-data] <in> <out>

Removed or emptied:
- the author, the last editor and the last print date (docProps/core.xml),
  the company and manager (docProps/app.xml)
- custom properties (docProps/custom.xml: SharePoint content type IDs and
  the like)
- add-ins registered with the file (ppt/webextensions, xl/webextensions)
- in presentations: the comment authors (ppt/authors.xml: name, initials,
  user ID and provider ID, kept as one anonymous author so the comments'
  author IDs still resolve)
- in the EMF pictures (Adobe Illustrator's EPS comment inside): the name
  after %%For: and the folder path of %%DocumentFiles:, overwritten in place
  with "x" of the same length, so the records, their sizes and the drawing
  stay as they were
- printer settings (xl/printerSettings: the printer and user names), with the
  r:id of the page setup that pointed to them
- in workbooks: comments and threaded comments with their people (names,
  account and directory IDs), the save history ID (xr:revisionPtr) and
  absolute paths (x15ac:absPath), and who last refreshed a PivotTable
  (pivotCacheDefinition refreshedBy)
- with --drop-chart-data: the workbooks charts keep their data in (for
  editing it; the chart is drawn from the values it caches), which may hold
  working notes and data the slides do not show
"""
import io
import re
import sys
import zipfile

DROP = re.compile(
    r'^(docProps/custom\.xml|(ppt|xl|word)/webextensions/.*|xl/persons/.*|xl/threadedComments/.*'
    r'|xl/printerSettings/.*|xl/comments\d*\.xml|xl/drawings/vmlDrawing\d*\.vml|xl/drawings/_rels/vmlDrawing\d*\.vml\.rels)$'
)
DROPPED_REL_TYPES = ('custom-properties', 'webextensiontaskpanes', '/comments', 'threadedComment', 'vmlDrawing', '/person', '/printerSettings')


def scrub_core(xml: str) -> str:
    xml = re.sub(r'<dc:creator>.*?</dc:creator>', '<dc:creator></dc:creator>', xml, flags=re.S)
    xml = re.sub(r'<cp:lastModifiedBy>.*?</cp:lastModifiedBy>', '<cp:lastModifiedBy></cp:lastModifiedBy>', xml, flags=re.S)
    return re.sub(r'<cp:lastPrinted>.*?</cp:lastPrinted>', '', xml, flags=re.S)


def scrub_app(xml: str) -> str:
    xml = re.sub(r'<Company>.*?</Company>', '<Company></Company>', xml, flags=re.S)
    return re.sub(r'<Manager>.*?</Manager>', '<Manager></Manager>', xml, flags=re.S)


def scrub_authors(xml: str) -> str:
    xml = re.sub(r'(<p188:author [^>]*?)name="[^"]*"', r'\1name="Author"', xml)
    xml = re.sub(r'(<p188:author [^>]*?)initials="[^"]*"', r'\1initials="A"', xml)
    xml = re.sub(r'(<p188:author [^>]*?)userId="[^"]*"', r'\1userId="anonymous"', xml)
    return re.sub(r'(<p188:author [^>]*?)providerId="[^"]*"', r'\1providerId="None"', xml)


EMF_FOR = re.compile(rb'(%%For: \()([^)\r\n]*)(\))')
EMF_FILES = re.compile(rb'(%%DocumentFiles:)([^\r\n\x00]*/)([^/\r\n\x00]*)')


def scrub_emf(data: bytes) -> bytes:
    """Overwrite, in place and at the same length, the user name and the
    folder path Illustrator wrote in the EPS comment of an EMF."""
    data = EMF_FOR.sub(lambda m: m.group(1) + b'x' * len(m.group(2)) + m.group(3), data)
    return EMF_FILES.sub(lambda m: m.group(1) + b'/' + b'x' * (len(m.group(2)) - 1) + m.group(3), data)


def scrub_rels(xml: str) -> str:
    def keep(m):
        return '' if any(t in m.group(0) for t in DROPPED_REL_TYPES) else m.group(0)
    return re.sub(r'<Relationship [^>]*/>', keep, xml)


def scrub_content_types(xml: str) -> str:
    def keep(m):
        return '' if DROP.match(m.group(1).lstrip('/')) else m.group(0)
    return re.sub(r'<Override PartName="([^"]*)"[^>]*/>', keep, xml)


def scrub_workbook(xml: str) -> str:
    xml = re.sub(r'<xr:revisionPtr [^>]*/>', '', xml)
    return re.sub(r'<mc:AlternateContent[^>]*>(?:(?!</mc:AlternateContent>).)*x15ac:absPath.*?</mc:AlternateContent>', '', xml, flags=re.S)


def scrub_sheet(xml: str) -> str:
    # The comments' drawing goes with the comments.
    xml = re.sub(r'<legacyDrawing [^>]*/>', '', xml)
    # The printer settings (the printer's and the user's names) go too.
    return re.sub(r'(<pageSetup\b[^>]*?) r:id="[^"]*"', r'\1', xml)


CHART_DATA = re.compile(r'^(ppt|xl|word)/embeddings/[^/]+\.xlsx$')


def drop_chart_data_rels(xml: str) -> str:
    return re.sub(r'<Relationship [^>]*relationships/package"[^>]*/>|<Relationship [^>]*Target="[^"]*embeddings/[^"]*\.xlsx"[^>]*/>', '', xml)


def scrub(data: bytes, drop_chart_data: bool = False) -> bytes:
    src = zipfile.ZipFile(io.BytesIO(data))
    out = io.BytesIO()
    with zipfile.ZipFile(out, 'w', zipfile.ZIP_DEFLATED) as dst:
        for info in src.infolist():
            name = info.filename
            if DROP.match(name) or (drop_chart_data and CHART_DATA.match(name)):
                continue
            body = src.read(name)
            if name.endswith(('.xlsx', '.xlsm', '.docx', '.pptx')):
                body = scrub(body)
            elif name.endswith('.emf'):
                body = scrub_emf(body)
            elif name.endswith(('.xml', '.rels')):
                text = body.decode('utf-8')
                if name == 'docProps/core.xml':
                    text = scrub_core(text)
                elif name == 'ppt/authors.xml':
                    text = scrub_authors(text)
                elif name == 'docProps/app.xml':
                    text = scrub_app(text)
                elif name == '[Content_Types].xml':
                    text = scrub_content_types(text)
                elif name.endswith('.rels'):
                    text = scrub_rels(text)
                elif name == 'xl/workbook.xml':
                    text = scrub_workbook(text)
                elif name.startswith('xl/worksheets/sheet'):
                    text = scrub_sheet(text)
                elif name.startswith('xl/pivotCache/pivotCacheDefinition'):
                    # who last refreshed the PivotTable (refreshedBy="<user name>")
                    text = re.sub(r'refreshedBy="[^"]*"', 'refreshedBy=""', text)
                if drop_chart_data and re.match(r'^(ppt|xl|word)/charts/', name):
                    if name.endswith('.rels'):
                        text = drop_chart_data_rels(text)
                    else:
                        text = re.sub(r'<c:externalData [^>]*>.*?</c:externalData>|<c:externalData [^>]*/>', '', text, flags=re.S)
                body = text.encode('utf-8')
            info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            dst.writestr(info, body)
    return out.getvalue()


if __name__ == '__main__':
    args = sys.argv[1:]
    drop = '--drop-chart-data' in args
    src, dst = [a for a in args if not a.startswith('--')]
    open(dst, 'wb').write(scrub(open(src, 'rb').read(), drop))
