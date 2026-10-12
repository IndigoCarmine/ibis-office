#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (c) ibis inc.
# Additional terms under AGPL-3.0 section 7(b): see NOTICE.
"""Write a long English document for benchmarks, with made-up content: a
securities registration statement (prospectus) of a fictional coffee chain
("Nimbus Coffee, Inc.", the same chain as in gen_xlsx.py), about 140 A4
pages of headings, body text and ruled tables (financial statements and
the like), as long as the one the benchmark first used.

    scripts/benchmark-data/gen_docx.py test/benchmark/securities-registration-statement.docx

The OOXML is written directly. The same file comes out every time.
"""
import random
import sys
import zipfile
from xml.sax.saxutils import escape

rng = random.Random(20260929)
COMPANY = 'Nimbus Coffee, Inc.'
TITLE = 'Securities Registration Statement'
BODY_FONT, HEAD_FONT = 'Times New Roman', 'Arial'
TEXT_WIDTH = 9072  # twips: A4 less 25 mm margins

REGIONS = ['Northfield', 'Eastbay', 'Southport', 'Westridge', 'Lakeside', 'Hillcrest', 'Riverside', 'Harborview']
CATEGORIES = ['Beverages', 'Food', 'Merchandise', 'Beans']
YEARS = [f'FY{2021 + i}' for i in range(5)]

# ----- Text --------------------------------------------------------------
ORGS = ['The Company', 'The Group', 'Nimbus Coffee']
UNITS = ([f'the {c} category' for c in CATEGORIES] + [f'stores in the {r} region' for r in REGIONS]
         + ['the online store', 'wholesale customers', 'company-operated stores'])
SENTENCES = [
    '{o} operates specialty coffee shops under the Nimbus Coffee name and roasts all of the coffee it serves at its own roastery in Northfield.',
    'Total revenue for the fiscal year was ${n1} thousand, an increase of {p1}% over the prior year, and operating income was ${n2} thousand, an increase of {p2}%.',
    'Comparable store sales increased {p3}%, driven by a {p3b}% increase in average ticket and a {p3c}% increase in transactions.',
    'At the end of the fiscal year, {om} operated {k1} company-operated stores in eight regions: Northfield, Eastbay, Southport, Westridge, Lakeside, Hillcrest, Riverside and Harborview.',
    'Green coffee is purchased from exporters and cooperatives in Central America, South America and East Africa under fixed-price contracts that generally cover the next nine to twelve months of expected needs.',
    'The price of green coffee is volatile, and a sustained increase could reduce gross margin if {om} is unable to pass the cost on to customers through menu prices.',
    'Revenue from {u} was ${n6} thousand, or {p4}% of total revenue.',
    'Orders placed through the online store, including orders for pickup in store and home delivery of whole beans, accounted for {p4}% of revenue in the Beverages and Beans categories.',
    'During the fiscal year, {om} opened {k2} new stores and closed {k3} stores whose leases expired.',
    'New stores generally reach the sales level of a mature store within eighteen to twenty-four months of opening.',
    'Store operating expenses, which consist mainly of wages, benefits and supplies, were {p5}% of revenue, compared with {p5b}% in the prior year.',
    'Occupancy costs increased to ${n2} thousand, reflecting rent for new stores and scheduled rent increases at existing locations.',
    '{o} leases all of its store premises under operating leases with initial terms of ten years, most of which include options to renew.',
    'Capital expenditures totaled ${n4} thousand and consisted mainly of the build-out of new stores, the remodeling of existing stores and a second roasting line at the Northfield roastery.',
    'The loyalty program had {n3} thousand active members at the end of the fiscal year, and purchases by members accounted for {p4}% of store revenue.',
    'Revenue from loyalty rewards is deferred until the reward is redeemed or expires.',
    '{o} depends on a small number of dairy and bakery suppliers in each region, and an interruption in their deliveries could affect sales at the stores they serve.',
    'Labor costs are affected by minimum wage laws in the regions where the stores are located, and further increases could reduce store margins.',
    'The ability of {om} to open new stores depends on finding suitable sites on acceptable lease terms and on hiring and training store managers.',
    'Food safety incidents, whether or not they occur at our stores, could harm the reputation of the Nimbus Coffee brand and reduce customer traffic.',
    'Merchandise sales, consisting of mugs, brewing equipment and seasonal gift sets, are highest in the fourth quarter of each year.',
    'Weather affects customer traffic: cold drinks sell best in summer, while hot beverages and whole beans sell best from October through February.',
    'Net cash provided by operating activities was ${n1} thousand, mainly reflecting net income and depreciation and amortization.',
    'Net cash used in investing activities was ${n4} thousand, primarily for purchases of property and equipment.',
    'Net cash used in financing activities was ${n4} thousand, reflecting repayments of the term loan and dividends paid.',
    'Total assets at the end of the fiscal year were ${n5} thousand, an increase of ${n2} thousand from the end of the prior fiscal year.',
    '{o} intends to pay dividends out of retained earnings while keeping sufficient funds to finance the opening of new stores.',
    'These forward-looking statements are based on information available as of the date of this statement, and actual results may differ materially because of many factors.',
    'Inventories consist of green coffee, roasted coffee, packaging, food and merchandise, and are stated at the lower of cost and net realizable value.',
    'The roastery produced {n3} thousand pounds of roasted coffee during the fiscal year, of which {p4}% was sold through company-operated stores.',
    'Each store is managed by a store manager who reports to a district manager, and district managers report to the director for their region.',
    'Baristas complete a four-week training course before working on the floor, and store managers complete a further course at the Northfield training center.',
    '{o} buys {p4}% of its green coffee by volume from farms certified under recognized sustainability standards.',
    'Energy use per store decreased {p1}% compared with the prior year, as older espresso machines and refrigerators were replaced with more efficient models.',
    '{o} holds no derivatives other than coffee futures contracts used to fix the price of part of its future green coffee purchases.',
    'Wholesale revenue comes from sales of roasted beans to restaurants, hotels and grocery stores in the regions where {om} operates.',
    'Revenue from {u} increased {p1}% compared with the prior year, while its share of total revenue was little changed.',
]


def fill(t):
    o = rng.choice(ORGS)
    return t.format(
        o=o, om=o.replace('The ', 'the '), u=rng.choice(UNITS), n6=f'{rng.randint(3_000, 64_000):,}',
        p3b=f'{rng.uniform(0.5, 5):.1f}', p3c=f'{rng.uniform(0.5, 4):.1f}', p5b=f'{rng.uniform(28, 36):.1f}',
        n1=f'{rng.randint(12_000, 312_000):,}', n2=f'{rng.randint(900, 38_000):,}', n3=f'{rng.randint(80, 1_450):,}',
        n4=f'{rng.randint(1_500, 26_000):,}', n5=f'{rng.randint(90_000, 260_000):,}',
        p1=f'{rng.uniform(1, 18):.1f}', p2=f'{rng.uniform(1, 25):.1f}', p3=f'{rng.uniform(0.5, 7):.1f}',
        p4=f'{rng.uniform(4, 62):.1f}', p5=f'{rng.uniform(28, 36):.1f}',
        k1=rng.randint(150, 196), k2=rng.randint(6, 19), k3=rng.randint(1, 5),
    )


def paragraph_text(sentences=None):
    return ' '.join(fill(rng.choice(SENTENCES)) for _ in range(sentences or rng.randint(3, 6)))


# ----- Blocks ------------------------------------------------------------
def run(text, bold=False, font=None, size=None):
    props = ''
    if font:
        props += f'<w:rFonts w:ascii="{font}" w:hAnsi="{font}"/>'
    if bold:
        props += '<w:b/>'
    if size:
        props += f'<w:sz w:val="{size}"/><w:szCs w:val="{size}"/>'
    rpr = f'<w:rPr>{props}</w:rPr>' if props else ''
    return f'<w:r>{rpr}<w:t xml:space="preserve">{escape(text)}</w:t></w:r>'


def para(text, style=None, jc=None, indent=None, first=None, keep=False, bold=False, size=None):
    ppr = ''
    if style:
        ppr += f'<w:pStyle w:val="{style}"/>'
    if keep:
        ppr += '<w:keepNext/>'
    if indent is not None or first is not None:
        ppr += f'<w:ind w:left="{indent or 0}" w:firstLine="{first or 0}"/>'
    if jc:
        ppr += f'<w:jc w:val="{jc}"/>'
    return f'<w:p><w:pPr>{ppr}</w:pPr>{run(text, bold=bold, size=size) if text else ""}</w:p>'


def page_break():
    return '<w:p><w:r><w:br w:type="page"/></w:r></w:p>'


def table(rows, widths, header_rows=1, right_cols=(), bold_rows=()):
    """A ruled table: `rows` of cell texts, `widths` in twips; numbers in
    `right_cols` set right, header rows shaded, rows in `bold_rows`
    (totals) bold."""
    grid = ''.join(f'<w:gridCol w:w="{w}"/>' for w in widths)
    trs = []
    for i, row in enumerate(rows):
        tcs = []
        for j, (text, w) in enumerate(zip(row, widths)):
            shade = '<w:shd w:val="clear" w:color="auto" w:fill="DDEBF7"/>' if i < header_rows else ''
            jc = 'center' if i < header_rows else ('right' if j in right_cols else 'left')
            tcs.append(f'<w:tc><w:tcPr><w:tcW w:w="{w}" w:type="dxa"/>{shade}<w:vAlign w:val="center"/></w:tcPr>'
                       f'<w:p><w:pPr><w:spacing w:before="0" w:after="0"/><w:jc w:val="{jc}"/></w:pPr>'
                       f'{run(text, bold=i < header_rows or i in bold_rows, size=18)}</w:p></w:tc>')
        header = '<w:trPr><w:tblHeader/></w:trPr>' if i < header_rows else ''
        trs.append(f'<w:tr>{header}{"".join(tcs)}</w:tr>')
    borders = ''.join(f'<w:{b} w:val="single" w:sz="4" w:space="0" w:color="000000"/>' for b in ('top', 'left', 'bottom', 'right', 'insideH', 'insideV'))
    return (f'<w:tbl><w:tblPr><w:tblW w:w="{sum(widths)}" w:type="dxa"/><w:tblBorders>{borders}</w:tblBorders><w:tblLayout w:type="fixed"/>'
            f'<w:tblCellMar><w:left w:w="72" w:type="dxa"/><w:right w:w="72" w:type="dxa"/></w:tblCellMar></w:tblPr><w:tblGrid>{grid}</w:tblGrid>{"".join(trs)}</w:tbl>'
            + para(''))


def usd(v):
    """Thousands of USD as filed: negatives in parentheses."""
    v = round(v)
    return f'({-v:,})' if v < 0 else f'{v:,}'


def jitter(v, spread=0.08):
    return v * rng.uniform(1 - spread, 1 + spread)


def date(y0=2011, y1=2025):
    months = ['January', 'February', 'March', 'April', 'May', 'June', 'July', 'August', 'September', 'October', 'November', 'December']
    return f'{rng.choice(months)} {rng.randint(y0, y1)}'


FIRST = ['Avery', 'Blake', 'Casey', 'Dana', 'Elliot', 'Finley', 'Harper', 'Jordan', 'Kendall', 'Logan', 'Morgan', 'Parker', 'Quinn', 'Reese', 'Rowan', 'Sawyer', 'Taylor', 'Emery']
LAST = ['Ellery', 'Tate', 'Marlow', 'Pennick', 'Oakes', 'Hale', 'Brandt', 'Corwin', 'Dunmore', 'Faraday', 'Greer', 'Holloway', 'Kessler', 'Lindqvist', 'Mercer', 'Northcott', 'Prescott', 'Vance']
HOLDER_A = ['Alder', 'Bramble', 'Cobalt', 'Dunmore', 'Elkhorn', 'Fenwick', 'Glenhaven', 'Hollis', 'Ivorydale', 'Juniper', 'Kestrel', 'Larchmont']
HOLDER_B = ['Ridge', 'Bay', 'Field', 'Stone', 'Brook', 'Hollow', 'Crest']
HOLDER_C = ['Capital LP', 'Partners LLC', 'Holdings LLC', 'Investments LP', 'Family Trust', 'Growth Fund LP']


def person():
    return f'{rng.choice(FIRST)} {rng.choice(LAST)}'


def holder():
    return f'{rng.choice(HOLDER_A)} {rng.choice(HOLDER_B)} {rng.choice(HOLDER_C)}'


def address():
    return f'{rng.randint(10, 990)} {rng.choice(["Harbor", "Mill", "Orchard", "Station", "Market", "Chapel"])} Street, {rng.choice(REGIONS)}'


# ----- The document ------------------------------------------------------
blocks = []
add = blocks.append


def h1(t):
    add(para(t, 'Heading1', keep=True))


def h2(t):
    # Each part starts on a new page, as in the filed statements.
    if blocks and blocks[-1] != page_break():
        add(page_break())
    add(para(t, 'Heading2', keep=True))


def h3(t):
    add(para(t, 'Heading3', keep=True))


def body(n=1):
    for _ in range(n):
        add(para(paragraph_text(), first=360))


def cover():
    add(para(TITLE.upper(), jc='center', bold=True, size=32))
    add(para('Cover Page', jc='center', bold=True))
    rows = [[k, v] for k, v in [
        ('Document filed', TITLE), ('Filed with', 'Director-General, Regional Securities Bureau (fictional)'), ('Filing date', 'September 29, 2026'),
        ('Registrant', COMPANY), ('Representative', 'Jordan Ellery, Chief Executive Officer'), ('Address of principal office', '100 Harbor Street, Northfield (fictional)'),
        ('Telephone', '(000) 000-0000 (fictional)'), ('Contact person', 'Morgan Tate, Chief Financial Officer'), ('Nearest place of contact', '100 Harbor Street, Northfield (fictional)'),
        ('Class of securities offered', 'Common stock, par value $0.01 per share'),
        ('Amount of offering', 'Offering by book-building: $XXX,XXX,XXX (to be determined on the pricing date)'),
        ('Places where copies are kept for public inspection', 'None')]]
    add(table(rows, [3600, 5472], header_rows=0))
    add(para('Note: This document is a fictional securities registration statement made for benchmarks. '
             'All names of companies, persons and places, and all figures, are made up and have no connection with any real company.'))
    add(page_break())


def statement(title, columns, rows, bold_rows):
    # Each statement on a page of its own.
    add(page_break())
    h3(title)
    add(para('(In thousands of U.S. dollars)', jc='right', size=18))
    add(table([['', 'Note'] + columns] + rows, [4872, 800, 1700, 1700], right_cols=(2, 3), bold_rows=bold_rows))


def lines(entries, scales):
    """Rows for `entries` of (label, weight) or (label, None) for a subtotal
    of the items since the last subtotal; values for each column scale.
    Returns (rows, bold row indices, sums per column)."""
    rows, bold = [], []
    vals = [[] for _ in scales]
    for c, scale in enumerate(scales):
        run_sum = 0
        for label, w in entries:
            if w is None:
                vals[c].append(run_sum)
                run_sum = 0
            else:
                v = round(jitter(w * scale))
                vals[c].append(v)
                run_sum += v
    for i, (label, w) in enumerate(entries):
        rows.append([label, str(rng.randint(1, 18)) if w is not None and rng.random() < 0.3 else ''] + [usd(vals[c][i]) for c in range(len(scales))])
        if w is None:
            bold.append(i + 1)
    return rows, bold, vals


CURRENT_ASSETS = [('Cash and cash equivalents', 0.14), ('Short-term investments', 0.03), ('Accounts receivable, net', 0.025), ('Inventories', 0.06),
                  ('Prepaid expenses and other current assets', 0.02), ('Total current assets', None)]
NONCURRENT_ASSETS = [('Property and equipment, net', 0.33), ('Operating lease right-of-use assets', 0.31), ('Deferred income taxes', 0.015),
                     ('Goodwill', 0.02), ('Other intangible assets, net', 0.01), ('Security deposits', 0.02), ('Other long-term assets', 0.01), ('Total non-current assets', None)]
CURRENT_LIAB = [('Accounts payable', 0.04), ('Accrued payroll and benefits', 0.03), ('Current portion of operating lease liabilities', 0.045),
                ('Current portion of long-term debt', 0.02), ('Deferred revenue (gift cards and loyalty)', 0.025), ('Income taxes payable', 0.008),
                ('Other accrued liabilities', 0.015), ('Total current liabilities', None)]
NONCURRENT_LIAB = [('Long-term debt, net of current portion', 0.09), ('Operating lease liabilities, net of current portion', 0.28),
                   ('Deferred rent and asset retirement obligations', 0.012), ('Other long-term liabilities', 0.008), ('Total non-current liabilities', None)]
EQUITY = [('Common stock', 0.001), ('Additional paid-in capital', 0.12)]
INCOME_LINES = ['Company-operated stores', 'Online store', 'Wholesale', 'Total revenue', 'Product and distribution costs', 'Store operating expenses',
                'Occupancy costs', 'Depreciation and amortization', 'General and administrative expenses', 'Store opening expenses',
                'Total operating expenses', 'Operating income', 'Interest income', 'Interest expense', 'Other income (expense), net',
                'Income before income taxes', 'Income tax expense', 'Net income', 'Earnings per share, basic (in dollars)',
                'Earnings per share, diluted (in dollars)']


def balance_sheets(columns, assets_scale):
    """Balance sheets for two dates: the assets page, then the liabilities
    and equity page, which balances with it (retained earnings take the rest)."""
    scales = [assets_scale / rng.uniform(1.06, 1.14), assets_scale]
    r1, b1, v1 = lines(CURRENT_ASSETS, scales)
    r2, b2, v2 = lines(NONCURRENT_ASSETS, scales)
    totals = [v1[c][-1] + v2[c][-1] for c in range(2)]
    rows = [['Current assets', ''] + ['', '']] + r1 + [['Non-current assets', '', '', '']] + r2 + [['Total assets', ''] + [usd(t) for t in totals]]
    bold = [1] + [b + 1 for b in b1] + [len(r1) + 2] + [b + len(r1) + 2 for b in b2] + [len(rows)]
    statement('Balance Sheets (Assets)', columns, rows, bold)
    r3, b3, v3 = lines(CURRENT_LIAB, scales)
    r4, b4, v4 = lines(NONCURRENT_LIAB, scales)
    liab = [v3[c][-1] + v4[c][-1] for c in range(2)]
    r5, _, v5 = lines(EQUITY, scales)
    treasury = [-round(jitter(0.01 * s)) for s in scales]
    aoci = [round(rng.uniform(-0.004, 0.004) * s) for s in scales]
    retained = [totals[c] - liab[c] - sum(v5[c]) - treasury[c] - aoci[c] for c in range(2)]
    equity = [totals[c] - liab[c] for c in range(2)]
    rows = ([['Current liabilities', '', '', '']] + r3 + [['Non-current liabilities', '', '', '']] + r4
            + [['Total liabilities', ''] + [usd(x) for x in liab], ["Stockholders' equity", '', '', '']] + r5
            + [['Retained earnings', ''] + [usd(x) for x in retained], ['Treasury stock, at cost', ''] + [usd(x) for x in treasury],
               ['Accumulated other comprehensive income (loss)', ''] + [usd(x) for x in aoci],
               ["Total stockholders' equity", ''] + [usd(x) for x in equity],
               ["Total liabilities and stockholders' equity", ''] + [usd(t) for t in totals]])
    n3 = len(r3)
    bold = ([1] + [b + 1 for b in b3] + [n3 + 2] + [b + n3 + 2 for b in b4]
            + [n3 + len(r4) + 3, n3 + len(r4) + 4, len(rows) - 1, len(rows)])
    statement("Balance Sheets (Liabilities and Stockholders' Equity)", columns, rows, bold)


def income_statement(columns, revenue):
    """The statements of income for two periods; returns (revenue, net
    income, depreciation) of each, for the statements of cash flows."""
    cols, figures = [], []
    for rev in (revenue / rng.uniform(1.08, 1.16), revenue):
        stores, online, wholesale = rev * rng.uniform(0.80, 0.84), rev * rng.uniform(0.10, 0.12), 0
        wholesale = rev - stores - online
        costs = [rev * rng.uniform(*r) for r in [(0.29, 0.31), (0.33, 0.35), (0.11, 0.12), (0.05, 0.055), (0.06, 0.07), (0.004, 0.008)]]
        op = rev - sum(costs)
        interest_in, interest_out, other = rev * 0.001, -rev * rng.uniform(0.003, 0.005), rev * rng.uniform(-0.001, 0.001)
        pretax = op + interest_in + interest_out + other
        tax = pretax * rng.uniform(0.24, 0.27)
        net = pretax - tax
        shares = 41_250
        figures.append((rev, net, costs[3]))
        cols.append([usd(x) for x in [stores, online, wholesale, rev] + costs + [sum(costs), op, interest_in, interest_out, other, pretax, -tax, net]]
                    + [f'{net / shares:.2f}', f'{net / (shares * 1.03):.2f}'])
    rows = [[label, str(rng.randint(1, 18)) if rng.random() < 0.25 else '', cols[0][i], cols[1][i]] for i, label in enumerate(INCOME_LINES)]
    rows.insert(0, ['Revenue', '', '', ''])
    rows.insert(5, ['Operating expenses', '', '', ''])
    statement('Statements of Income', columns, rows, [1, 5, 6, 13, 14, 18, 20])
    return figures


def cash_flows(columns, figures):
    cols, labels = [], []
    for rev, net, dep in figures:
        ops = [('Net income', net), ('Depreciation and amortization', dep), ('Stock-based compensation', rev * 0.006),
               ('Deferred income taxes', rev * rng.uniform(-0.003, 0.003)), ('Inventories', -rev * rng.uniform(0.002, 0.008)),
               ('Accounts receivable', -rev * rng.uniform(0.001, 0.004)), ('Accounts payable', rev * rng.uniform(0.001, 0.005)),
               ('Deferred revenue', rev * rng.uniform(0.001, 0.004)), ('Other accrued liabilities', rev * rng.uniform(-0.002, 0.003))]
        inv = [('Purchases of property and equipment', -rev * rng.uniform(0.08, 0.10)), ('Purchases of roasting equipment', -rev * rng.uniform(0.005, 0.02)),
               ('Purchases of short-term investments', -rev * rng.uniform(0.01, 0.02)), ('Maturities of short-term investments', rev * rng.uniform(0.01, 0.02))]
        fin = [('Proceeds from borrowings under the credit facility', rev * rng.uniform(0, 0.02)), ('Repayments of long-term debt', -rev * rng.uniform(0.01, 0.02)),
               ('Dividends paid', -rev * 0.012), ('Proceeds from exercise of stock options', rev * rng.uniform(0.001, 0.003))]
        begin = rev * rng.uniform(0.09, 0.11)
        o, i, f = (sum(v for _, v in g) for g in (ops, inv, fin))
        labels = ([('Cash flows from operating activities', None)] + [(k, 1) for k, _ in ops] + [('Net cash provided by operating activities', None),
                  ('Cash flows from investing activities', None)] + [(k, 1) for k, _ in inv] + [('Net cash used in investing activities', None),
                  ('Cash flows from financing activities', None)] + [(k, 1) for k, _ in fin] + [('Net cash used in financing activities', None),
                  ('Net increase (decrease) in cash and cash equivalents', None), ('Cash and cash equivalents at beginning of period', 1),
                  ('Cash and cash equivalents at end of period', None)])
        cols.append([''] + [usd(v) for _, v in ops] + [usd(o), ''] + [usd(v) for _, v in inv] + [usd(i), ''] + [usd(v) for _, v in fin]
                    + [usd(f), usd(o + i + f), usd(begin), usd(begin + o + i + f)])
    rows = [[k, '', cols[0][n], cols[1][n]] for n, (k, _) in enumerate(labels)]
    statement('Statements of Cash Flows', columns, rows, [n + 1 for n, (_, w) in enumerate(labels) if w is None])


HISTORY = ['Company founded in Northfield', 'First Nimbus Coffee shop opened on Harbor Street, Northfield', 'Began roasting its own coffee in a leased roastery in Northfield',
           'First store outside Northfield opened in Eastbay', 'Launched the online store for whole beans and brewing equipment', 'Opened the 25th store',
           'Entered the Southport and Westridge regions', 'Introduced the Nimbus Rewards loyalty program', 'Moved roasting to a new roastery and training center in Northfield',
           'Entered the Lakeside and Hillcrest regions', 'Began wholesale sales of roasted beans to restaurants and hotels', 'Opened the 100th store',
           'Entered the Riverside and Harborview regions', 'Added a second roasting line at the Northfield roastery', 'Opened the 180th store']
RISKS = ['Green coffee prices', 'Competition', 'Opening of new stores', 'Labor costs and staffing', 'Food safety', 'Information security and payment card data',
         'Supply chain disruption', 'Laws and regulations', 'Trademarks and the Nimbus Coffee brand', 'Dependence on key suppliers', 'Weather and seasonality',
         'Lease obligations', 'Concentration of stores in eight regions', 'Key personnel']
NOTES = ['Summary of significant accounting policies', 'Revenue recognition', 'Leases', 'Fair value measurements', 'Inventories', 'Property and equipment',
         'Goodwill and other intangible assets', 'Debt', 'Stock-based compensation', 'Income taxes', 'Segment reporting', 'Related-party transactions',
         'Earnings per share', 'Commitments and contingencies', 'Gift cards and the loyalty program', 'Subsequent events']
SEGMENTS = ['Company-operated stores', 'Roasting and wholesale', 'Online store', 'Corporate', 'Total']


def build():
    cover()
    h1('Part I — Information about the Securities')
    for sec in ['Item 1. Terms of the Offering', 'Item 2. Secondary Offering by Selling Stockholders', 'Item 3. Special Matters on Allotment to Third Parties']:
        h2(sec)
        for sub in ['1. Shares Offered', '2. Method of Offering', '3. Terms of the Offering', '4. Underwriting', '5. Use of Proceeds']:
            h3(sub)
            body(2)
            add(table([['Class', 'Number of shares', 'Description']]
                      + [[f'Common stock (Series {"ABC"[k]})', f'{rng.randint(20, 650) * 1000:,}', 'Full voting rights; the standard class of shares of the Company, with no restriction on the rights of holders.'] for k in range(3)],
                      [2200, 1600, 5272], right_cols=(1,)))
    add(page_break())
    h1('Part II — Information about the Company')
    h2('Item 1. Overview of the Company')
    h3('1. Selected Financial Data')
    for scope in ['(1) Consolidated', '(2) Parent company']:
        add(para(scope, bold=True, keep=True))
        rev = [168_400 * 1.13 ** i for i in range(5)]
        metrics = [('Revenue (USD thousands)', [jitter(r, 0.02) for r in rev]), ('Operating income (USD thousands)', [jitter(r * 0.09) for r in rev]),
                   ('Net income (USD thousands)', [jitter(r * 0.06) for r in rev]), ('Common stock (USD thousands)', [412.5] * 5),
                   ('Shares outstanding', [41_250_000 + 150_000 * i for i in range(5)]), ('Total equity (USD thousands)', [jitter(r * 0.62) for r in rev]),
                   ('Total assets (USD thousands)', [jitter(r * 1.35) for r in rev])]
        rows = [[m, *[usd(v) for v in vs]] for m, vs in metrics]
        rows += [['Book value per share (USD)'] + [f'{jitter(r * 0.62) / 41_250:.2f}' for r in rev],
                 ['Earnings per share (USD)'] + [f'{jitter(r * 0.06) / 41_250:.2f}' for r in rev],
                 ['Equity ratio (%)'] + [f'{rng.uniform(42, 50):.1f}' for _ in rev], ['Return on equity (%)'] + [f'{rng.uniform(8, 12):.1f}' for _ in rev],
                 ['Net cash provided by operating activities (USD thousands)'] + [usd(jitter(r * 0.13)) for r in rev],
                 ['Number of stores at year end'] + [str(98 + 20 * i + rng.randint(0, 4)) for i in range(5)],
                 ['Employees'] + [f'{2_900 + 640 * i + rng.randint(0, 90):,}' for i in range(5)]]
        add(table([['Fiscal year'] + YEARS] + rows, [3072, 1200, 1200, 1200, 1200, 1200], right_cols=(1, 2, 3, 4, 5)))
        body(1)
    h3('2. History')
    add(table([['Date', 'Event']] + [[f'{rng.choice(["March", "May", "July", "October"])} {2011 + i}', e] for i, e in enumerate(HISTORY)], [1800, 7272]))
    h3('List of Subsidiaries')
    kinds = ['Store operations', 'Store operations', 'Real estate holding', 'Coffee roasting', 'Distribution', 'Gift card issuance']
    subs = [[f'Nimbus Coffee {REGIONS[k % 8]} {["LLC", "Stores LLC", "Properties LLC", "Roasting LLC", "Logistics LLC", "Cards LLC"][k // 8]}',
             REGIONS[k % 8], usd(jitter(250 + 60 * (k % 5))), kinds[k // 8], f'{100 if k % 7 else rng.uniform(51, 90):.1f}', 'Officers serve concurrently'] for k in range(45)]
    add(table([['Name', 'Location', 'Capital (USD thousands)', 'Principal business', 'Voting interest (%)', 'Relationship']] + subs,
              [2272, 1300, 1300, 1600, 1100, 1500], right_cols=(2, 4)))
    for sec in ['3. Description of Business', '4. Subsidiaries and Affiliates', '5. Employees']:
        h3(sec)
        body(6)
        add(table([['Segment', 'Employees', 'Average age (years)', 'Average tenure (years)']]
                  + [[s, f'{rng.randint(40, 3900):,}', f'{rng.uniform(24, 41):.1f}', f'{rng.uniform(1.5, 7):.1f}'] for s in SEGMENTS],
                  [3072, 2000, 2000, 2000], right_cols=(1, 2, 3), bold_rows=(5,)))
    h2('Item 2. Business')
    for i, sec in enumerate(['1. Management Policy, Business Environment and Issues to Address', '2. Sustainability', '3. Risk Factors',
                             "4. Management's Discussion and Analysis of Financial Condition, Results of Operations and Cash Flows",
                             '5. Material Contracts', '6. Product and Menu Development']):
        if i:
            add(page_break())
        h3(sec)
        for k in range(rng.randint(8, 15)):
            add(para(f'({k + 1}) ' + rng.choice(RISKS), bold=True, keep=True))
            body(2)
        rows = [[s, usd(jitter(v / 1.12)), usd(jitter(v)), f'{rng.uniform(102, 124):.1f}'] for s, v in
                [('Beverages', 176_000), ('Food', 58_000), ('Merchandise', 21_000), ('Beans', 57_000), ('  of which Store', 268_000), ('  of which Online', 44_000), ('Total', 312_000)]]
        add(table([['Category', 'FY2024 (USD thousands)', 'FY2025 (USD thousands)', 'Change (% of prior year)']] + rows,
                  [3072, 2000, 2000, 2000], right_cols=(1, 2, 3), bold_rows=(7,)))
    h2('Item 3. Properties')
    for sec in ['1. Overview of Capital Expenditures', '2. Principal Properties', '3. Plans for New Facilities and Disposals']:
        h3(sec)
        body(3)
        places = [('Headquarters (Northfield)', 'Offices'), ('Northfield roastery', 'Roasting and training'), ('Eastbay distribution center', 'Warehouse')] + [(f'Stores in {r} ({rng.randint(12, 34)} stores)', 'Coffee shops') for r in REGIONS]
        add(table([['Location', 'Use', 'Buildings (USD thousands)', 'Equipment (USD thousands)', 'Leasehold improvements (USD thousands)', 'Total (USD thousands)']]
                  + [[p, u, *(usd(jitter(v)) for v in (3_800, 5_200, 6_400)), usd(jitter(15_400))] for p, u in places],
                  [2272, 1400, 1300, 1300, 1500, 1300], right_cols=(2, 3, 4, 5)))
        add(table([['Region', 'Stores at beginning of year', 'Opened', 'Closed', 'Stores at end of year']]
                  + [[r, str(a := rng.randint(12, 30)), str(o := rng.randint(0, 4)), str(c := rng.randint(0, 1)), str(a + o - c)] for r in REGIONS],
                  [2672, 1800, 1500, 1300, 1800], right_cols=(1, 2, 3, 4)))
    h2('Item 4. Information about the Registrant')
    for sec in ['1. Shares', '2. Repurchase of Shares', '3. Dividend Policy', '4. Corporate Governance']:
        h3(sec)
        body(5)
        add(table([['Name', 'Address', 'Shares held', 'Ownership (%)']] + [[holder(), address(), f'{rng.randint(80, 4_900) * 1000:,}', f'{rng.uniform(0.3, 12):.2f}'] for k in range(10)],
                  [2872, 2400, 1900, 1900], right_cols=(2, 3)))
    h3('(2) Directors and Executive Officers')
    titles = ['Chief Executive Officer and Director', 'President and Chief Operating Officer', 'Chief Financial Officer and Director', 'Director', 'Director',
              'Independent Director', 'Independent Director', 'Independent Director (Audit Committee Chair)', 'Vice President, Roasting and Sourcing',
              'Vice President, Store Operations', 'Vice President, Supply Chain', 'General Counsel and Corporate Secretary']
    career_items = ['Joined Nimbus Coffee, Inc. as store manager', 'District Manager, Nimbus Coffee, Inc.', 'Director of Store Operations, Nimbus Coffee, Inc.',
                    'Head Roaster, Nimbus Coffee, Inc.', 'Director of Coffee Sourcing, Nimbus Coffee, Inc.', 'Controller, Nimbus Coffee, Inc.',
                    'Partner, Fenwick & Oakes LLP (accounting firm, fictional)', 'Certified public accountant', 'Admitted to the bar',
                    'Regional Director, Harborview Bakeries Co. (fictional)', 'Vice President, Nimbus Coffee, Inc.', 'Director, Nimbus Coffee, Inc. (current)',
                    'Professor of Business, Westridge State College (fictional)', 'Buyer, Glenhaven Grocers Inc. (fictional)']
    for k in range(24):
        add(para(f'{titles[k % len(titles)]}: {person()} (born {date(1958, 1988)})', bold=True, keep=True))
        career = [[f'{rng.randint(1985, 2025)}-{rng.randint(1, 12):02d}', c] for c in rng.sample(career_items, rng.randint(6, 12))]
        add(table([['Date', 'Career summary']] + sorted(career), [2000, 7072]))
    add(page_break())
    h3('(3) Stock Options')
    for k in range(8):
        add(para(f'Stock options granted under the {2018 + k} Equity Incentive Plan (resolution of the board of directors, {date(2018 + k, 2018 + k)})', bold=True, keep=True))
        items = ['Grantees (category and number)', 'Number of options', 'Class and number of shares issuable on exercise', 'Exercise price per share (USD)',
                 'Exercise period', 'Issue price and amount credited to capital on exercise (USD)', 'Conditions of exercise', 'Transfer of options',
                 'Treatment of options on a merger or reorganization']
        add(table([['', 'At fiscal year end', 'At the end of the month before filing']] + [[i, f'{rng.randint(10, 9999):,}', f'{rng.randint(10, 9999):,}'] for i in items],
                  [4072, 2500, 2500], right_cols=(1, 2)))
        add(para('Note: ' + paragraph_text(2)))
        add(page_break())
    h2('Item 5. Financial Information')
    for period, columns, rev, assets in [
            ('Fiscal year 2024, consolidated', ['FY2023', 'FY2024'], 276_000, 368_000), ('Fiscal year 2025, consolidated', ['FY2024', 'FY2025'], 312_000, 412_000),
            ('Fiscal year 2024, parent company', ['FY2023', 'FY2024'], 241_000, 331_000), ('Fiscal year 2025, parent company', ['FY2024', 'FY2025'], 272_000, 369_000),
            ('Three months ended March 31, 2026', ['Q1 2025', 'Q1 2026'], 78_500, 421_000), ('Six months ended June 30, 2026', ['H1 2025', 'H1 2026'], 164_000, 433_000),
            ('Three months ended June 30, 2026', ['Q2 2025', 'Q2 2026'], 85_600, 433_000)]:
        h3(f'1. Financial Statements ({period})')
        body(2)
        balance_sheets(columns, assets)
        cash_flows(columns, income_statement(columns, rev))
        h3('Notes to the Financial Statements')
        for k, note in enumerate(NOTES):
            add(para(f'{k + 1}. {note}', bold=True, keep=True))
            body(3 if k % 4 == 0 else 2)
            if k % 2 == 0:
                labels = [n for n, w in CURRENT_ASSETS + NONCURRENT_ASSETS + CURRENT_LIAB + NONCURRENT_LIAB if w is not None]
                add(table([['Item'] + columns] + [[rng.choice(labels), usd(v := jitter(assets * rng.uniform(0.005, 0.08))), usd(jitter(v * 1.1))] for _ in range(6)],
                          [4072, 2500, 2500], right_cols=(1, 2)))
        add(page_break())
    h2('Item 6. Share Handling')
    add(table([[k, v] for k, v in [('Fiscal year', 'January 1 to December 31'), ('Annual meeting of stockholders', 'Within three months after the end of each fiscal year'),
                                   ('Record date', 'December 31'), ('Trading unit', '100 shares'), ('Transfer agent', 'Kestrel Bay Stock Transfer Company (fictional)'),
                                   ('Public notices', 'Electronic public notice (fictional)')]], [3000, 6072], header_rows=0))
    h1('Part III — Special Information')
    body(4)
    h1('Part IV — Information on the Listing of Shares')
    for sec in ['Item 1. Transfers of Shares by Related Parties', 'Item 2. Allotments to Third Parties', 'Item 3. Principal Stockholders']:
        h2(sec)
        body(3)
        add(table([['Name', 'Address', 'Shares held', 'Ownership of total shares outstanding (%)']]
                  + [[holder(), address(), f'{rng.randint(10, 4_900) * 1000:,}', f'{rng.uniform(0.1, 12):.2f}'] for k in range(60)],
                  [2872, 2400, 1900, 1900], right_cols=(2, 3)))
    h2('Report of Independent Registered Public Accounting Firm')
    body(12)


build()

W_NS = ('xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" '
        'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"')
SECT = ('<w:sectPr><w:headerReference w:type="default" r:id="rId3"/><w:footerReference w:type="default" r:id="rId4"/>'
        '<w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1418" w:right="1417" w:bottom="1418" w:left="1417" w:header="851" w:footer="567" w:gutter="0"/></w:sectPr>')
DOCUMENT = f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<w:document {W_NS}><w:body>{"".join(blocks)}{SECT}</w:body></w:document>'
STYLES = (
    f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<w:styles {W_NS}>'
    f'<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="{BODY_FONT}" w:hAnsi="{BODY_FONT}" w:cs="{BODY_FONT}"/><w:sz w:val="21"/><w:szCs w:val="21"/><w:lang w:val="en-US"/></w:rPr></w:rPrDefault>'
    '<w:pPrDefault><w:pPr><w:spacing w:after="120" w:line="252" w:lineRule="auto"/><w:jc w:val="both"/></w:pPr></w:pPrDefault></w:docDefaults>'
    '<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style>'
    + ''.join(
        f'<w:style w:type="paragraph" w:styleId="Heading{k}"><w:name w:val="heading {k}"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/>'
        f'<w:pPr><w:keepNext/><w:spacing w:before="{b}" w:after="{a}"/><w:jc w:val="left"/><w:outlineLvl w:val="{k - 1}"/></w:pPr>'
        f'<w:rPr><w:rFonts w:ascii="{HEAD_FONT}" w:hAnsi="{HEAD_FONT}" w:cs="{HEAD_FONT}"/><w:b/><w:sz w:val="{s}"/><w:szCs w:val="{s}"/></w:rPr></w:style>'
        for k, s, b, a in [(1, 28, 360, 180), (2, 24, 240, 120), (3, 21, 180, 60)])
    + '</w:styles>'
)
HEADER = (f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<w:hdr {W_NS}><w:p><w:pPr><w:jc w:val="right"/></w:pPr>'
          f'{run(COMPANY + " — " + TITLE + " (fictional)", size=16)}</w:p></w:hdr>')
FOOTER = (f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<w:ftr {W_NS}><w:p><w:pPr><w:jc w:val="center"/></w:pPr>'
          f'{run("- ")}<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> PAGE </w:instrText></w:r>'
          f'<w:r><w:fldChar w:fldCharType="separate"/></w:r>{run("1")}<w:r><w:fldChar w:fldCharType="end"/></w:r>{run(" -")}</w:p></w:ftr>')
SETTINGS = (f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<w:settings {W_NS}><w:zoom w:percent="100"/><w:defaultTabStop w:val="720"/>'
            '<w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat></w:settings>')
CT = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
      '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>'
      '<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'
      '<Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>'
      '<Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/>'
      '<Override PartName="/word/header1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/>'
      '<Override PartName="/word/footer1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml"/>'
      '<Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/>'
      '<Override PartName="/docProps/app.xml" ContentType="application/vnd.openxmlformats-officedocument.extended-properties+xml"/></Types>')
ROOT_RELS = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
             '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>'
             '<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/>'
             '<Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties" Target="docProps/app.xml"/></Relationships>')
DOC_RELS = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
            '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>'
            '<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/>'
            '<Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/>'
            '<Relationship Id="rId4" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer" Target="footer1.xml"/></Relationships>')
CORE = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">'
        f'<dc:title>{COMPANY} {TITLE} (fictional)</dc:title><dcterms:created xsi:type="dcterms:W3CDTF">2026-09-29T00:00:00Z</dcterms:created><dcterms:modified xsi:type="dcterms:W3CDTF">2026-09-29T00:00:00Z</dcterms:modified></cp:coreProperties>')
APP = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"><Application>Microsoft Office Word</Application></Properties>'


def main(path):
    parts = [('[Content_Types].xml', CT), ('_rels/.rels', ROOT_RELS), ('word/document.xml', DOCUMENT), ('word/_rels/document.xml.rels', DOC_RELS),
             ('word/styles.xml', STYLES), ('word/settings.xml', SETTINGS), ('word/header1.xml', HEADER), ('word/footer1.xml', FOOTER),
             ('docProps/core.xml', CORE), ('docProps/app.xml', APP)]
    with zipfile.ZipFile(path, 'w', zipfile.ZIP_DEFLATED) as z:
        for name, text in parts:
            info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            z.writestr(info, text)
    print(f'{path}: {len(blocks)} blocks')


if __name__ == '__main__':
    main(sys.argv[1])
