//! The strings the views show, by key with English fallbacks,
//! translated through the provider installed with
//! `egui_pop_core::strings::install`. Retry buttons use core's
//! [`RETRY`](egui_pop_core::strings::RETRY).

use egui_pop_core::strings::StrKey;

/// A tree's accessible name, unless the application gives one.
pub const TREE: StrKey = StrKey::new("egui-pop-views-tree", "Tree");
/// A tree's filter field: its placeholder and accessible name.
pub const TREE_FILTER: StrKey = StrKey::new("egui-pop-views-tree-filter", "Filter");
/// Beside a tree's filter field: how many nodes match of how many.
pub const TREE_FILTER_COUNT: StrKey = StrKey::new(
    "egui-pop-views-tree-filter-count",
    "{ $count } of { $total }",
);
/// A tree's filter that matches nothing: beside the field and as the
/// tree's one row.
pub const TREE_FILTER_NONE: StrKey = StrKey::new("egui-pop-views-tree-filter-none", "No match");
/// Announced to screen readers when a tree's filter changes.
pub const TREE_FILTER_ANNOUNCE: StrKey = StrKey::new(
    "egui-pop-views-tree-filter-announce",
    "{ $count } of { $total } match the filter",
);
/// A list's accessible name, unless the application gives one.
pub const LIST: StrKey = StrKey::new("egui-pop-views-list", "List");
/// A table's accessible name, unless the application gives one.
pub const TABLE: StrKey = StrKey::new("egui-pop-views-table", "Table");
/// A branch whose children are on their way.
pub const LOADING: StrKey = StrKey::new("egui-pop-views-loading", "Loading…");
/// A branch whose children could not be loaded.
pub const FAILED: StrKey = StrKey::new("egui-pop-views-failed", "Could not load: { $reason }");
/// An empty branch, shown under it when expanded.
pub const EMPTY: StrKey = StrKey::new("egui-pop-views-empty", "Empty");
/// The rename field's accessible name.
pub const RENAME: StrKey = StrKey::new("egui-pop-views-rename", "Rename { $name }");
/// A list item's drag handle: its accessible name.
pub const ITEM_MOVE: StrKey = StrKey::new("egui-pop-views-item-move", "Move { $name }");
/// A column header's accessible name when sorted up.
pub const SORTED_UP: StrKey =
    StrKey::new("egui-pop-views-sorted-up", "{ $column }, sorted ascending");
/// A column header's accessible name when sorted down.
pub const SORTED_DOWN: StrKey = StrKey::new(
    "egui-pop-views-sorted-down",
    "{ $column }, sorted descending",
);
/// A header's context menu: sort the table up by this column.
pub const SORT_ASCENDING: StrKey = StrKey::new("egui-pop-views-sort-ascending", "Sort ascending");
/// A header's context menu: sort the table down by this column.
pub const SORT_DESCENDING: StrKey =
    StrKey::new("egui-pop-views-sort-descending", "Sort descending");
/// A header's context menu: leave the table unsorted.
pub const SORT_CLEAR: StrKey = StrKey::new("egui-pop-views-sort-clear", "Unsorted");

/// A thumbnail grid's accessible name, unless the application gives one.
pub const LIBRARY: StrKey = StrKey::new("egui-pop-views-library", "Library");
/// A filmstrip's accessible name.
pub const FILMSTRIP: StrKey = StrKey::new("egui-pop-views-filmstrip", "Filmstrip");
/// The comparison view's accessible name.
pub const COMPARE: StrKey = StrKey::new("egui-pop-views-compare", "Compare");
/// The rating stars' name, and the filter menu's rating entry.
pub const RATING: StrKey = StrKey::new("egui-pop-views-rating", "Rating");
/// A star of a thumbnail, rating its image.
pub const RATE_STARS: StrKey = StrKey::new("egui-pop-views-rate-stars", "Rate { $stars } of 5");
/// The colour labels' names.
pub const RED: StrKey = StrKey::new("egui-pop-views-red", "Red");
pub const YELLOW: StrKey = StrKey::new("egui-pop-views-yellow", "Yellow");
pub const GREEN: StrKey = StrKey::new("egui-pop-views-green", "Green");
pub const BLUE: StrKey = StrKey::new("egui-pop-views-blue", "Blue");
pub const PURPLE: StrKey = StrKey::new("egui-pop-views-purple", "Purple");
/// A colour label's toggle button.
pub const LABEL_TOGGLE: StrKey = StrKey::new("egui-pop-views-label-toggle", "{ $label } label");
/// The filter menu's label entry.
pub const LABEL: StrKey = StrKey::new("egui-pop-views-label", "Label");
/// The filter menu's flag entry.
pub const FLAG: StrKey = StrKey::new("egui-pop-views-flag", "Flag");
/// The flags' names.
pub const PICK: StrKey = StrKey::new("egui-pop-views-pick", "Pick");
pub const REJECT: StrKey = StrKey::new("egui-pop-views-reject", "Reject");
pub const UNFLAGGED: StrKey = StrKey::new("egui-pop-views-unflagged", "Unflagged");
/// A collapsed stack's button.
pub const EXPAND_STACK: StrKey = StrKey::new(
    "egui-pop-views-expand-stack",
    "Expand the stack of { $count }",
);
/// An expanded stack's button.
pub const COLLAPSE_STACK: StrKey = StrKey::new(
    "egui-pop-views-collapse-stack",
    "Collapse the stack of { $count }",
);
/// A chip's remove button.
pub const REMOVE: StrKey = StrKey::new("egui-pop-views-remove", "Remove { $item }");
/// The tag input's name and hint.
pub const ADD_TAG: StrKey = StrKey::new("egui-pop-views-add-tag", "Add a tag");
/// The search field's name and hint.
pub const SEARCH: StrKey = StrKey::new("egui-pop-views-search", "Search");
/// The search field's menu of criteria.
pub const ADD_FILTER: StrKey = StrKey::new("egui-pop-views-add-filter", "Add a filter");
/// The search field's clear button.
pub const CLEAR_FILTERS: StrKey = StrKey::new("egui-pop-views-clear-filters", "Clear the search");
/// The filter chips.
pub const FILTER_RATING: StrKey =
    StrKey::new("egui-pop-views-filter-rating", "Rating ≥ { $rating }");
pub const FILTER_LABEL: StrKey = StrKey::new("egui-pop-views-filter-label", "Label: { $label }");
pub const FILTER_FLAG: StrKey = StrKey::new("egui-pop-views-filter-flag", "Flag: { $flag }");
pub const FILTER_TAG: StrKey = StrKey::new("egui-pop-views-filter-tag", "Tag: { $tag }");
pub const FILTER_DATES: StrKey = StrKey::new("egui-pop-views-filter-dates", "{ $from } to { $to }");
pub const FILTER_FROM: StrKey = StrKey::new("egui-pop-views-filter-from", "From { $from }");
pub const FILTER_TO: StrKey = StrKey::new("egui-pop-views-filter-to", "Until { $to }");
/// The comparison's "best" toggle.
pub const MARK_BEST: StrKey = StrKey::new("egui-pop-views-mark-best", "Best of the comparison");

/// The viewer's toolbar: the previous page.
pub const VIEWER_BACK: StrKey = StrKey::new("egui-pop-views-viewer-back", "Back");
/// The viewer's toolbar: the next page.
pub const VIEWER_FORWARD: StrKey = StrKey::new("egui-pop-views-viewer-forward", "Forward");
/// The viewer's toolbar: the page again.
pub const VIEWER_RELOAD: StrKey = StrKey::new("egui-pop-views-viewer-reload", "Reload");
/// The viewer's address bar's accessible name.
pub const VIEWER_ADDRESS: StrKey = StrKey::new("egui-pop-views-viewer-address", "Address");
/// The viewer's search field's accessible name and hint.
pub const VIEWER_FIND: StrKey = StrKey::new("egui-pop-views-viewer-find", "Find in page");
/// Where the search is: the current match and how many.
pub const VIEWER_FIND_COUNT: StrKey =
    StrKey::new("egui-pop-views-viewer-find-count", "{ $at } of { $count }");
/// The search found nothing.
pub const VIEWER_FIND_NONE: StrKey = StrKey::new("egui-pop-views-viewer-find-none", "No match");
pub const VIEWER_FIND_PREVIOUS: StrKey =
    StrKey::new("egui-pop-views-viewer-find-previous", "Previous match");
pub const VIEWER_FIND_NEXT: StrKey = StrKey::new("egui-pop-views-viewer-find-next", "Next match");
pub const VIEWER_FIND_CLOSE: StrKey =
    StrKey::new("egui-pop-views-viewer-find-close", "Close the search");
/// The page's zoom, a button resetting it.
pub const VIEWER_ZOOM: StrKey = StrKey::new("egui-pop-views-viewer-zoom", "Page zoom: { $zoom }");
pub const VIEWER_ZOOM_IN: StrKey = StrKey::new("egui-pop-views-viewer-zoom-in", "Zoom in");
pub const VIEWER_ZOOM_OUT: StrKey = StrKey::new("egui-pop-views-viewer-zoom-out", "Zoom out");
/// Nothing open yet.
pub const VIEWER_EMPTY: StrKey = StrKey::new(
    "egui-pop-views-viewer-empty",
    "Nothing is open. Help pages and images open here.",
);
/// A page on its way.
pub const VIEWER_LOADING: StrKey =
    StrKey::new("egui-pop-views-viewer-loading", "Loading { $target }…");
/// The bar over a page whose remote images wait for the user.
pub const VIEWER_IMAGES_BLOCKED: StrKey = StrKey::new(
    "egui-pop-views-viewer-images-blocked",
    "Images from { $host } are not loaded.",
);
/// The bar's button: load them, and remember the host.
pub const VIEWER_IMAGES_LOAD: StrKey = StrKey::new(
    "egui-pop-views-viewer-images-load",
    "Load images from { $host }",
);
/// The bar over an HTML page: what it holds that the viewer does not show.
pub const VIEWER_SIMPLIFIED: StrKey = StrKey::new(
    "egui-pop-views-viewer-simplified",
    "Not shown here: { $what }. Open outside shows the page whole.",
);
/// In place of an image that is not shown.
pub const VIEWER_IMAGE_MISSING: StrKey = StrKey::new(
    "egui-pop-views-viewer-image-missing",
    "Image not shown: { $alt }",
);
/// The error page's titles.
pub const VIEWER_NOT_FOUND: StrKey = StrKey::new(
    "egui-pop-views-viewer-not-found",
    "This page does not exist",
);
pub const VIEWER_NO_CONNECTION: StrKey = StrKey::new(
    "egui-pop-views-viewer-no-connection",
    "This page could not be loaded",
);
pub const VIEWER_TOO_LARGE: StrKey = StrKey::new(
    "egui-pop-views-viewer-too-large",
    "This page is too large to show here",
);
pub const VIEWER_UNSUPPORTED: StrKey = StrKey::new(
    "egui-pop-views-viewer-unsupported",
    "This page cannot be shown here",
);
/// A video's button while paused (the `Video` widget and the viewer's page).
pub const VIEWER_VIDEO_PLAY: StrKey = StrKey::new("egui-pop-views-viewer-video-play", "Play");
/// A video's button while playing.
pub const VIEWER_VIDEO_PAUSE: StrKey = StrKey::new("egui-pop-views-viewer-video-pause", "Pause");
/// A video's position slider.
pub const VIEWER_VIDEO_POSITION: StrKey =
    StrKey::new("egui-pop-views-viewer-video-position", "Position");
/// A video's volume slider.
pub const VIEWER_VIDEO_VOLUME: StrKey = StrKey::new("egui-pop-views-viewer-video-volume", "Volume");
/// A video's time.
pub const VIEWER_VIDEO_TIME: StrKey = StrKey::new(
    "egui-pop-views-viewer-video-time",
    "{ $elapsed } / { $total }",
);
/// A video's picture, its accessible name.
pub const VIEWER_VIDEO_PICTURE: StrKey =
    StrKey::new("egui-pop-views-viewer-video-picture", "Video: { $name }");
/// A video before its first picture.
pub const VIEWER_VIDEO_LOADING: StrKey =
    StrKey::new("egui-pop-views-viewer-video-loading", "Opening the video…");
/// A YouTube page whose web view is in a window of its own.
pub const VIEWER_EMBED_OWN_WINDOW: StrKey = StrKey::new(
    "egui-pop-views-viewer-embed-own-window",
    "This video plays in a window of its own",
);
/// A YouTube page's button, once its own window was closed.
pub const VIEWER_EMBED_SHOW: StrKey =
    StrKey::new("egui-pop-views-viewer-embed-show", "Show the video");
/// The card in place of YouTube's player when the panel is smaller than the
/// player's minimum.
pub const VIEWER_EMBED_TOO_SMALL: StrKey = StrKey::new(
    "egui-pop-views-viewer-embed-too-small",
    "The panel is too small for YouTube's player",
);
/// The card's reason: the minimum and the panel's size, in pixels.
pub const VIEWER_EMBED_TOO_SMALL_DETAIL: StrKey = StrKey::new(
    "egui-pop-views-viewer-embed-too-small-detail",
    "YouTube's player needs at least { $min } by { $min } pixels; here it would get { $width } by { $height }. Enlarge the panel, or open the video outside.",
);
/// The title of the error page of a video that cannot be played, and of a
/// `Video` widget's failure.
pub const VIEWER_VIDEO_FAILED: StrKey = StrKey::new(
    "egui-pop-views-viewer-video-failed",
    "This video cannot be played here",
);
/// A `Video` widget without a player (a build without the `video` feature,
/// the web).
pub const VIDEO_NO_PLAYER: StrKey = StrKey::new(
    "egui-pop-views-video-no-player",
    "This application has no video player",
);
/// The viewer's content's accessible name.
pub const VIEWER_PAGE: StrKey = StrKey::new("egui-pop-views-viewer-page", "Page");
/// A code block's accessible name.
pub const VIEWER_CODE_BLOCK: StrKey = StrKey::new("egui-pop-views-viewer-code-block", "Code block");

/// A timeline's accessible name, unless the application gives one.
pub const TIMELINE: StrKey = StrKey::new("egui-pop-views-timeline", "Timeline");
/// A timeline's marker: its accessible name, and what is announced when the
/// keys reach it.
pub const TIMELINE_MARKER: StrKey = StrKey::new(
    "egui-pop-views-timeline-marker",
    "Marker { $label } at { $time }",
);
/// A timeline's cue point, the same way.
pub const TIMELINE_CUE: StrKey =
    StrKey::new("egui-pop-views-timeline-cue", "Cue { $label } at { $time }");
/// A marker added with M or a double click: its label.
pub const TIMELINE_NEW_MARKER: StrKey =
    StrKey::new("egui-pop-views-timeline-new-marker", "Marker { $number }");
/// The history's steps of a timeline.
pub const TIMELINE_ADD_MARKER: StrKey =
    StrKey::new("egui-pop-views-timeline-add-marker", "Add marker");
pub const TIMELINE_MOVE_MARKER: StrKey =
    StrKey::new("egui-pop-views-timeline-move-marker", "Move marker");
pub const TIMELINE_REMOVE_MARKER: StrKey =
    StrKey::new("egui-pop-views-timeline-remove-marker", "Remove marker");
pub const TIMELINE_SET_RANGE: StrKey =
    StrKey::new("egui-pop-views-timeline-set-range", "Set range");
pub const TIMELINE_CLEAR_RANGE: StrKey =
    StrKey::new("egui-pop-views-timeline-clear-range", "Clear range");
pub const TIMELINE_LOOP_ON: StrKey =
    StrKey::new("egui-pop-views-timeline-loop-on", "Loop the range");
pub const TIMELINE_LOOP_OFF: StrKey =
    StrKey::new("egui-pop-views-timeline-loop-off", "Stop looping");
/// The speed shown beside the playhead while shuttling: `2×`, `-4×`.
pub const TIMELINE_SPEED: StrKey = StrKey::new("egui-pop-views-timeline-speed", "{ $speed }×");

/// Every string the views show, for catalogues and tests.
pub const ALL: &[StrKey] = &[
    TREE,
    TREE_FILTER,
    TREE_FILTER_COUNT,
    TREE_FILTER_NONE,
    TREE_FILTER_ANNOUNCE,
    LIST,
    TABLE,
    LOADING,
    FAILED,
    EMPTY,
    RENAME,
    ITEM_MOVE,
    SORTED_UP,
    SORTED_DOWN,
    SORT_ASCENDING,
    SORT_DESCENDING,
    SORT_CLEAR,
    LIBRARY,
    FILMSTRIP,
    COMPARE,
    RATING,
    RATE_STARS,
    RED,
    YELLOW,
    GREEN,
    BLUE,
    PURPLE,
    LABEL_TOGGLE,
    LABEL,
    FLAG,
    PICK,
    REJECT,
    UNFLAGGED,
    EXPAND_STACK,
    COLLAPSE_STACK,
    REMOVE,
    ADD_TAG,
    SEARCH,
    ADD_FILTER,
    CLEAR_FILTERS,
    FILTER_RATING,
    FILTER_LABEL,
    FILTER_FLAG,
    FILTER_TAG,
    FILTER_DATES,
    FILTER_FROM,
    FILTER_TO,
    MARK_BEST,
    VIEWER_BACK,
    VIEWER_CODE_BLOCK,
    VIEWER_FORWARD,
    VIEWER_RELOAD,
    VIEWER_ADDRESS,
    VIEWER_FIND,
    VIEWER_FIND_COUNT,
    VIEWER_FIND_NONE,
    VIEWER_FIND_PREVIOUS,
    VIEWER_FIND_NEXT,
    VIEWER_FIND_CLOSE,
    VIEWER_ZOOM,
    VIEWER_ZOOM_IN,
    VIEWER_ZOOM_OUT,
    VIEWER_EMPTY,
    VIEWER_LOADING,
    VIEWER_IMAGES_BLOCKED,
    VIEWER_IMAGES_LOAD,
    VIEWER_SIMPLIFIED,
    VIEWER_IMAGE_MISSING,
    VIEWER_NOT_FOUND,
    VIEWER_NO_CONNECTION,
    VIEWER_TOO_LARGE,
    VIEWER_UNSUPPORTED,
    VIEWER_PAGE,
    VIEWER_VIDEO_PLAY,
    VIEWER_VIDEO_PAUSE,
    VIEWER_VIDEO_POSITION,
    VIEWER_VIDEO_VOLUME,
    VIEWER_VIDEO_TIME,
    VIEWER_VIDEO_PICTURE,
    VIEWER_VIDEO_LOADING,
    VIEWER_VIDEO_FAILED,
    VIDEO_NO_PLAYER,
    VIEWER_EMBED_OWN_WINDOW,
    VIEWER_EMBED_SHOW,
    VIEWER_EMBED_TOO_SMALL,
    VIEWER_EMBED_TOO_SMALL_DETAIL,
    TIMELINE,
    TIMELINE_MARKER,
    TIMELINE_CUE,
    TIMELINE_NEW_MARKER,
    TIMELINE_ADD_MARKER,
    TIMELINE_MOVE_MARKER,
    TIMELINE_REMOVE_MARKER,
    TIMELINE_SET_RANGE,
    TIMELINE_CLEAR_RANGE,
    TIMELINE_LOOP_ON,
    TIMELINE_LOOP_OFF,
    TIMELINE_SPEED,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_unique_fluent_ids() {
        let mut seen = std::collections::BTreeSet::new();
        for k in ALL {
            assert!(seen.insert(k.key), "`{}` twice", k.key);
            assert!(k.key.starts_with("egui-pop-views-"), "{}", k.key);
            assert!(
                k.key
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "{}",
                k.key
            );
        }
    }
}
