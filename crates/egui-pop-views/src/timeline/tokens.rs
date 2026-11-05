//! What a timeline is drawn with: its own tokens (`timeline.*`), each read by
//! path and falling back on the system token it aliases, so a theme restyles
//! the ruler, the playhead, the range, the markers, the tracks and the clips
//! with data only.

use egui::{Color32, Context};
use egui_pop_core::theme::Tokens;

/// A timeline's colours and sizes, from the theme installed.
#[derive(Clone, Debug, PartialEq)]
pub struct TimelineTokens {
    /// Behind the lanes (`timeline.background`, else `surface.sunken`).
    pub background: Color32,
    /// Behind the ruler (`timeline.ruler.fill`, else `surface.raised`).
    pub ruler_fill: Color32,
    /// The line under the ruler (`timeline.ruler.border`, else `border.default`).
    pub ruler_border: Color32,
    /// Its ticks (`timeline.ruler.tick`, else `text.muted`).
    pub ruler_tick: Color32,
    /// Its labels (`timeline.ruler.text`, else `text.muted`).
    pub ruler_text: Color32,
    /// The lines down the lanes at the labelled ticks (`timeline.grid`, else
    /// `border.default`).
    pub grid: Color32,
    /// The selected range's fill (`timeline.range.fill`, else `accent.subtle`).
    pub range_fill: Color32,
    /// Its band on the ruler and its edges while it loops
    /// (`timeline.range.edge`, else `accent.default`).
    pub range_edge: Color32,
    /// The same while it does not (`timeline.range.idle`, else `text.muted`).
    pub range_edge_idle: Color32,
    /// The playhead (`timeline.playhead`, else `accent.default`).
    pub playhead: Color32,
    /// A marker without a colour of its own (`timeline.marker`, else
    /// `info`, apart from the playhead's accent).
    pub marker: Color32,
    /// A marker's label (`timeline.marker.text`, else `text.default`).
    pub marker_text: Color32,
    /// Around the marker the keys act on (`timeline.marker.selected`, else
    /// `focus.ring`).
    pub marker_selected: Color32,
    /// The line between two tracks (`timeline.track.border`, else
    /// `border.default`).
    pub track_border: Color32,
    /// A track's name (`timeline.track.text`, else `text.muted`).
    pub track_text: Color32,
    /// A clip without a colour of its own (`timeline.clip.fill`, else
    /// `surface.raised`).
    pub clip_fill: Color32,
    /// A clip's outline (`timeline.clip.border`, else `border.strong`).
    pub clip_border: Color32,
    /// A clip's label (`timeline.clip.text`, else `text.default`).
    pub clip_text: Color32,
    /// A selected clip's outline and a trimming handle
    /// (`timeline.clip.selected`, else `accent.default`).
    pub clip_selected: Color32,
    /// The outline while the timeline has the keyboard focus (`focus.ring`).
    pub focus: Color32,
    /// The speed badge's fill and text (`surface.raised`, `text.strong`).
    pub badge_fill: Color32,
    pub badge_text: Color32,
    /// The ruler's height (`size.control.height`).
    pub ruler_height: f32,
    /// The markers' lane (`size.control.height`, four fifths of it).
    pub lane_height: f32,
    /// A track's height unless it or the timeline says
    /// (`size.control.height`, twice).
    pub track_height: f32,
    /// The labels' size (`font.scale.xs`).
    pub font_size: f32,
    /// Corners (`radius.widget`).
    pub radius: f32,
}

impl TimelineTokens {
    /// The tokens of the theme installed in `ctx`.
    pub fn of(ctx: &Context) -> TimelineTokens {
        let t = Tokens::of(ctx);
        let c = |path: &str, fallback: Color32| t.color(path).unwrap_or(fallback);
        TimelineTokens {
            background: c("timeline.background", t.surface.sunken),
            ruler_fill: c("timeline.ruler.fill", t.surface.raised),
            ruler_border: c("timeline.ruler.border", t.border.default),
            ruler_tick: c("timeline.ruler.tick", t.text.muted),
            ruler_text: c("timeline.ruler.text", t.text.muted),
            grid: c("timeline.grid", t.border.default),
            range_fill: c("timeline.range.fill", t.accent.subtle),
            range_edge: c("timeline.range.edge", t.accent.default),
            range_edge_idle: c("timeline.range.idle", t.text.muted),
            playhead: c("timeline.playhead", t.accent.default),
            marker: c("timeline.marker", t.status.info),
            marker_text: c("timeline.marker.text", t.text.default),
            marker_selected: c("timeline.marker.selected", t.focus_ring),
            track_border: c("timeline.track.border", t.border.default),
            track_text: c("timeline.track.text", t.text.muted),
            clip_fill: c("timeline.clip.fill", t.surface.raised),
            clip_border: c("timeline.clip.border", t.border.strong),
            clip_text: c("timeline.clip.text", t.text.default),
            clip_selected: c("timeline.clip.selected", t.accent.default),
            focus: t.focus_ring,
            badge_fill: t.surface.raised,
            badge_text: t.text.strong,
            ruler_height: t.size.control_height,
            lane_height: (t.size.control_height * 0.8).round(),
            track_height: t.size.control_height * 2.0,
            font_size: t.font.scale.xs,
            radius: t.radius.widget,
        }
    }
}
