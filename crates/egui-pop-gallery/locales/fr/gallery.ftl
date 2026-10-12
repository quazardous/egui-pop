gallery-i18n-intro = L’interface suit le réglage de langue : le texte d’egui-pop, ce que déclarent les plugins, et le catalogue de cette page.
gallery-i18n-language = Langue
gallery-i18n-system = Système
gallery-i18n-show-keys = Afficher les clés de traduction
gallery-i18n-own = Le texte d’egui-pop
gallery-i18n-declared = Titres déclarés
gallery-i18n-formats = Pluriels, nombres et dates
gallery-i18n-count = Images
gallery-i18n-photos =
    { $count ->
        [one] { $count } image sélectionnée
       *[other] { $count } images sélectionnées
    }
gallery-i18n-size = Taille : { NUMBER($mib, maximumFractionDigits: 1) } Mio
gallery-i18n-date = Prise le { DATETIME($when, dateStyle: "full") }
gallery-i18n-missing = Manquantes en { $locale } : { $count }
gallery-i18n-none-missing = Rien ne manque en { $locale }.
