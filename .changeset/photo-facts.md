---
egui-pop-api: major
egui-pop-i18n: major
egui-pop-platform: major
egui-pop-views: major
egui-pop-editors: major
egui-pop-imaging: major
egui-pop-test: major
egui-pop: major
---

A photo's facts in the file picker's preview pane (egui-pop-api;
`docs/files.md`): `PhotoFacts` (when it was taken, camera, lens, shutter,
aperture, ISO, focal length, rating), given by a provider with
`Preview::photo`, shows in one order whichever provider read it: the exposure
on one line (`1/250 s  f/5.6  ISO 200`), the rating as five stars, read "3 of
5" by a screen reader. EXIF's date form reads as a date. Agents read them as
`photo` in the preview's metadata. French included. To move: `Preview` gains a
field and is `#[non_exhaustive]`; make one with `Preview::new` and its
builders.
