gallery-i18n-intro = The interface follows the language setting: egui-pop's own text, what plugins declare, and this page's catalogue.
gallery-i18n-language = Language
gallery-i18n-system = System
gallery-i18n-show-keys = Show translation keys
gallery-i18n-own = egui-pop's own strings
gallery-i18n-declared = Declared titles
gallery-i18n-formats = Plurals, numbers and dates
gallery-i18n-count = Images
gallery-i18n-photos =
    { $count ->
        [one] { $count } image selected
       *[other] { $count } images selected
    }
gallery-i18n-size = Size: { NUMBER($mib, maximumFractionDigits: 1) } MiB
gallery-i18n-date = Taken on { DATETIME($when, dateStyle: "full") }
gallery-i18n-missing = Missing in { $locale }: { $count }
gallery-i18n-none-missing = Nothing missing in { $locale }.
tip-gallery-next-page =
    .text = Alt+Down and Alt+Up go to the next and the previous page.
    .title = Go through the pages
tip-gallery-next-theme =
    .text = Ctrl+Shift+T switches to the next theme; the theme editor starts from it.
    .title = Try every theme
tip-gallery-palette =
    .text = Ctrl+Shift+P opens the command palette.
    .title = Find any command
tip-gallery-export-hint =
    .text = This button exports with the settings used last time.
    .title = Exports keep your choices
