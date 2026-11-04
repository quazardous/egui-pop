---
egui-pop-views: minor
---

The viewer's YouTube page (feature `webview`) follows YouTube's terms. Its
`Referer` names the application, `https://<app id>/`, from
`AppBuilder::app_id` (`viewer::referrer_for`), in place of
`https://localhost/`; `ViewerPlugin::embed_referrer` still replaces it, and
without either YouTube links open outside. The player is never smaller than
200 by 200 of the web view's pixels (`viewer::MIN_PLAYER`,
`viewer::fits_player`): over a smaller panel a card says why, with Open
outside, and the player comes back when the panel grows; its own window
cannot be made smaller. `EmbedBackend::in_panel` says which of the two a
backend does. `viewer::DEFAULT_REFERRER` is deprecated.
