#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (c) ibis inc.
# Additional terms under AGPL-3.0 section 7(b): see NOTICE.
"""A made-up video-list CSV of about 100MB in which some cells hold about 100
line breaks (quoted cells). Used to try a CSV with many-line cells.

  python3 scripts/benchmark-data/gen_csv_multiline.py target/repro/multiline.csv [MB] [EVERY]

MB is the size to reach (default 100); EVERY is how often a row has the
many-line cell (default 1000; row 3 always has one).
"""
import random
import sys


def main(path, mb=100, every=1000):
    rng = random.Random(20261014)
    target = mb * 1024 * 1024
    size = 0
    with open(path, "w", encoding="utf-8", newline="") as f:
        head = "No,Title,URL,Views,Likes,Comments,Subscribers,Published,Seconds,Length,Description,Updated\r\n"
        f.write(head)
        size += len(head.encode())
        i = 1
        while size < target:
            i += 1
            if i == 3 or i % every == 0:
                desc = "\n".join(
                    ("Link %d https://example.com/item/%d" % (k, rng.randrange(10**6)) if k % 3 else "")
                    for k in range(100)
                )
            else:
                desc = "Sample description %d" % i
            row = '%d,Sample video %d,https://example.com/watch?v=%08d,%d,%d,%d,%d,2026/10/01 16:28,%d,0:07:33,"%s",2026/10/07 17:59\r\n' % (
                i, i, i, rng.randrange(10**6), rng.randrange(10**4), rng.randrange(10**3), rng.randrange(10**5), rng.randrange(900), desc)
            f.write(row)
            size += len(row.encode())


if __name__ == "__main__":
    main(sys.argv[1], int(sys.argv[2]) if len(sys.argv) > 2 else 100, int(sys.argv[3]) if len(sys.argv) > 3 else 1000)
