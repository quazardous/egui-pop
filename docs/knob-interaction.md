# Knob interaction: a study

How should a knob on a screen be driven? This study reads egui-pop's
knob as it is today, compares it with the audio, photo and video
software people already know and with the web's knob libraries, and
ends with a recommendation for the knob's API: a drag mode the
application chooses, a travel in points independent of the knob's size,
one fine modifier, the wheel, reset, typing, and an endless knob.

Read on 2026-11-11, against egui-pop's `main` (`crates/egui-pop-core/src/knob.rs`
and `slider.rs`).

## Summary

- **Linear drag wins.** Nearly every audio application and plug-in
  maker drives knobs by a straight drag, up for more (Ableton Live,
  Bitwig, Logic, Reason, Native Instruments, FabFilter, u-he, Cubase's
  "Linear" mode, Arturia's "linear" mode). Circular drag survives as an
  option (Cubase, Arturia, VCV Rack, JUCE, MultitrackStudio, the VST 3
  host knob mode) and as the default of general-purpose toolkits (Qt's
  `Dial`, Nexus UI), never as the only way.
- **Hybrids confuse.** The places that pick the mode from where the
  press lands (FabFilter: the arrow turns, the centre drags; some synths:
  straying from vertical engages circular) draw steady complaints about
  falling into the wrong mode by accident. egui-pop's knob is such a
  hybrid today.
- **The mode is often the host's or the person's choice.** VST 3 has a
  call for the host to tell a plug-in its knob mode; Cubase, VCV Rack,
  MultitrackStudio and some Arturia instruments have a preference; JUCE
  lets the application, or the person through an optional menu, pick
  among circular, horizontal, vertical, and both.
- **Travel is fixed in pixels**, not proportional to the knob: JUCE's
  default is 250 pixels for the full range, egui-pop's 200 points;
  others make it a per-control speed (VCV Rack, u-he) or a preference
  (Reason).
- **Shift is the fine modifier** in almost every audio application
  (Live, Bitwig, Logic, Reason, Kontakt, FabFilter, u-he, webaudio-controls);
  Ctrl in FL Studio and JUCE's velocity toggle; darktable uses Ctrl for
  finer and Shift for coarser.
- **The wheel is expected on hover** in plug-ins (FabFilter, JUCE by
  default), but in a scrolling panel it hijacks the scroll: the web's
  long fight with number inputs, and VCV Rack's wheel control being a
  menu option.
- **Double click** means reset in Live, Bitwig, darktable and egui-pop,
  but typing in FabFilter; Ctrl/Cmd-click or Alt/Option-click resets in
  Logic, Reason, Kontakt, FabFilter and FL Studio.

## egui-pop's knob today

`Knob` (`egui_pop_core::knob`) shares the slider's terms (soft and hard
range, step, unit, default, hook) and its history contract. How it is
driven:

| Gesture | Today |
|---|---|
| Drag | **Hybrid, chosen at the press**: a press within 60 % of the radius from the centre drags **vertically** (up is more, `Knob::travel` points, 200 by default, for the soft range, whatever the knob's size); a press farther out drags **around**, relative to the press angle, the soft range over the 270° sweep. Horizontal motion is ignored in the vertical mode. |
| Fine | Shift, or the profile's fine modifier (Ctrl by default): ten times finer, from where it is. Coarse is never applied to a drag. |
| Wheel | The slider's rule: by default (the "guarded" profiles) the wheel scrolls the panel and steps the knob only when it has the focus or Alt is held over it; the darktable profile steps the knob under the pointer. One step per frame with wheel input, whatever the delta. |
| Keys | When focused: arrows step, Page Up/Down ten steps, Home/End the soft ends, Enter opens the typed entry. |
| Double click | Back to the default. |
| Typing | Enter on a focused knob; no pointer gesture opens it. |
| Cursor | A drag cursor; the pointer is neither hidden nor held. |
| Accessibility | An AccessKit slider, with value text, range, step, increment, decrement and set-value actions. |

What makes it hard to control:

1. **The rim turns.** The value's arc and the pointer line are drawn
   from a quarter to nine tenths of the radius, so a person aiming at
   the pointer, or at the arc, presses in the outer 40 % and gets the
   circular mode. On a 48-point dial that ring is about 9 points wide;
   one press lands in it, the next one not, and the same motion does two
   different things.
2. **Circular is hard near the centre.** In the circular mode, the angle
   swept per point of motion grows as the pointer nears the centre, so a
   drag that passes close to it jumps.
3. **Shift means two things.** Shift makes a drag finer, but with the
   default profile Shift makes the wheel and the arrows coarser (the
   slider's rule: Ctrl finer, Shift coarser).
4. **The wheel does nothing by default.** Over an unfocused knob the
   guarded profiles scroll the panel; a person who expects the wheel to
   turn a knob, as in every plug-in, sees nothing happen.
5. **Up only.** A person who drags sideways, as on a horizontal slider,
   gets nothing.

## What others do

A dash: the sources read do not say.

| Software | Drag | Full range | Fine | Wheel | Reset | Typing |
|---|---|---|---|---|---|---|
| Ableton Live | vertical | — | Shift | — | double click, Delete | type digits on a selected control |
| Bitwig Studio | vertical (up/down) | — | Shift (also pressed mid-drag) | — | double click | Ctrl/Cmd-click |
| Logic Pro | vertical | — | Shift (before the drag) | after selecting the control, wheel or trackpad | Option-click | — |
| Cubase / Nuendo | preference: Circular, Relative Circular, Linear | — | Shift (linear mode) | — | — | — |
| Reason | vertical (up = clockwise) | preference "Mouse Knob Range" | Shift | — | Ctrl/Cmd-click | — |
| FL Studio | — | — | Ctrl, or both buttons | — | Alt-click | right-click menu, "Type in value" |
| FabFilter | vertical from the centre; rotational from the arrow | slow motion finer | Shift (drag and wheel) | hover | Ctrl/Cmd-click | double click |
| Native Instruments (Kontakt, Massive) | vertical | — | Shift | — | Ctrl/Cmd-click | — |
| u-he | vertical | per-control setting (`normedMouseTravel`) | Shift | — | — | — |
| Arturia (V instruments) | circular by default, linear as an option (some via the host) | — | right-click or Alt/Shift-click (x4 finer) | — | — | — |
| JUCE `Slider` | application's style: `Rotary` (circular, absolute), `RotaryHorizontalDrag`, `RotaryVerticalDrag`, `RotaryHorizontalVerticalDrag` (right plus up) | 250 px by default (`setMouseDragSensitivity`) | velocity mode, toggled by Ctrl/Alt/Cmd | on by default, 15 % of the range per unit, at least one interval | double click to a set value (`setDoubleClickReturnValue`) | text box |
| VST 3 host knob mode | the host says: `kCircularMode` (jump), `kRelativCircularMode`, `kLinearMode` (vertical) | the plug-in's | the plug-in's | the plug-in's | the plug-in's | the plug-in's |
| VCV Rack 2 | preference: linear, scaled linear (each with a locked cursor or not), absolute rotary, relative rotary; a module may make a knob horizontal | `speed` per knob | — | a View menu option | — | — |
| MultitrackStudio | preference: Rotary or Vertical/Horizontal; touch: Horizontal/Rotating or Vertical | — | — | — | — | — |
| darktable (bauhaus sliders) | the slider follows the drag; right-click pop-up for fine control, finer the farther the pointer is from the marker | the slider's width | Ctrl finer, Shift coarser (x10, preference) | hover | double click | right-click, then type |
| Lightroom Classic (sliders) | horizontal | the slider's width | Shift (reported, conflicting) | reported on hover, varies by system | double click on the label | — |
| DaVinci Resolve (wheels and rings) | drag the ring or the puck | — | — | — | double click in the ring, reset button | value fields under the wheel |
| Qt Quick `Dial` | `inputMode`: Circular (default, absolute), Horizontal, Vertical (relative) | — | none | — | none | none |
| Nexus UI `Dial` | `interaction`: radial (default), vertical, horizontal; `mode` relative (default) or absolute | — | none | — | none | none |
| webaudio-controls | up or right is more | `sensitivity` | Shift (drag and wheel, by `step`) | yes | Ctrl/Cmd-click | — |
| jQuery Knob | absolute: the value follows the pointer's angle | the dial | none | yes | none | none |

## Findings

**Circular versus linear.** Circular drag maps the screen to a physical
gesture the mouse does not have: the hand moves the wrist in a straight
line, and turning in a circle around a small target is slow and
imprecise, worst near the centre, where the angle per point of motion
explodes. Developers report that newcomers drag knobs up and down and
that "radial is confusing", and several moved to linear for that reason
(linux-audio-dev). The DAW manuals read here (Live, Bitwig, Logic, Reason,
Kontakt) document a vertical drag as the knob's gesture. Circular keeps one strength: the travel can
be set by moving farther from the knob (a longer lever, finer control),
which Arturia's old instruments and darktable's pop-up use for fine
adjustment.

**Hybrids.** FabFilter switches to rotation when the press lands on the
knob's arrow; users ask to turn it off, report falling into it by
accident, and FabFilter's own designer says he hardly uses it since the
vertical drag gained its slow-down. Plug-ins that switch to circular
when the drag strays from vertical draw the same complaint. A mode
chosen from where the press lands is invisible: the person cannot tell
which one they will get.

**Vertical, horizontal or both.** Vertical is the audio convention: a
knob sits in a column of knobs on a channel strip and up reads as more.
Horizontal suits rows of knobs and matches the horizontal sliders next
to them (Lightroom, darktable). JUCE's `RotaryHorizontalVerticalDrag`
and webaudio-controls add both components (right plus up), so either
gesture works and a diagonal one is faster; the cost is that a sloppy
vertical drag picks up its horizontal wander (one synth's knobs were
reported to jump from exactly that).

**Distance.** The toolkits read here fix the travel in pixels (JUCE
250, egui-pop 200 points) or as a per-knob speed (VCV Rack, u-he's
`normedMouseTravel`); none ties it to the knob's size, which would make
a small knob too fast for the same motion. A fixed travel gives the same feel to every knob, small or
large; 200 to 300 points is the common range, and some ranges need
their own (u-he exposes it per control because one cutoff knob felt too
fast).

**Fine.** Shift dominates the audio world; Ctrl is FL Studio's and
JUCE's. darktable's photo convention is the reverse pair (Ctrl finer,
Shift coarser), and egui-pop's default profile follows it for sliders.
What matters most is that the modifier means the same thing for drag,
wheel and arrows on one control, and that pressing it mid-drag works
without a jump (Bitwig, egui-pop).

**Wheel.** Plug-in users expect the wheel to turn the knob under the
pointer (FabFilter, JUCE on by default). In a scrolling panel that
steals the scroll: the web's number inputs are the long-running
example, with users changing values by accident while scrolling and
browsers backing the behaviour out. Hover-to-adjust is safe in a fixed
layout (a plug-in window, a HUD over an image) and unsafe in a
scrolling form. JUCE's step is a fraction of the range per wheel unit,
never less than one interval; webaudio-controls steps by `step`, and so
does egui-pop. A touchpad sends a stream of small deltas: stepping once
per frame regardless of size makes a fast swipe and a slow one equal.

**Reset and typing.** Double click to reset is the most common binding
(Live, Bitwig, darktable, JUCE); Ctrl/Cmd-click (Reason,
Kontakt, FabFilter, webaudio-controls) and Alt/Option-click (Logic, FL
Studio) are the alternatives. Typing is reached by double click
(FabFilter), Ctrl/Cmd-click (Bitwig), right-click (FL Studio,
darktable) or a focused control and digits (Live).

**Cursor.** JUCE hides the pointer and lets it move without bound in
its velocity mode, VCV Rack offers a "locked cursor" mode, and the
browser's Pointer Lock API exists for the same reason: a long vertical
drag otherwise stops at the screen's edge, and a knob near the top of
the screen has little room above it.

**Endless knobs.** A value that wraps (an angle, a hue) or has no bounds
has no arc ends to show; it is driven by relative motion, and the
hardware analogue (an encoder, the Surface Dial's rotation delta) sends
increments, not positions. JUCE's `stopAtEnd(false)` and Qt's
`wrapping` make a dial wrap.

**Touch and pen.** Touch prefers a single axis that does not fight the
panel's scroll: MultitrackStudio's touch mode notes that a vertical
knob stops vertical scrolling of the mixer, and offers horizontal or
rotating instead. A pen behaves as a mouse but cannot be hidden and
warped back on every platform.

**Keyboard and screen readers.** The WAI-ARIA slider pattern applies to
a knob as is: role slider, value, minimum, maximum, value text; arrows
step, Page Up/Down take a larger step, Home/End go to the ends.
egui-pop already does this.

## Recommendation

The owner's view, that a knob is driven by the wheel or by a vertical
or a horizontal drag at the application's choice, is what the evidence
supports, with two additions: offer both axes at once as an option, and
keep circular as an option rather than as a zone of the knob.

```rust
/// How a drag turns a knob.
pub enum KnobDrag {
    /// Up is more (the audio convention). The default.
    Vertical,
    /// Right is more.
    Horizontal,
    /// Right and up both add (JUCE's horizontal-vertical drag).
    Both,
    /// Around the centre, relative to the press: never a jump.
    Circular,
}
```

- **One drag mode per knob, never chosen by where the press lands.**
  `Knob::drag(KnobDrag)` sets it for one knob; the pointer profile gives
  the default for all (`options.knob_drag`, `vertical` in every built-in
  profile), so an application, or the person in its settings, sets it
  once. The hybrid centre/rim zones go.
- **Travel in points, not a fraction of the size.** Keep
  `Knob::travel(points)` for the soft range, 200 by default; the same
  travel for every axis, `Both` included. A per-profile multiplier
  (a "knob speed" setting) scales it for all knobs.
- **One fine modifier, the same for drag, wheel and arrows:** the
  profile's `gesture.value.fine`. The knob drops its hard-coded Shift.
  Audio-minded profiles bind Shift; the default and darktable profiles
  keep Ctrl finer and Shift coarser. Pressed mid-drag, it takes effect
  from where the value is, as today.
- **Wheel.** Keep the profile's rule (guarded or on values), and add a
  per-knob `Knob::wheel(KnobWheel::Hover)` for knobs in places that do
  not scroll (a HUD over an image, a fixed plug-in strip), where hover
  is safe and expected. The step stays the knob's step, scaled by the
  modifiers; a touchpad's stream accumulates its delta into steps
  instead of one step per frame.
- **Double click resets** to the default, as today; Ctrl/Cmd-click as an
  optional reset binding in the profiles that copy applications using it.
- **Typing:** keep Enter on a focused knob, and add a pointer way in:
  a right-click menu entry "Type a value", or the profile's binding.
- **Long drags:** hide the pointer during a drag and put it back where
  the drag started at its end, where the platform allows it (egui's
  `ViewportCommand::CursorGrab` and `CursorVisible`), so the travel
  never meets the screen's edge. Opt-in at first.
- **Endless knob:** a face with no arc ends (ticks that turn with the
  value), driven by the same `KnobDrag`, reporting relative nudges
  (`Change` deltas) rather than positions; a wrapping value wraps
  instead of clamping.
- **Touch:** `Horizontal` is the better default under a finger in a
  vertically scrolling panel; the profile for touch says so.
- **Keys and accessibility:** unchanged.

Where the evidence differs from the owner's view: toolkits that serve
everyone (Qt, Nexus UI, jQuery Knob) default to circular, and some
plug-ins keep it for its lever effect (farther is finer). The
recommendation keeps `Circular` as a choice, relative to the press so it
never jumps, and does not make it the default. The audio world also
expects the wheel on hover, which egui-pop's guarded profiles refuse
over scrolling panels: the per-knob hover wheel answers it where it is
safe.

## Sources

DAWs and plug-ins:

- Ableton Live keyboard shortcuts: <https://www.ableton.com/en/manual/live-keyboard-shortcuts>
- Bitwig Studio, user interfacing: <https://www.bitwig.com/userguide/latest/user_interfacing>
- Logic Pro 9, adjusting plug-in parameters: <https://help.apple.com/logicpro/mac/9.1.6/en/logicpro/usermanual/chapter_10_section_5.html>
- Cubase / Nuendo, Editing - Controls preferences: <https://steinberg.help/cubase_pro_artist/v9/en/cubase_nuendo/topics/preferences/preferences_editing_controls_r.html>
- Steinberg forum, changing a plug-in's knob mode: <https://forums.steinberg.net/t/changing-3rd-party-plugin-knob-mode/631332>
- Reason, common operations: <https://docs.propellerheads.se/reason9/CommonOperationsAndConcepts.02.4.html>
- FL Studio basics: <https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/basics_interface.htm>
- FabFilter Pro-Q 4, knobs: <https://www.fabfilter.com/help/pro-q/using/knobs>
- FabFilter forum, rotation mode: <https://prod.fabfilter.com/forum/add-comment?id=601>, <https://prod.fabfilter.com/forum/add-comment?id=2627>
- Kontakt Player, user interface elements: <https://www.native-instruments.com/ni-tech-manuals/kontakt-player-manual/en/user-interface-elements>
- KVR, u-he fine control and `normedMouseTravel`: <https://www.kvraudio.com/forum/viewtopic.php?p=9099521>
- Arturia Jupiter-8V 2 manual (knob modes): <https://www.manualsdir.com/manuals/47449/arturia-jupiter-8v-2.html?page=52>
- Arturia Moog Modular V 2.6 manual: <https://www.manualsdir.com/manuals/39764/arturia-moog-modular-26.html?page=141>
- MultitrackStudio preferences (Knob Style): <https://multitrackstudio.com/preference.php>
- KVR, circular knobs in a synth: <https://www.kvraudio.com/forum/viewtopic.php?p=6309446>

Frameworks:

- JUCE `Slider`: <https://docs.juce.com/master/classSlider.html> and its source, `juce_Slider.cpp` (250-pixel default, wheel step, rotary menu): <https://github.com/juce-framework/JUCE/blob/master/modules/juce_gui_basics/widgets/juce_Slider.cpp>
- JUCE forum, rotary knob sensitivity: <https://forum.juce.com/t/rotary-knob-movement-sensitivity/48280>
- VST 3 `IEditController2::setKnobMode`: <https://steinbergmedia.github.io/vst3_dev_portal/pages/Technical+Documentation/Change+History/3.1.0/IEditController2.html>
- Ardour, VST 3 knob mode: <https://discourse.ardour.org/t/vst3-knob-mode/112544>
- VCV Rack `Knob`: <https://vcvrack.com/docs-v2/Knob_8hpp_source.html>; knob modes commit: <https://git.kx.studio/VCVRack/Rack/commit/b93e889c75906fe2df566838bd1fab2d39553c88>
- VCV community, dragging behaviour: <https://community.vcvrack.com/t/idea-for-better-dragging-behaviour/2838>
- Qt Quick `Dial`: <https://doc.qt.io/qt-6/qml-qtquick-controls-dial.html>

Photo and video:

- darktable, module controls: <https://docs.darktable.org/usermanual/development/en/darkroom/processing-modules/module-controls/>
- Lightroom Classic, sliders and the wheel (community): <https://community.adobe.com/t5/lightroom-classic-discussions/slider-scrolling-with-mouse-wheel/m-p/14244667>
- DaVinci Resolve 18.6 manual, colour wheels: <https://www.steakunderwater.com/VFXPedia/__man/Resolve18-6/DaVinciResolve18_Manual_files/part2715.htm>

Web libraries:

- webaudio-controls: <https://g200kg.github.io/webaudio-controls/docs/index.html>
- Nexus UI `Dial`: <https://github.com/nexus-js/ui/blob/master/lib/interfaces/dial.js>
- precision-inputs: <https://github.com/jhnsnc/precision-inputs>
- jQuery Knob: <https://github.com/aterrien/jQuery-Knob>

Usability, accessibility and the platform:

- linux-audio-dev, knobs and widget design: <https://lists.linuxaudio.org/archives/linux-audio-dev/2004-June/008370.html>
- WAI-ARIA Authoring Practices, slider pattern: <https://www.w3.org/WAI/ARIA/apg/patterns/slider/>
- MDN, Pointer Lock API: <https://developer.mozilla.org/en-US/docs/Web/API/Pointer_Lock_API>
- Firefox, the wheel changing number inputs: <https://bugzilla.mozilla.org/show_bug.cgi?id=1741469>
