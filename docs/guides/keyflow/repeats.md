---
title: Repeats & Endings
kind: concept
type: concept
order: 10
---

# Repeats & Endings

Songs loop. Rather than write the same bars again, Keyflow has a few ways to say
"play that again."

## Repeat a bar — `%`

`%` replays the bar before it (from [[rhythm|Rhythm]]):

```kf-src
1  4  %  5        bar 3 repeats bar 2 (the 4)
```

`%` replays the previous bar — engraved, bar 3 repeats the 4:

```kf+
1 | 4 | % | 5
```

## Repeat a line — `xN`

Put `x` and a number at the end of a line to play the whole line that many times:

```kf-src
1 4 5 1 x2        these four bars, played twice (eight bars)
```

## Repeat a span — `|: … :|`

Wrap bars in repeat barlines to mark a section that plays twice:

```kf-src
|: 1 4 | 5 1 :|
```

The `|: … :|` only marks the repeat — the bars are still written once. Use bar
lines `|` between them when the span is more than one bar (as above), since a
bare `|: 1 4 5 1 :|` would pack everything into a single bar.

## First and second endings

When a repeat ends differently the second time, mark the alternate endings with
`[1]` and `[2]` — placed right after a bar line, on the bars they apply to:

```kf-src
|: 1 | [1] 4 :| | [2] 5 |
```

That reads: play the `1` bar, take the **first ending** (`4`) and repeat back;
the second time through, skip to the **second ending** (`5`). Write `[1,2]` for a
bar shared by both endings.

## That's the whole tour

You can now read and write every part of a Keyflow chart — the
[[structure|header]] and [[sections|sections]], the
[[chords|chords]], [[rhythm|rhythm]], [[melody|melody]], and
[[lyrics|lyrics]], the [[key-meter-changes|key and meter changes]],
the [[annotations|markings]], and now the repeats that tie a song's form
together. Open a real `.kf` file and start playing with it — that's the best
teacher from here.

---

Previous: [[annotations|Annotations & Expression]] · Up: [[keyflow|Keyflow Guide]]
