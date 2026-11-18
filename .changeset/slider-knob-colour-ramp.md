---
egui-pop-core: minor
---

Colour ramps on sliders and knobs: `Slider::ramp(min, max)` and `Knob::ramp(min, max)` give a colour for the minimum and one for the maximum (`ramp_stops` for more stops, `ramp_with` for a `Ramp` built beforehand), the value drawn in the colour between them, blended in Oklab over the soft range and clamped outside it. Each colour is a theme token by path (`"danger"`, `"accent.default"`, read in the theme drawn now) or an explicit colour (`RampColor`). By default the slider's fill, or the knob's arc and pointer, take the colour at the value, kept 3:1 against the track in dark and light themes; `ramp_track()` shows the whole ramp on the track, the thumb or the pointer marking the value. The number and the thumb's place still say the value; a high-contrast theme keeps the theme's own fill. `slider::oklab_mix` blends two colours in Oklab. The gallery has a Colour ramps page.
