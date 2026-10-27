# Candidate fonts (gallery only)

The other fonts the gallery's **Font pairs** page compares with the pair
egui-pop bundles (Inter and JetBrains Mono NL, chosen on that page on
2026-10-27; their files, licences and sources are in
`crates/egui-pop-core/fonts`). They are gallery assets: the gallery is
not published (`publish = false`), and no published crate includes them. Every one is under the SIL Open Font
License 1.1; each folder holds its `OFL.txt`. OFL fonts may be bundled
and redistributed with any software; only the fonts stay under the OFL,
and a modified font may not use a Reserved Font Name.

Subset here means: kept the code points U+0000-024F (Latin, Latin-1,
Latin Extended-A and B), U+0370-03FF (Greek), U+0400-04FF (Cyrillic),
U+1E00-1EFF (Latin Extended Additional, Vietnamese), U+2000-206F
(punctuation), U+20A0-20CF (currency), U+2100-214F, U+2190-21FF,
U+2200-22FF, U+25A0-25FF and U+FFFD, every OpenType layout feature, with
fontTools 4 (`pyftsubset`), then pinned the axes other than `wght` with
`fonttools varLib.instancer`, as for the bundled pair.

| File | Family | Source | Version | Changes | Size | Full file |
|---|---|---|---|---|---|---|
| `noto/NotoSans-subset.ttf` | Noto Sans | [google/fonts](https://github.com/google/fonts/tree/main/ofl/notosans) `NotoSans[wdth,wght].ttf` (from [notofonts](https://github.com/notofonts/latin-greek-cyrillic)) | 2.015 | subset, `wdth` pinned at 100 | 453 KB | 2049 KB |
| `noto/NotoSansMono-subset.ttf` | Noto Sans Mono | [google/fonts](https://github.com/google/fonts/tree/main/ofl/notosansmono) `NotoSansMono[wdth,wght].ttf` | 2.014 | subset, `wdth` pinned at 100 | 381 KB | 1708 KB |
| `ibm-plex/IBMPlexSans-VF.ttf` | IBM Plex Sans | [google/fonts](https://github.com/google/fonts/tree/main/ofl/ibmplexsans) `IBMPlexSans[wdth,wght].ttf` (from [IBM/plex](https://github.com/IBM/plex)) | 3.201 | none (file renamed) | 537 KB | 537 KB |
| `ibm-plex/IBMPlexMono-Regular.ttf` | IBM Plex Mono | [google/fonts](https://github.com/google/fonts/tree/main/ofl/ibmplexmono) | 2.3 | none | 136 KB | 136 KB |
| `ibm-plex/IBMPlexMono-Bold.ttf` | IBM Plex Mono | the same | 2.3 | none | 138 KB | 138 KB |

IBM Plex has the Reserved Font Name "Plex": a modified (subset) build
must be renamed, so its files are kept as published. Subset like the
others (and renamed), Plex Sans would weigh 347 KB and Plex Mono
Regular 98 KB. Plex Mono has no Greek.

egui's own pair, Ubuntu Light and Hack (`default_fonts`), is the
baseline: already shipped by egui, nothing to add.
