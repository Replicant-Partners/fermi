# v0.13.0 — You can read it now

Testers reported the console was too small to read comfortably. It was, and
the reason turned out to be two separate problems that happened to land on
the same complaint.

The obvious one: the type was tiny. The three most-used sizes in the whole
app were 9px, 10px and 11px, with some numerals at 7px. That is below the
legible floor for a monospace face on a normal display, and six nominal
"sizes" packed into a 3px range don't read as a hierarchy — they read as
noise.

The less obvious one, and the worse one: **two of the three text colours
were effectively invisible.** The greyest one measured 1.7:1 against the
background — the accessibility floor for body text is 4.5:1 — and it was
being used as a text colour in 269 places. Making that text bigger would
have changed nothing. You cannot fix a colour the eye can't separate from
its background by setting it in a larger size.

And none of it was adjustable. Roughly 3,400 hardcoded pixel values meant
there was no setting to change, so "it's too small" had no answer.

## What's new

- **Text size control** — `Ctrl` `+` / `-` to adjust, `Ctrl` `0` to reset.
  There's also a stepper at the bottom of the sidebar showing the current
  value, and **View → Larger Text**. Range is 90%–160%; your choice is
  remembered between launches.

- **Everything scales together, not just the font.** Type, padding, row
  heights, column widths and panel sizes all move as one. This is the part
  that matters: scaling only the text would have made labels overflow their
  boxes and columns collide. Text grows, and so does the space around it.

- **A real type scale.** Body text is now 15px at the default (it was 11px),
  the smallest badge numerals are 11.5px (they were 8px, and in one chart,
  7px). Each step is far enough from the next to actually signal hierarchy.

- **Readable secondary text.** The three text tiers now measure 9.6:1, 6.8:1
  and 5.5:1 against the background — all above the accessibility floor. In
  practice: timestamps, column headers, driver names, status chips and chart
  axis labels were all washing into the background, and now they don't.

- **The default is 115%, not 100%.** A preference nobody finds doesn't fix
  anything, so the console starts at a comfortable size. If you liked the old
  density, `Ctrl` `-` twice gets you close, and it'll stick.

## Fixes

- Chart axis labels were still using the old washed-out grey. The three chart
  modules had each declared their own private copy of the colour palette, so
  they never picked up earlier contrast fixes. They now share one palette.

- The sign-in screen could lose its title and the top of the sign-in card,
  with no way to scroll back up to them. Happened on short windows before
  this release; a large text size made it easy to hit.

- The sidebar footer — connection status, credit balance, shortcuts hint —
  could be pushed off the bottom of the screen. Worst case, the text-size
  stepper itself disappeared, which is exactly when you'd want it. The nav
  list now scrolls instead, and the footer stays put.

## Known issues

- Text size is uniform across the app. There's no separate control for, say,
  keeping tables dense while enlarging prose.

- Some very dense tables get tighter at the top of the range. Nothing is
  clipped, but if a column looks cramped at 150%+, that's known — a report
  naming the panel is useful.

## Breaking changes

None.

## Upgrade notes

Nothing to do. Update and restart, and the console opens at 115%.

If you preferred the previous density, press `Ctrl` `-` — the setting is
saved to `~/.config/fermi-console/ui.json` (on macOS,
`~/Library/Application Support/FermiConsole/ui.json`) and survives updates.
Deleting that file restores the default.

To pin a size without changing your saved preference — for a screenshot or a
demo — launch with `FERMI_UI_SCALE=1.4`.
