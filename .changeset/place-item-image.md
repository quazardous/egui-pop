---
egui-pop-api: minor
egui-pop: minor
---

A place can show a picture in its icon's place: `PlaceItem::image(source)`
takes an egui image source (a texture the application loaded, or a
`file://`, `bytes://` or `https://` URI read by egui's image loaders), as a
detected camera shows its photo in an import window's sources. The picture
fills the icon's square, cropped to its middle and rounded as the theme
rounds widgets, faded on a dimmed item; the item's registry icon is drawn
while it loads or if it fails, and the item is still said by its name. The
item's new `image` field holds a `PlaceImage`.
