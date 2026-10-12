#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (c) ibis inc.
# Additional terms under AGPL-3.0 section 7(b): see NOTICE.
"""Write a heavy workbook for benchmarks, with made-up data: daily sales
of a fictional coffee chain ("Nimbus Coffee") by category, region and
channel, on many data sheets, and one sheet ("Overview") of charts over
all of them, thousands of points each (as heavy as the internal report the
benchmark first used, but nothing like its data).

    scripts/benchmark-data/gen_xlsx.py test/benchmark/coffee-chain-sales-report.xlsx

The OOXML is written directly, with each chart caching its values (as
Excel saves them), so any reader can draw the charts without calculating.
As in the internal report, the daily sheets are formulas over raw sheets
(SUMIFS over whole columns, the average ticket divided across sheets,
about half a million in all) with their values cached, and the workbook is marked as
calculated by the engine that report was (calcId 191028). Excel for Mac
(16.113) calculates the formulas again whenever it opens the workbook
(checked with cached values set wrong: whatever the calcId, with or
without calcChain.xml, even for a file Excel itself saved), without asking
to save on closing; that calculation is what makes the workbook slow to
open.
The same file comes out every time.
"""
import datetime
import math
import random
import sys
import zipfile
from xml.sax.saxutils import escape

START = datetime.date(2020, 1, 1)
END = datetime.date(2026, 7, 31)
DAYS = (END - START).days + 1
EPOCH = datetime.date(1899, 12, 30)
SERIALS = [(START - EPOCH).days + i for i in range(DAYS)]

REGIONS = ['Northfield', 'Eastbay', 'Southport', 'Westridge', 'Lakeside', 'Hillcrest', 'Riverside', 'Harborview']
CHANNELS = ['Store', 'Online']
CATEGORIES = ['Beverages', 'Food', 'Merchandise', 'Beans']
COLORS = ['4472C4', 'ED7D31', 'A5A5A5', 'FFC000', '5B9BD5', '70AD47', '264478', '9E480E', '636363', '997300', '255E91', '43682B', '698ED0', 'F1975A', 'B7B7B7', 'FFCD33']

rng = random.Random(20260929)


def col_name(i):
    s = ''
    i += 1
    while i:
        i, r = divmod(i - 1, 26)
        s = chr(65 + r) + s
    return s


def series(base, growth, weekend, yearly, peak, noise, spikes=0.003, start_day=0):
    """A daily series: a trend, a lift (or dip) at weekends, a yearly wave
    highest on day `peak` of the year, noise and a few spikes; zero before
    `start_day` (a store not open yet)."""
    out = []
    level = base
    for i in range(DAYS):
        if i < start_day:
            out.append(0.0)
            continue
        day = START + datetime.timedelta(days=i)
        level *= 1 + growth / 365 + rng.gauss(0, 0.003)
        season = 1 + yearly * math.cos(2 * math.pi * (day.timetuple().tm_yday - peak) / 365.25)
        week = 1 + (weekend if day.weekday() >= 5 else 0)
        v = level * season * week * (1 + rng.gauss(0, noise))
        if rng.random() < spikes:
            v *= rng.uniform(1.3, 2.2)
        out.append(max(v, 0.0))
    return out


# ----- The data ---------------------------------------------------------
# A fictional coffee chain: daily sales, orders and the average ticket of
# four categories, in eight regions, in store and online; and visitors.
visitors = {c: series(b, g, w, y, 200, 0.04) for c, b, g, w, y in [('Store', 21_000, 0.03, 0.22, 0.08), ('Online', 6_500, 0.14, -0.1, 0.05), ('App', 3_200, 0.3, 0.05, 0.04)]}
sheets = []  # (name, header, columns)
# Formulas by sheet and column (1-based): a function of the row giving the
# formula, cached with the column's value. The daily sheets sum a raw sheet
# with SUMIFS over whole columns, and the average ticket divides across
# sheets.
formulas = {}
raw_sheets = []
sheets.append(('Visitors_Daily', ['Date', 'Store', 'Online', 'App', 'Total'],
               [visitors['Store'], visitors['Online'], visitors['App'], [a + b + c for a, b, c in zip(visitors['Store'], visitors['Online'], visitors['App'])]]))
formulas['Visitors_Daily'] = {4: lambda r: f'SUM(B{r}:D{r})'}

sales = {}
# (category, daily sales of a store region, average ticket, yearly swing, peak day)
for f, sales_base, ticket, yearly, peak in [('Beverages', 5_200, 5.4, 0.18, 200), ('Food', 2_900, 7.9, 0.06, 60), ('Merchandise', 700, 24.0, 0.35, 355), ('Beans', 1_300, 16.5, 0.12, 330)]:
    rcols, icols, ecols, header = [], [], [], ['Date']
    for c in CHANNELS:
        for n, region in enumerate(REGIONS):
            size = rng.uniform(0.5, 1.8)
            start = rng.choice([0, 0, 0, 0, 300, 700, 1100])
            online = c == 'Online'
            r = series(sales_base * size * (0.35 if online else 1.0), rng.uniform(0.01, 0.12) + (0.1 if online else 0), -0.08 if online else 0.25, yearly, peak, 0.09, start_day=start)
            e = series(ticket * rng.uniform(0.85, 1.2) * (1.3 if online else 1.0), rng.uniform(0.01, 0.04), 0.03, 0.02, peak, 0.03, spikes=0.001, start_day=start)
            orders = [ri / ei if ei > 0 else 0.0 for ri, ei in zip(r, e)]
            rcols.append(r)
            icols.append(orders)
            ecols.append(e)
            header.append(f'{c}-{region}')
    store_r = [sum(v) for v in zip(*rcols[:8])]
    online_r = [sum(v) for v in zip(*rcols[8:])]
    store_o = [sum(v) for v in zip(*icols[:8])]
    online_o = [sum(v) for v in zip(*icols[8:])]
    tot_r = [a + b for a, b in zip(store_r, online_r)]
    tot_o = [a + b for a, b in zip(store_o, online_o)]
    ticket_of = lambda r, o: [ri / oi if oi > 0 else 0.0 for ri, oi in zip(r, o)]
    h = header + ['Store total', 'Online total', 'Total']
    # The raw sheet: a row a day and channel, sales and orders by region.
    raw = f'Raw_{f}'
    raw_header = ['Date', 'Channel'] + [f'Sales {n}' for n in REGIONS] + [f'Orders {n}' for n in REGIONS]
    raw_cols = [[c for _ in range(DAYS) for c in CHANNELS]]
    for k in range(8):
        raw_cols.append([v for i in range(DAYS) for v in (rcols[k][i], rcols[8 + k][i])])
    for k in range(8):
        raw_cols.append([v for i in range(DAYS) for v in (icols[k][i], icols[8 + k][i])])
    raw_sheets.append((raw, raw_header, raw_cols))

    def summed(first_raw_col, raw=raw):
        def column(j):
            letter = col_name(first_raw_col + j % 8)
            channel = CHANNELS[j // 8]
            return lambda r: f'SUMIFS({raw}!{letter}:{letter},{raw}!$A:$A,$A{r},{raw}!$B:$B,"{channel}")'
        totals = {17: lambda r: f'SUM(B{r}:I{r})', 18: lambda r: f'SUM(J{r}:Q{r})', 19: lambda r: f'R{r}+S{r}'}
        return {**{j + 1: column(j) for j in range(16)}, **totals}
    sheets.append((f'Sales_{f}', h, rcols + [store_r, online_r, tot_r]))
    formulas[f'Sales_{f}'] = summed(2)
    sheets.append((f'Orders_{f}', h, icols + [store_o, online_o, tot_o]))
    formulas[f'Orders_{f}'] = summed(10)
    sheets.append((f'Ticket_{f}', h, ecols + [ticket_of(store_r, store_o), ticket_of(online_r, online_o), ticket_of(tot_r, tot_o)]))
    formulas[f'Ticket_{f}'] = {j: (lambda L, f=f: lambda r: f'IFERROR(Sales_{f}!{L}{r}/Orders_{f}!{L}{r},0)')(col_name(j)) for j in range(1, 20)}
    sales[f] = (store_r, online_r, tot_r)

sales_store = [sum(v) for v in zip(*(sales[f][0] for f in CATEGORIES))]
sales_online = [sum(v) for v in zip(*(sales[f][1] for f in CATEGORIES))]
sheets.append(('Sales_Total', ['Date'] + CATEGORIES + ['Total'], [sales[f][2] for f in CATEGORIES] + [[a + b for a, b in zip(sales_store, sales_online)]]))
formulas['Sales_Total'] = {**{k + 1: (lambda f: lambda r: f'Sales_{f}!T{r}')(f) for k, f in enumerate(CATEGORIES)}, 5: lambda r: f'SUM(B{r}:E{r})'}
sheets.append(('Sales_Channel', ['Date', 'Store', 'Online', 'Total'], [sales_store, sales_online, [a + b for a, b in zip(sales_store, sales_online)]]))
formulas['Sales_Channel'] = {1: lambda r: '+'.join(f'Sales_{f}!R{r}' for f in CATEGORIES), 2: lambda r: '+'.join(f'Sales_{f}!S{r}' for f in CATEGORIES), 3: lambda r: f'B{r}+C{r}'}
sheets.append(('Sales_Store_Category', ['Date'] + CATEGORIES, [sales[f][0] for f in CATEGORIES]))
formulas['Sales_Store_Category'] = {k + 1: (lambda f: lambda r: f'Sales_{f}!R{r}')(f) for k, f in enumerate(CATEGORIES)}
sheets.append(('Sales_Online_Category', ['Date'] + CATEGORIES, [sales[f][1] for f in CATEGORIES]))
formulas['Sales_Online_Category'] = {k + 1: (lambda f: lambda r: f'Sales_{f}!S{r}')(f) for k, f in enumerate(CATEGORIES)}

# ----- The Overview's charts ---------------------------------------------
# (title, kind, sheet, column indexes (1-based data columns))
sheet_index = {name: (header, cols) for name, header, cols in sheets}
charts = [
    ('Total sales by channel', 'line', 'Sales_Channel', [1, 2, 3]),
    ('Store sales by category', 'line', 'Sales_Store_Category', [1, 2, 3, 4]),
    ('Online sales by category', 'line', 'Sales_Online_Category', [1, 2, 3, 4]),
]
for f in CATEGORIES:
    charts += [
        (f'{f} sales', 'line', f'Sales_{f}', [17, 18, 19]),
        (f'{f} orders', 'line', f'Orders_{f}', [17, 18, 19]),
        (f'{f} average ticket', 'line', f'Ticket_{f}', [17, 18, 19]),
        (f'{f} sales share by region', 'share', f'Sales_{f}', list(range(1, 17))),
        (f'{f} store sales share', 'share', f'Sales_{f}', list(range(1, 9))),
        (f'{f} online sales share', 'share', f'Sales_{f}', list(range(9, 17))),
    ]
charts += [
    ('Visitors (stacked)', 'stacked', 'Visitors_Daily', [1, 2, 3]),
    ('Visitors', 'line', 'Visitors_Daily', [1, 2, 3, 4]),
]


def num(v):
    return f'{v:.2f}'.rstrip('0').rstrip('.') if abs(v) < 1e15 else repr(v)


def quote(name):
    return "'" + name.replace("'", "''") + "'"


def chart_xml(title, kind, sheet, cols):
    header, data = sheet_index[sheet]
    n = DAYS
    last = n + 1
    cat_ref = f'{quote(sheet)}!$A$2:$A${last}'
    cat_cache = ''.join(f'<c:pt idx="{i}"><c:v>{s}</c:v></c:pt>' for i, s in enumerate(SERIALS))
    cat = f'<c:cat><c:numRef><c:f>{escape(cat_ref)}</c:f><c:numCache><c:formatCode>yyyy\\-mm\\-dd</c:formatCode><c:ptCount val="{n}"/>{cat_cache}</c:numCache></c:numRef></c:cat>'
    sers = []
    for k, c in enumerate(cols):
        letter = col_name(c)
        name_ref = f'{quote(sheet)}!${letter}$1'
        val_ref = f'{quote(sheet)}!${letter}$2:${letter}${last}'
        vals = ''.join(f'<c:pt idx="{i}"><c:v>{num(v)}</c:v></c:pt>' for i, v in enumerate(data[c - 1]))
        color = COLORS[k % len(COLORS)]
        if kind == 'line':
            sp = f'<c:spPr><a:ln w="15875" cap="rnd"><a:solidFill><a:srgbClr val="{color}"/></a:solidFill><a:round/></a:ln></c:spPr><c:marker><c:symbol val="none"/></c:marker>'
        else:
            sp = f'<c:spPr><a:solidFill><a:srgbClr val="{color}"/></a:solidFill><a:ln><a:noFill/></a:ln></c:spPr>'
        tail = '<c:smooth val="0"/>' if kind == 'line' else ''
        sers.append(
            f'<c:ser><c:idx val="{k}"/><c:order val="{k}"/><c:tx><c:strRef><c:f>{escape(name_ref)}</c:f><c:strCache><c:ptCount val="1"/><c:pt idx="0"><c:v>{escape(header[c])}</c:v></c:pt></c:strCache></c:strRef></c:tx>'
            f'{sp}{cat}<c:val><c:numRef><c:f>{escape(val_ref)}</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val="{n}"/>{vals}</c:numCache></c:numRef></c:val>{tail}</c:ser>'
        )
    if kind == 'line':
        group = f'<c:lineChart><c:grouping val="standard"/><c:varyColors val="0"/>{"".join(sers)}<c:marker val="1"/><c:axId val="10"/><c:axId val="20"/></c:lineChart>'
    else:
        grouping = 'percentStacked' if kind == 'share' else 'stacked'
        group = f'<c:areaChart><c:grouping val="{grouping}"/><c:varyColors val="0"/>{"".join(sers)}<c:axId val="10"/><c:axId val="20"/></c:areaChart>'
    fmt = '0%' if kind == 'share' else '#,##0'
    text = '<c:txPr><a:bodyPr/><a:lstStyle/><a:p><a:pPr><a:defRPr sz="900"><a:solidFill><a:srgbClr val="595959"/></a:solidFill></a:defRPr></a:pPr><a:endParaRPr lang="en-US"/></a:p></c:txPr>'
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
        '<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">'
        '<c:roundedCorners val="0"/><c:chart>'
        f'<c:title><c:tx><c:rich><a:bodyPr/><a:lstStyle/><a:p><a:pPr><a:defRPr sz="1400" b="0"><a:solidFill><a:srgbClr val="595959"/></a:solidFill></a:defRPr></a:pPr><a:r><a:rPr lang="en-US" sz="1400" b="0"><a:solidFill><a:srgbClr val="595959"/></a:solidFill></a:rPr><a:t>{escape(title)}</a:t></a:r></a:p></c:rich></c:tx><c:overlay val="0"/></c:title>'
        '<c:autoTitleDeleted val="0"/><c:plotArea><c:layout/>'
        f'{group}'
        '<c:dateAx><c:axId val="10"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="b"/>'
        '<c:numFmt formatCode="yyyy\\-mm\\-dd" sourceLinked="1"/><c:majorTickMark val="none"/><c:minorTickMark val="none"/><c:tickLblPos val="nextTo"/>'
        f'<c:spPr><a:ln w="9525"><a:solidFill><a:srgbClr val="D9D9D9"/></a:solidFill></a:ln></c:spPr>{text}'
        '<c:crossAx val="20"/><c:crosses val="autoZero"/><c:auto val="1"/><c:lblOffset val="100"/><c:baseTimeUnit val="days"/></c:dateAx>'
        '<c:valAx><c:axId val="20"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="l"/>'
        '<c:majorGridlines><c:spPr><a:ln w="9525"><a:solidFill><a:srgbClr val="D9D9D9"/></a:solidFill></a:ln></c:spPr></c:majorGridlines>'
        f'<c:numFmt formatCode="{fmt}" sourceLinked="0"/><c:majorTickMark val="none"/><c:minorTickMark val="none"/><c:tickLblPos val="nextTo"/>'
        f'<c:spPr><a:ln><a:noFill/></a:ln></c:spPr>{text}'
        '<c:crossAx val="10"/><c:crosses val="autoZero"/><c:crossBetween val="midCat"/></c:valAx>'
        '</c:plotArea>'
        f'<c:legend><c:legendPos val="b"/><c:overlay val="0"/>{text}</c:legend>'
        '<c:plotVisOnly val="1"/><c:dispBlanksAs val="zero"/></c:chart>'
        '<c:spPr><a:solidFill><a:srgbClr val="FFFFFF"/></a:solidFill><a:ln w="9525"><a:solidFill><a:srgbClr val="000000"/></a:solidFill></a:ln></c:spPr>'
        '</c:chartSpace>'
    )


# ----- The package -------------------------------------------------------
strings = []
string_index = {}


def sst(s):
    if s not in string_index:
        string_index[s] = len(strings)
        strings.append(s)
    return string_index[s]


def data_sheet_xml(header, cols, dates=None, sheet_formulas=None):
    """A data sheet: dates in column A, then `cols` (numbers, or text),
    each with its formula from `sheet_formulas` if it has one."""
    dates = dates or SERIALS
    sheet_formulas = sheet_formulas or {}
    rows = []
    cells = ''.join(f'<c r="{col_name(j)}1" t="s" s="2"><v>{sst(h)}</v></c>' for j, h in enumerate(header))
    rows.append(f'<row r="1">{cells}</row>')
    for i, date in enumerate(dates):
        r = i + 2
        cells = [f'<c r="A{r}" s="1"><v>{date}</v></c>']
        for j, col in enumerate(cols):
            ref = f'{col_name(j + 1)}{r}'
            v = col[i]
            if isinstance(v, str):
                cells.append(f'<c r="{ref}" t="s"><v>{sst(v)}</v></c>')
            elif (j + 1) in sheet_formulas:
                cells.append(f'<c r="{ref}" s="3"><f>{escape(sheet_formulas[j + 1](r))}</f><v>{num(v)}</v></c>')
            else:
                cells.append(f'<c r="{ref}" s="3"><v>{num(v)}</v></c>')
        rows.append(f'<row r="{r}">{"".join(cells)}</row>')
    last = f'{col_name(len(header) - 1)}{len(dates) + 1}'
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
        '<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">'
        f'<dimension ref="A1:{last}"/><sheetViews><sheetView workbookViewId="0"><pane ySplit="1" topLeftCell="A2" activePane="bottomLeft" state="frozen"/></sheetView></sheetViews>'
        '<sheetFormatPr defaultRowHeight="15"/><cols><col min="1" max="1" width="12" customWidth="1"/>'
        f'<col min="2" max="{len(header)}" width="14" customWidth="1"/></cols>'
        f'<sheetData>{"".join(rows)}</sheetData></worksheet>'
    )


# Charts in three columns, each 8 columns wide and 20 rows high, under a title.
PER_ROW, CHART_COLS, CHART_ROWS, TOP = 3, 8, 20, 3


def anchor(k):
    col = (k % PER_ROW) * (CHART_COLS + 1)
    row = TOP + (k // PER_ROW) * (CHART_ROWS + 1)
    return col, row


def drawing_xml():
    parts = []
    for k in range(len(charts)):
        c, r = anchor(k)
        parts.append(
            f'<xdr:twoCellAnchor editAs="oneCell"><xdr:from><xdr:col>{c}</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>{r}</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from>'
            f'<xdr:to><xdr:col>{c + CHART_COLS}</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>{r + CHART_ROWS}</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to>'
            f'<xdr:graphicFrame macro=""><xdr:nvGraphicFramePr><xdr:cNvPr id="{k + 2}" name="Chart {k + 1}"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr>'
            '<xdr:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/></xdr:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart">'
            f'<c:chart xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" r:id="rId{k + 1}"/></a:graphicData></a:graphic></xdr:graphicFrame><xdr:clientData/></xdr:twoCellAnchor>'
        )
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
        '<xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">'
        f'{"".join(parts)}</xdr:wsDr>'
    )


def overview_xml():
    rows = (
        f'<row r="1"><c r="A1" t="s" s="4"><v>{sst("Nimbus Coffee sales overview (fictional data)")}</v></c></row>'
        f'<row r="2"><c r="A2" t="s"><v>{sst("Daily figures, 2020-01-01 to 2026-07-31. All names and values are made up.")}</v></c></row>'
    )
    last_row = anchor(len(charts) - 1)[1] + CHART_ROWS + 1
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
        '<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">'
        f'<dimension ref="A1:Z{last_row}"/><sheetViews><sheetView showGridLines="0" tabSelected="1" zoomScale="90" zoomScaleNormal="90" workbookViewId="0"><selection activeCell="A1" sqref="A1"/></sheetView></sheetViews>'
        f'<sheetFormatPr defaultRowHeight="15"/><sheetData>{rows}</sheetData><drawing r:id="rId1"/></worksheet>'
    )


STYLES = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
    '<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">'
    '<numFmts count="1"><numFmt numFmtId="164" formatCode="yyyy\\-mm\\-dd"/></numFmts>'
    '<fonts count="3"><font><sz val="11"/><name val="Calibri"/><family val="2"/></font><font><b/><sz val="11"/><name val="Calibri"/><family val="2"/></font><font><b/><sz val="16"/><name val="Calibri"/><family val="2"/></font></fonts>'
    '<fills count="3"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill><fill><patternFill patternType="solid"><fgColor rgb="FFDDEBF7"/><bgColor indexed="64"/></patternFill></fill></fills>'
    '<borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders>'
    '<cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs>'
    '<cellXfs count="5"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/>'
    '<xf numFmtId="164" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/>'
    '<xf numFmtId="0" fontId="1" fillId="2" borderId="0" xfId="0" applyFont="1" applyFill="1"/>'
    '<xf numFmtId="3" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/>'
    '<xf numFmtId="0" fontId="2" fillId="0" borderId="0" xfId="0" applyFont="1"/></cellXfs>'
    '<cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles></styleSheet>'
)


def main(path):
    raw_dates = [d for d in SERIALS for _ in CHANNELS]
    names = ['Overview'] + [s[0] for s in sheets] + [s[0] for s in raw_sheets]
    sheet_xmls = ([overview_xml()] + [data_sheet_xml(h, c, sheet_formulas=formulas.get(n)) for n, h, c in sheets]
                  + [data_sheet_xml(h, c, dates=raw_dates) for _, h, c in raw_sheets])
    ct = ['<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>',
          '<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>',
          '<Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>',
          '<Override PartName="/xl/sharedStrings.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"/>',
          '<Override PartName="/xl/drawings/drawing1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawing+xml"/>',
          '<Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/>',
          '<Override PartName="/docProps/app.xml" ContentType="application/vnd.openxmlformats-officedocument.extended-properties+xml"/>']
    ct += [f'<Override PartName="/xl/worksheets/sheet{i + 1}.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>' for i in range(len(names))]
    ct += [f'<Override PartName="/xl/charts/chart{k + 1}.xml" ContentType="application/vnd.openxmlformats-officedocument.drawingml.chart+xml"/>' for k in range(len(charts))]
    workbook = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
        '<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">'
        '<bookViews><workbookView activeTab="0"/></bookViews><sheets>'
        + ''.join(f'<sheet name="{escape(n)}" sheetId="{i + 1}" r:id="rId{i + 1}"/>' for i, n in enumerate(names))
        + '</sheets><calcPr calcId="191028"/></workbook>'
    )
    wb_rels = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        + ''.join(f'<Relationship Id="rId{i + 1}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet{i + 1}.xml"/>' for i in range(len(names)))
        + f'<Relationship Id="rId{len(names) + 1}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>'
        + f'<Relationship Id="rId{len(names) + 2}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="sharedStrings.xml"/></Relationships>'
    )
    drawing_rels = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        + ''.join(f'<Relationship Id="rId{k + 1}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart" Target="../charts/chart{k + 1}.xml"/>' for k in range(len(charts)))
        + '</Relationships>'
    )
    root_rels = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>'
        '<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/>'
        '<Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties" Target="docProps/app.xml"/></Relationships>'
    )
    core = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">'
        '<dc:title>Nimbus Coffee sales report (fictional)</dc:title><dcterms:created xsi:type="dcterms:W3CDTF">2026-09-29T00:00:00Z</dcterms:created><dcterms:modified xsi:type="dcterms:W3CDTF">2026-09-29T00:00:00Z</dcterms:modified></cp:coreProperties>'
    )
    app = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"><Application>Microsoft Excel</Application></Properties>'
    # The sheets first: their text goes into the shared strings.
    parts = {f'xl/worksheets/sheet{i + 1}.xml': x for i, x in enumerate(sheet_xmls)}
    parts['xl/worksheets/_rels/sheet1.xml.rels'] = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing" Target="../drawings/drawing1.xml"/></Relationships>'
    )
    parts['xl/drawings/drawing1.xml'] = drawing_xml()
    parts['xl/drawings/_rels/drawing1.xml.rels'] = drawing_rels
    for k, (title, kind, sheet, cols) in enumerate(charts):
        parts[f'xl/charts/chart{k + 1}.xml'] = chart_xml(title, kind, sheet, cols)
    parts['xl/sharedStrings.xml'] = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
        f'<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="{len(strings)}" uniqueCount="{len(strings)}">'
        + ''.join(f'<si><t>{escape(s)}</t></si>' for s in strings) + '</sst>'
    )
    parts['xl/styles.xml'] = STYLES
    parts['xl/workbook.xml'] = workbook
    parts['xl/_rels/workbook.xml.rels'] = wb_rels
    parts['docProps/core.xml'] = core
    parts['docProps/app.xml'] = app
    parts['_rels/.rels'] = root_rels
    parts['[Content_Types].xml'] = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">' + ''.join(ct) + '</Types>'
    order = ['[Content_Types].xml', '_rels/.rels'] + [p for p in parts if p not in ('[Content_Types].xml', '_rels/.rels')]
    with zipfile.ZipFile(path, 'w', zipfile.ZIP_DEFLATED) as z:
        for p in order:
            info = zipfile.ZipInfo(p, date_time=(1980, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            z.writestr(info, parts[p])
    print(f'{path}: {len(names)} sheets ({DAYS} days), {len(charts)} charts')


if __name__ == '__main__':
    main(sys.argv[1])
