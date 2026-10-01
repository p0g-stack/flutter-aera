#!/usr/bin/env python3
"""Checks two sim frames of the counter app: START (before the taps) and
AFTER. The app bar must be at the top of AFTER (an upright frame: the bar's
tint at the top, the body's near-white at the bottom) and the frames must
differ (the taps reached the app). Standard library only."""
import os
import sys

sys.path.insert(0, os.path.dirname(__file__))
from png_compare import pixel, read_png  # noqa: E402

start, after = read_png(sys.argv[1]), read_png(sys.argv[2])
w, h = after[0], after[1]
top, bottom = pixel(after, w // 2, 60), pixel(after, w // 2, h - 10)
print(f"top {top} bottom {bottom}")
if sum(top) >= sum(bottom):
    sys.exit("the app bar is not at the top: the frame is upside down or blank")
if start[2] == after[2]:
    sys.exit("the taps did not change the frame")
