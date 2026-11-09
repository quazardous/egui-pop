---
egui-pop-api: minor
egui-pop-model: patch
egui-pop-i18n: patch
---

The Settings app's search reaches inside its pages: it finds the
commands of the shortcuts and gestures editors by name and by key, the
settings a custom page edits, and the sections by their title and
keywords. The left panel lists each section found with the items found in
it; choosing one (or "Show" on the page of results) lands on its page,
the item marked and scrolled to, and said selected to assistive
technology. A widget kind registers its search with
`Admin::widget_search`, and reads the item landed on with
`Admin::marked` and `Admin::take_key`. A custom page claims the settings
it edits, so they no longer show a second time on a generated page; drawn
with `settings_window::setting_row` in the Settings app, they take the
app's rows (value scopes, origin, modified bar). A deep link to such a
setting (`admin.open {"key": …}`) lands on its page.
