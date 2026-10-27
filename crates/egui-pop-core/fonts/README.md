# Bundled fonts

egui-pop's default pair, compiled in by the `bundled-fonts` feature (on
by default): Inter for interface text, JetBrains Mono NL for values and
code. Without the feature, these files are not included in the binary
and egui's own pair (Ubuntu Light and Hack) is the default.

The fonts are under the SIL Open Font License 1.1, not under egui-pop's
MIT or Apache-2.0 licence: each folder holds its `OFL.txt`, which ships
with the fonts. OFL fonts may be bundled, embedded and redistributed
with any software; only the fonts stay under the OFL. Neither family
declares a Reserved Font Name.

Subset here means: kept the code points U+0000-024F (Latin, Latin-1,
Latin Extended-A and B), U+0370-03FF (Greek), U+0400-04FF (Cyrillic),
U+1E00-1EFF (Latin Extended Additional, Vietnamese), U+2000-206F
(punctuation), U+20A0-20CF (currency), U+2100-214F, U+2190-21FF,
U+2200-22FF, U+25A0-25FF and U+FFFD, every OpenType layout feature, with
fontTools 4 (`pyftsubset`), then pinned the axes other than `wght` with
`fonttools varLib.instancer`.

| File | Family | Source | Version | Changes | Size | Full file |
|---|---|---|---|---|---|---|
| `inter/Inter-subset.ttf` | Inter | [google/fonts](https://github.com/google/fonts/tree/main/ofl/inter) `Inter[opsz,wght].ttf` (from [rsms/inter](https://github.com/rsms/inter)) | 4.001 | subset, `opsz` pinned at 14 | 416 KB | 877 KB |
| `jetbrains-mono/JetBrainsMonoNL-Regular-subset.ttf` | JetBrains Mono NL | [JetBrains/JetBrainsMono](https://github.com/JetBrains/JetBrainsMono) release 2.304, `fonts/ttf/` | 2.304 | subset | 101 KB | 209 KB |
| `jetbrains-mono/JetBrainsMonoNL-Bold-subset.ttf` | JetBrains Mono NL | the same | 2.304 | subset | 103 KB | 211 KB |

Inter is variable: its `wght` axis draws every weight egui-pop asks
for. JetBrains Mono NL is the cut without programming ligatures (egui
cannot turn `calt` off); it has no variable build, so its regular and
bold weights are carried, and the weights between draw the nearest.
Its digits are tabular, so values drawn in it line up.
