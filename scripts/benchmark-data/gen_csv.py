#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (c) ibis inc.
# Additional terms under AGPL-3.0 section 7(b): see NOTICE.
"""A CSV benchmark data generator. Default is as large as an Excel sheet:
a header and 1,048,575 rows (Excel's 1,048,576 rows in all) of 20 columns.
The first column is a date (yyyy/mm/dd, one day per row from 2000/01/01),
the others made-up KPIs as plain numbers. The random seed is fixed, so it
is the same file every time.

Usage:
  python3 scripts/benchmark-data/gen_csv.py [path] [--rows N]
  python3 scripts/benchmark-data/gen_csv.py test/benchmark/kpi-1m-rows.csv --rows 1000000
  python3 scripts/benchmark-data/gen_csv.py test/benchmark/kpi-1m-rows.csv 1000000
"""
import argparse
import datetime
import random
import sys

DEFAULT_ROWS = 1_048_575
COLUMNS = [
    # (name, base, spread, decimals)
    ("Sessions", 52000, 9000, 0),
    ("Users", 31000, 6000, 0),
    ("New Users", 9800, 2500, 0),
    ("Page Views", 184000, 30000, 0),
    ("Bounce Rate (%)", 41.5, 6.0, 1),
    ("Avg Session (s)", 182, 40, 0),
    ("Conversions", 1240, 260, 0),
    ("Conversion Rate (%)", 2.38, 0.45, 2),
    ("Orders", 1105, 230, 0),
    ("Revenue", 4865000, 950000, 0),
    ("Average Order Value", 4402, 520, 0),
    ("Refunds", 37, 12, 0),
    ("Ad Spend", 612000, 140000, 0),
    ("Impressions", 2410000, 380000, 0),
    ("Clicks", 48200, 9100, 0),
    ("CTR (%)", 2.0, 0.35, 2),
    ("CPC", 12.7, 2.6, 1),
    ("ROAS", 7.95, 1.6, 2),
    ("Support Tickets", 214, 48, 0),
]


def generate_csv(path: str, rows: int = DEFAULT_ROWS):
    rng = random.Random(20261012)
    start = datetime.date(2000, 1, 1)
    print(f"Generating CSV with {rows:,} data rows (total {rows + 1:,} rows) -> {path}")
    with open(path, "w", encoding="utf-8", newline="") as f:
        f.write("Date," + ",".join(name for name, *_ in COLUMNS) + "\r\n")
        for i in range(rows):
            day = start + datetime.timedelta(days=i)
            cells = [day.strftime("%Y/%m/%d")]
            for _, base, spread, decimals in COLUMNS:
                v = max(0.0, rng.gauss(base, spread / 2))
                cells.append(f"{v:.{decimals}f}")
            f.write(",".join(cells) + "\r\n")
    print(f"Done generating {path}")


def main():
    parser = argparse.ArgumentParser(description="Generate benchmark CSV file.")
    parser.add_argument("path", nargs="?", default="test/benchmark/kpi-max-rows.csv", help="Output path")
    parser.add_argument("pos_rows", nargs="?", type=int, default=None, help="Number of data rows (positional)")
    parser.add_argument("--rows", "-r", type=int, default=None, help="Number of data rows")
    args = parser.parse_args()

    rows = args.rows if args.rows is not None else (args.pos_rows if args.pos_rows is not None else DEFAULT_ROWS)
    generate_csv(args.path, rows)


if __name__ == "__main__":
    main()
