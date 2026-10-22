# Le texte d’egui-pop et ce que déclarent ses plugins, en français.
# Traduction de ../en/egui-pop.ftl, sous les mêmes clés.

category-clipboard = Presse-papiers

category-dialogs = Dialogues

category-edit = Édition

category-editor = Éditeur

category-file = Fichier

category-help = Aide

category-jobs = Tâches

category-library = Photothèque

category-navigation = Navigation

category-notifications = Notifications

category-terminal = Terminal

category-theme = Thème

category-values = Valeurs

category-view = Affichage

category-window = Fenêtre

command-app-devtools =
    .description = Ouvre ou ferme le panneau des outils de développement (quand le réglage Outils de développement est activé).
    .title = Outils de développement

command-app-fullscreen =
    .description = Occupe tout l’écran ; le bord du haut ramène la sortie.
    .title = Plein écran

command-app-palette =
    .description = Chercher et lancer n’importe quelle commande.
    .title = Palette de commandes

command-app-quit =
    .description = Fermer l’application.
    .title = Quitter

command-app-settings =
    .description = Tous les réglages, avec recherche, et les raccourcis clavier.
    .title = Réglages

command-app-settings-page =
    .description = Ouvre la fenêtre des réglages sur une page personnalisée (son identifiant).
    .title = Page de réglages

command-app-window-close =
    .description = Ferme la fenêtre, comme son bouton de fermeture.
    .title = Fermer la fenêtre

command-app-window-maximize =
    .description = Agrandit la fenêtre, ou la restaure.
    .title = Agrandir

command-app-window-menu =
    .description = Ouvre le menu de la fenêtre (restaurer, réduire, agrandir, fermer).
    .title = Menu de la fenêtre

command-app-window-minimize =
    .description = Réduit la fenêtre.
    .title = Réduire

command-clipboard-copy =
    .description = Copie depuis une source (`source`) dans l’emplacement de la session (`slot`) ; `cut` supprime ce qui a été copié ; `dry_run` dit ce qui serait copié et laissé de côté.
    .title = Copier depuis une source

command-clipboard-offers =
    .description = Ce que contient un emplacement (`slot`) : son type, son titre, ses formats et sa révision ; avec `target`, chaque façon de l’y coller, avec son aperçu.
    .title = Ce que propose le presse-papiers

command-clipboard-paste =
    .description = Colle l’emplacement de la session (`slot`) dans une cible (`target`), dans un format (`format`), par une transformation (`transform`), seulement certaines parties (`parts`), vérifié contre `if_rev` ; `dry_run` dit le plan et les changements sans les faire.
    .title = Coller dans une cible

command-clipboard-read =
    .description = L’élément d’un emplacement (`slot`, celui de la session par défaut), contenu compris ; celui de l’utilisateur est refusé à un agent sauf si l’hôte le permet.
    .title = Lire le presse-papiers

command-clipboard-write =
    .description = Met un élément dans un emplacement (`slot`, celui de la session par défaut) : son format (`format`), ses données (`data`), un titre (`title`).
    .title = Écrire le presse-papiers

command-code-actions =
    .description = Demande au fournisseur d’assistance les actions et corrections rapides au curseur.
    .title = Actions sur le code

command-code-complete =
    .description = Demande au fournisseur d’assistance des complétions au curseur.
    .title = Proposer des complétions

command-code-definition =
    .description = Demande au fournisseur d’assistance où est défini le symbole sous le curseur, et y va.
    .title = Aller à la définition

command-code-fold =
    .description = Replie le bloc au curseur (ou à `line`, à partir de 0).
    .title = Replier

command-code-hover =
    .description = Demande au fournisseur d’assistance ce qu’est le symbole sous le curseur, comme le survol.
    .title = Afficher le survol

command-code-set-diagnostics =
    .description = Remplace les diagnostics d’un éditeur de code par l’étape `code.diagnostics`.
    .title = Définir les diagnostics

command-code-toggle-breakpoint =
    .description = Pose ou retire un point d’arrêt sur la ligne du curseur (ou `line`, à partir de 0).
    .title = Basculer le point d’arrêt

command-code-unfold =
    .description = Déplie le repli au curseur (ou à `line`, à partir de 0).
    .title = Déplier

command-code-unfold-all =
    .description = Déplie tous les replis.
    .title = Tout déplier

command-context-go =
    .description = Va à un chemin de lieux (`places` : chaque `frame` avec son `payload`).
    .title = Aller à un lieu

command-context-hold =
    .description = Retient des groupes de la pile (`groups`, vide : toute la pile ; `place` pour les lieux) avec une raison (`reason`) : les transitions qui les touchent sont refusées jusqu’à leur libération. Rend son jeton.
    .title = Retenir la pile de contextes

command-context-read =
    .description = Les lieux et les modes actifs, leur révision, les retenues et les cadres déclarés, et ce que montre la barre d’état (`status` : l’identifiant, le texte, le ton, le compte et la progression de chaque élément visible, s’il est occupé, ses comptes d’erreurs et d’avertissements, et sa carte de survol).
    .title = Lire la pile de contextes

command-context-release =
    .description = Libère une retenue de la pile de contextes (`token`).
    .title = Libérer une retenue

command-context-request =
    .description = Demande une transition au propriétaire (`request` : `op` enter, leave, jump, go, set_mode, push_mode, pop_mode, clear_mode ou back), vérifiée contre `if_rev`, appliquée sauf avec `dry_run`.
    .title = Demander une transition de contexte

command-context-set-mode =
    .description = Active un mode (`mode`), ou l’empile (`push` : true) ; avec `group` et sans mode, désactive le groupe.
    .title = Activer un mode

command-crash-dismiss =
    .description = Supprime un rapport de plantage (`path`).
    .title = Supprimer un rapport de plantage

command-crash-reveal =
    .description = Ouvre le fichier d’un rapport de plantage (`path`).
    .title = Afficher un rapport de plantage

command-dialogs-move =
    .description = Déplace le dialogue affiché avec les flèches ; Entrée garde la place, Échap le remet où il était.
    .title = Déplacer le dialogue

command-edit-copy =
    .description = Copier la sélection.
    .title = Copier

command-edit-cut =
    .description = Couper la sélection : copiée une fois sa suppression acceptée.
    .title = Couper

command-edit-paste =
    .description = Coller dans le format le plus riche que l’élément actif accepte.
    .title = Coller

command-edit-pasteSpecial =
    .description = Choisir le format, la transformation ou les parties à coller, chacun avec son aperçu.
    .title = Collage spécial

command-edit-redo =
    .description = Rétablir la dernière modification annulée.
    .title = Rétablir

command-edit-undo =
    .description = Annuler la dernière modification.
    .title = Annuler

command-editor-find =
    .description = Ouvre la barre de recherche ; avec `query` (et `case_sensitive`, `whole_word`, `regex`), cherche et sélectionne la première occurrence après le curseur.
    .title = Rechercher

command-editor-find-next =
    .description = Sélectionne l’occurrence suivante de la recherche.
    .title = Occurrence suivante

command-editor-find-previous =
    .description = Sélectionne l’occurrence précédente de la recherche.
    .title = Occurrence précédente

command-editor-goto-line =
    .description = Ouvre le champ « aller à la ligne » ; avec `line` (à partir de 1), y place le curseur.
    .title = Aller à la ligne

command-editor-read =
    .description = Le texte de l’éditeur (ou les lignes `from_line` à `to_line`, à partir de 0, fin exclue), avec sa version, sa sélection et son état.
    .title = Lire le texte

command-editor-redo =
    .description = Rétablit la dernière modification annulée de l’éditeur.
    .title = Rétablir dans l’éditeur

command-editor-replace =
    .description = Ouvre la barre de recherche avec le remplacement ; avec `query` et `replacement`, remplace l’occurrence au curseur (ou toutes avec `all`).
    .title = Remplacer

command-editor-save =
    .description = Enregistre le texte de l’éditeur par l’étape `edit.save`.
    .title = Enregistrer

command-editor-select =
    .description = Sélectionne de `anchor` à `head` (`{"{"}line, col{"}"}` à partir de 0 ; `head` seul y place le curseur).
    .title = Sélectionner

command-editor-undo =
    .description = Annule la dernière modification de l’éditeur (son propre historique, comme Ctrl+Z dans l’éditeur).
    .title = Annuler dans l’éditeur

command-editor-write =
    .description = Modifie le texte par `edit.change` : tout le `text` ; ou `text` entre `from` et `to` (`{"{"}line, col{"}"}` à partir de 0) ; ou une liste de `changes`. Une seule étape d’annulation.
    .title = Écrire le texte

command-job-cancel =
    .description = Demande à l’hôte d’annuler la tâche `id` ; elle apparaît en cours d’annulation jusqu’à ce que l’hôte confirme.
    .title = Annuler une tâche

command-job-pause =
    .description = Demande à l’hôte de mettre en pause la tâche `id`.
    .title = Mettre une tâche en pause

command-job-resume =
    .description = Demande à l’hôte de reprendre la tâche `id` mise en pause.
    .title = Reprendre une tâche

command-job-retry =
    .description = Demande à l’hôte de relancer la tâche `id` échouée ou annulée.
    .title = Relancer une tâche

command-library-filter =
    .description = Règle le filtre d’une photothèque (`filter` : `text`, `min_rating`, `labels`, `flags`, `tags`, `from`, `to` ; `{"{"}{"}"}` l’efface), par l’étape `library.filter` de son champ de recherche.
    .title = Filtrer des images

command-library-flag =
    .description = Retient, rejette ou démarque des images (`flag` : pick, reject, none) : `keys`, sinon la sélection ; par `library.flag`.
    .title = Marquer des images

command-library-label =
    .description = Pose une étiquette de couleur (`label` : red, yellow, green, blue, purple) sur des images ou l’enlève (`on` ; sinon inversée d’après l’image du curseur) : `keys`, sinon la sélection ; par `library.label`.
    .title = Étiqueter des images

command-library-rate =
    .description = Note des images (`rating`, de 0 à 5) d’une photothèque : `keys`, sinon sa sélection ; par son étape `library.rate`.
    .title = Noter des images

command-library-select =
    .description = Sélectionne des images d’une photothèque par clé (`keys`, le curseur sur `cursor` ou la dernière clé), par son étape `library.select`.
    .title = Sélectionner des images

command-library-tag =
    .description = Ajoute des mots-clés (`add`) à des images et en enlève d’autres (`remove`) : `keys`, sinon la sélection ; par `library.tag`.
    .title = Indexer des images

command-logs-reveal =
    .description = Ouvre le dossier des fichiers journaux de l’application.
    .title = Afficher le dossier des journaux

command-nav-back =
    .description = Dépile le dernier mode empilé, sinon quitte le lieu affiché pour son parent.
    .title = Retour

command-nav-enter =
    .description = Entre dans un lieu sous celui affiché (`frame`, `payload`).
    .title = Entrer dans un lieu

command-nav-up =
    .description = Quitte le lieu affiché pour son parent.
    .title = Remonter au lieu parent

command-notify-act =
    .description = Exécute une action d’une notification (`id`, `action`), comme un clic dessus.
    .title = Agir sur une notification

command-palettes-reset =
    .description = Remet les palettes flottantes là où elles s’ouvrent d’abord, déroulées et à la taille de leur contenu.
    .title = Réinitialiser les palettes

command-palettes-toggle =
    .description = Masque les palettes qui ne sont pas épinglées, ou les affiche de nouveau.
    .title = Masquer ou afficher les palettes

command-status-item-hide =
    .description = Masque un élément de la barre d’état (`id`) à l’endroit affiché, ou l’affiche de nouveau (`hidden` : false).
    .title = Masquer un élément de la barre d’état

command-status-item-hover =
    .description = Ouvre la carte de survol d’un élément de la barre d’état (`id` ; par défaut celui qui a le focus clavier), comme le survol. Échap la ferme.
    .title = Afficher la carte d’un élément de la barre d’état

command-status-item-move =
    .description = Déplace un élément de la barre d’état (`id`) d’un pas vers son début ou sa fin (`toward` : start ou end), à l’endroit affiché.
    .title = Déplacer un élément de la barre d’état

command-status-item-open =
    .description = Ouvre la fenêtre contextuelle d’un élément de la barre d’état (`id`), comme un clic dessus, ou la ferme (`open` : false).
    .title = Ouvrir un élément de la barre d’état

command-theme-generate =
    .description = Écrit une gamme générée à partir d’une couleur source (`seed`, #rrggbb) sur une gamme de couleurs du thème (`scale`) : nuances 1 à 12 et contrast, pour la variante éditée, claire ou sombre ; annulée en une fois.
    .title = Générer une gamme de couleurs

command-theme-export =
    .description = Le thème en cours d’édition comme fichier .tokens.json (`name`) : $extends sa base, les jetons modifiés avec leur type ; une paire claire et sombre en un seul fichier portant sa contrepartie.
    .title = Exporter le thème

command-theme-import =
    .description = Charge un fichier .tokens.json (`text`) dans l’éditeur de thème, par l’étape theme.load ; dit quels jetons ont été laissés de côté (`skipped`) et avec quels avertissements il a été lu (`warnings`).
    .title = Importer un thème

command-theme-read =
    .description = Le thème en cours d’édition : sa base, les jetons modifiés, l’historique et les paires de contraste en échec ; avec `path`, un seul jeton, sa valeur, son alias et ses mesures de contraste.
    .title = Lire le thème

command-theme-redo =
    .description = Rétablit la dernière modification annulée de l’éditeur de thème.
    .title = Rétablir une modification du thème

command-theme-set =
    .description = Donne une valeur à un jeton du thème en cours d’édition (`path`, `value` : une valeur DTCG brute ou l’alias d’un autre jeton, null pour celle du thème de base), par l’étape theme.token ; annulable.
    .title = Modifier un jeton du thème

command-theme-start =
    .description = Repart d’un thème (`base`) dans l’éditeur de thème, son historique vidé, par l’étape theme.load.
    .title = Partir d’un thème

command-theme-undo =
    .description = Annule la dernière modification de l’éditeur de thème, par l’étape theme.token.
    .title = Annuler une modification du thème

command-tips-dismiss =
    .description = Écarte une astuce pour de bon (`tip`, son id), par l’étape tips.dismiss.
    .title = Écarter une astuce

command-tips-list =
    .description = Toutes les astuces enregistrées : leur id, leur titre et leur texte en anglais, si elles sont contextuelles et si elles ont été écartées.
    .title = Lister les astuces

command-tips-reset =
    .description = Oublie quelles astuces ont été écartées.
    .title = Afficher de nouveau toutes les astuces

command-value-get =
    .description = Une valeur avec son descripteur (`path`).
    .title = Lire une valeur

command-value-list =
    .description = Les valeurs sous un préfixe de chemin (`prefix`).
    .title = Lister les valeurs

command-value-redo =
    .description = Rétablir la dernière modification annulée des valeurs.
    .title = Rétablir

command-value-reset =
    .description = Revenir à sa valeur par défaut (`path`).
    .title = Réinitialiser une valeur

command-value-set =
    .description = `path` ; `value`, `step`, `coarse`, `reset`, `top` ou `bottom` ; `force`, `if_seq`.
    .title = Modifier une valeur

command-value-undo =
    .description = Annuler la dernière modification des valeurs.
    .title = Annuler

command-view-interface-zoom-in =
    .description = Agrandit toute l’interface d’un cran.
    .title = Agrandir l’interface

command-view-interface-zoom-out =
    .description = Réduit toute l’interface d’un cran.
    .title = Réduire l’interface

command-terminal-clear =
    .description = Efface l’écran et l’historique (le programme continue).
    .title = Effacer le terminal

command-terminal-close =
    .description = Termine la session du terminal par `terminal.exit` (le programme est arrêté).
    .title = Fermer la session

command-terminal-copy =
    .description = Copie la sélection du terminal dans le presse-papiers.
    .title = Copier la sélection

command-terminal-read =
    .description = Le texte du terminal : son historique et son écran (ou les lignes `from_line` à `to_line`, à partir de 0, fin exclue ; `screen` pour l’écran seul), avec le curseur, la taille et l’état de la session.
    .title = Lire le terminal

command-terminal-restart =
    .description = Relance le programme du terminal par `terminal.spawn` (en fermant d’abord celui qui tourne).
    .title = Redémarrer

command-terminal-send =
    .description = Envoie `text` au programme par `terminal.input` (avec `enter`, suivi d’Entrée).
    .title = Envoyer une saisie

command-theme-counterpart =
    .description = L’autre variante de la paire générée à partir de celle éditée (couleurs en miroir : même teinte et même saturation, luminosité inversée ; alias et autres valeurs tels quels) : les modifications proposées, ou, avec `apply`, écrites en une seule modification.
    .title = Générer l’autre variante

command-theme-switch =
    .description = Édite l’autre variante, claire ou sombre, de la paire en cours d’édition, et l’affiche ; chaque variante garde ses modifications et son historique.
    .title = Basculer la variante éditée

command-view-interface-zoom-reset =
    .description = Rend à l’interface sa taille normale.
    .title = Taille normale de l’interface

command-view-side-bar-toggle =
    .description = Replie la barre latérale de début sur sa bande, ou la déplie.
    .title = Replier la barre latérale

command-view-theme-toggle-light-dark =
    .description = Affiche l’autre variante du thème, claire ou sombre, quand il en a une.
    .title = Basculer entre thème clair et sombre

command-workbench-arrangement-reset =
    .description = Remet les panneaux comme le modèle de la disposition actuelle les place.
    .title = Réinitialiser la disposition

command-workbench-arrangement-save =
    .description = Enregistre la place des panneaux sous un nom.
    .title = Enregistrer la disposition sous…

command-workbench-arrangement-switch =
    .description = Place les panneaux comme une disposition enregistrée ou un modèle.
    .title = Changer de disposition

command-workbench-leave-zen =
    .description = Affiche de nouveau tout l’établi.
    .title = Quitter le mode zen

command-workbench-zen =
    .description = N’affiche que le centre : ni barre de titre, de menus ou d’état, ni zone latérale ou du bas, ni palette. Échap en sort.
    .title = Mode zen

egui-pop-area-bottom = le panneau du bas

egui-pop-area-center = le centre

egui-pop-area-end-side = la barre latérale de fin

egui-pop-area-start-side = la barre latérale de début

egui-pop-busy = Occupé

egui-pop-cancel = Annuler

egui-pop-clipboard-as = Comme { $format }

egui-pop-clipboard-as-through = Comme { $format }, par { $transform }

egui-pop-clipboard-copied-stripped = Copié ; laissé de côté : { $what }

egui-pop-clipboard-loses = perd { $what }

egui-pop-clipboard-not-copied = Non copié : { $reason }

egui-pop-clipboard-not-pasted = Non collé : { $reason }

egui-pop-clipboard-only-part = Seulement { $part }

egui-pop-clipboard-paste-special = Collage spécial

egui-pop-clipboard-pasted-lossy = Collé ; perdu : { $what }

egui-pop-close = Fermer

egui-pop-command-label = { $category } : { $title }

egui-pop-command-with-change = { $title } { $change }

egui-pop-context-back-from-mode = Revenir de { $mode }

egui-pop-context-cannot-leave = { $mode } reste actif : { $reason }

egui-pop-context-dropped = Non restauré : { $frames }

egui-pop-context-go-there = Y aller

egui-pop-context-go-to = Aller à { $place }

egui-pop-context-here = Vous êtes dans { $place }

egui-pop-context-leave-mode = Quitter { $mode }

egui-pop-context-more-places = Lieux au-dessus

egui-pop-context-no-mode = Aucun

egui-pop-context-now-in = Maintenant dans { $place }

egui-pop-context-redone-in = Rétabli dans { $place }

egui-pop-context-refused = Pas maintenant : { $reason }

egui-pop-context-root = Accueil

egui-pop-context-undone-in = Annulé dans { $place }

egui-pop-crash-offer = { $app } s’est fermé de façon inattendue la dernière fois. Un rapport a été enregistré sur cet ordinateur ; rien n’a été envoyé.

egui-pop-crash-show = Afficher le rapport

egui-pop-devtools-arguments = Arguments

egui-pop-devtools-context-keys = Clés de contexte

egui-pop-devtools-focus-chain = Chaîne du focus

egui-pop-devtools-hook-by = { $kind } par { $owner }

egui-pop-devtools-mutates = { $action } (modifie)

egui-pop-devtools-no-hooks = aucun crochet

egui-pop-devtools-off = Les outils de développement sont désactivés (Réglages > Développement > Outils de développement).

egui-pop-devtools-overlays = Calques

egui-pop-devtools-press-a-key = Appuyez sur une touche liée à une commande : sa résolution s’affiche ici.

egui-pop-devtools-tab-collectors = Collecteurs

egui-pop-devtools-tab-focus = Focus

egui-pop-devtools-tab-journal = Journal

egui-pop-devtools-tab-keys = Touches

egui-pop-devtools-tab-scale = Échelle

egui-pop-devtools-tab-steps = Étapes

egui-pop-devtools-waiting = En attente de l’accord suivant après { $keys }

egui-pop-dialog-browse = Parcourir…

egui-pop-dialog-filter = Filtrer

egui-pop-editors-close = Fermer la recherche

egui-pop-editors-code = Éditeur de code

egui-pop-editors-count = { $current } sur { $total }

egui-pop-editors-find = Rechercher

egui-pop-editors-goto = Aller à la ligne

egui-pop-editors-job-log = Journal de { $job }

egui-pop-editors-match-case = Respecter la casse

egui-pop-editors-next = Occurrence suivante

egui-pop-editors-no-results = Aucun résultat

egui-pop-editors-position = Ligne { $line }, colonne { $col }

egui-pop-editors-previous = Occurrence précédente

egui-pop-editors-regex = Expression régulière

egui-pop-editors-replace = Remplacer par

egui-pop-editors-replace-all = Tout remplacer

egui-pop-editors-replace-one = Remplacer

egui-pop-editors-terminal = Terminal

egui-pop-editors-terminal-closed = Session fermée

egui-pop-editors-terminal-ended = Processus terminé

egui-pop-editors-terminal-exited = Processus terminé avec le code { $code }

egui-pop-editors-terminal-refused = Non démarré : { $reason }

egui-pop-editors-text = Éditeur de texte

egui-pop-editors-toggle-replace = Afficher le remplacement

egui-pop-editors-whole-word = Mot entier

egui-pop-fault-body = Le reste de l’application fonctionne. Réessayez, ou signalez le message ci-dessous.

egui-pop-fault-title = { $name } a cessé de fonctionner

egui-pop-field-disabled = Indisponible : { $reason }

egui-pop-field-modified-default = Modifié par rapport à la valeur par défaut

egui-pop-field-modified-saved = Modifié depuis l’enregistrement

egui-pop-field-pending = En cours de vérification

egui-pop-field-required = Obligatoire

egui-pop-field-reset = Rétablir { $label } par défaut

egui-pop-files-all = Tous les fichiers

egui-pop-files-bookmark = Ajouter ce dossier aux favoris

egui-pop-files-bookmarks = Favoris

egui-pop-files-filter = Filtrer, ou saisir un chemin

egui-pop-files-hidden = Fichiers cachés

egui-pop-files-home = Dossier personnel

egui-pop-files-name = Nom du fichier, ou un chemin

egui-pop-files-open = Ouvrir

egui-pop-files-recent = Récents

egui-pop-files-save = Enregistrer

egui-pop-files-unbookmark = Retirer des favoris

egui-pop-fonts-numbers = Les valeurs alignent leurs chiffres :

egui-pop-fonts-origin-bundled = Fournie avec egui

egui-pop-fonts-origin-file = Un de vos fichiers de police

egui-pop-fonts-origin-registered = Fournie avec l’application

egui-pop-fonts-origin-system = Installée sur le système

egui-pop-fonts-picker-hint = Chaque famille est dessinée dans sa propre police, sauf celles du système ; laissez le pointeur sur l’une d’elles pour la prévisualiser ci-dessous.

egui-pop-fonts-preview = Aperçu

egui-pop-fonts-themes = Celle du thème

egui-pop-frame-exit-fullscreen = Quitter le plein écran

egui-pop-frame-maximize = Agrandir

egui-pop-frame-minimize = Réduire

egui-pop-frame-restore = Restaurer

egui-pop-frame-window-menu = Menu de la fenêtre

egui-pop-indicator-done = Terminé

egui-pop-indicator-error = Erreur

egui-pop-indicator-failed = Échec

egui-pop-indicator-invalid = Non valide

egui-pop-indicator-mixed = Mixte

egui-pop-indicator-modified = Modifié

egui-pop-indicator-no-color = Aucune couleur

egui-pop-indicator-off = Éteint

egui-pop-indicator-on = Allumé

egui-pop-indicator-percent = { $percent } %

egui-pop-indicator-pinned = Épinglé

egui-pop-indicator-required = Obligatoire

egui-pop-indicator-warning = Avertissement

egui-pop-input-choices = { $label }, choix

egui-pop-input-clear = Effacer { $label }

egui-pop-input-more = { $label }, plus

egui-pop-job-action-name = { $action } : { $job }

egui-pop-job-asked = { $state }, en attente de l’hôte

egui-pop-job-cancelled = annulée

egui-pop-job-cancelling = annulation en cours

egui-pop-job-dismiss = Retirer

egui-pop-job-done = terminée

egui-pop-job-eta-minutes = environ { $n } min restantes

egui-pop-job-eta-seconds = environ { $n } s restantes

egui-pop-job-failed = échouée

egui-pop-job-hide-log = Masquer le journal de { $job }

egui-pop-job-no-log = Pas encore de journal.

egui-pop-job-of = { $done } sur { $total }

egui-pop-job-pause = Pause

egui-pop-job-paused = en pause

egui-pop-job-refused = Refusé : { $error }

egui-pop-job-resume = Reprendre

egui-pop-job-running = en cours

egui-pop-job-show-log = Afficher le journal de { $job }

egui-pop-jobs-active = Tâches : { $count }

egui-pop-jobs-active-percent = Tâches : { $count } ({ $percent } %)

egui-pop-jobs-clear = Effacer les terminées

egui-pop-jobs-failed = Échecs : { $count }

egui-pop-jobs-history = Historique

egui-pop-jobs-none = Aucune tâche.

egui-pop-jobs-open-panel = Ouvrir le panneau

egui-pop-jobs-title = Tâches

egui-pop-loading = Chargement

egui-pop-menu-does-not-apply = Ne s’applique pas ici ({ $when })

egui-pop-menu-empty = (vide)

egui-pop-menu-last-run-failed = Le dernier lancement a échoué : { $error }

egui-pop-menu-main = Menu principal

egui-pop-menu-more = Autres menus

egui-pop-menu-no-command = Pas de commande `{ $id }`

egui-pop-menu-not-available = Indisponible ici

egui-pop-menu-running = En cours…

egui-pop-menu-too-deep = `{ $menu }` est imbriqué trop profondément

egui-pop-nothing-matches = Aucun résultat.

egui-pop-ok = OK

egui-pop-palette-argument-help = Argument { $at } sur { $count }. Entrée : suivant ; vide : { $example } ; Échap : retour.

egui-pop-palette-argument-title = { $command } : { $argument }

egui-pop-palette-close = Fermer { $palette }

egui-pop-palette-close-group = Fermer le groupe

egui-pop-palette-group-menu = Menu de { $palette }

egui-pop-palette-hint = Tapez une commande

egui-pop-palette-no-match = Aucune commande ne correspond.

egui-pop-palette-not-bool = `{ $input }` n’est ni true ni false

egui-pop-palette-not-json = pas du JSON : { $error }

egui-pop-palette-not-number = `{ $input }` n’est pas un nombre

egui-pop-palette-pin = Épingler { $palette }

egui-pop-palette-recent = récent

egui-pop-palette-recent-keys = récent  { $keys }

egui-pop-palette-roll-down = Dérouler { $palette }

egui-pop-palette-roll-up = Enrouler { $palette }

egui-pop-palette-title = Palette de commandes

egui-pop-palette-unpin = Désépingler { $palette }

egui-pop-panel-focus-description = Affiche le panneau { $panel } et lui donne le clavier.

egui-pop-panel-focus-title = Aller à { $panel }

egui-pop-panel-toggle-description = Rouvre le panneau { $panel } à sa place, ou le ferme.

egui-pop-panel-toggle-title = Afficher ou masquer { $panel }

egui-pop-picker-choose-folder = Choisir ce dossier

egui-pop-picker-close = Fermer le sélecteur

egui-pop-picker-up = Dossier parent

egui-pop-reset = Réinitialiser

egui-pop-retry = Réessayer

egui-pop-section-resize = Redimensionner { $section }

egui-pop-settings-at-default = { $setting } est à sa valeur par défaut

egui-pop-settings-fonts = Polices

egui-pop-settings-nothing-to-redo = Rien à rétablir dans les réglages

egui-pop-settings-nothing-to-undo = Rien à annuler dans les réglages

egui-pop-settings-open-page = Ouvrir

egui-pop-settings-open-page-name = Ouvrir la page { $page }

egui-pop-settings-page-at-default = { $page } est à ses valeurs par défaut

egui-pop-settings-redo = Rétablir

egui-pop-settings-redo-name = Rétablir dans les réglages

egui-pop-settings-redo-step = Rétablir { $change } (les réglages seulement)

egui-pop-settings-reset-page = Réinitialiser la page

egui-pop-settings-reset-page-name = Réinitialiser { $page }

egui-pop-settings-reset-setting = Réinitialiser { $setting }

egui-pop-settings-search-hint = Chercher dans les réglages et les raccourcis

egui-pop-settings-shortcuts = Raccourcis clavier

egui-pop-settings-title = Réglages

egui-pop-settings-undo = Annuler

egui-pop-settings-undo-name = Annuler dans les réglages

egui-pop-settings-undo-step = Annuler { $change } (les réglages seulement)

egui-pop-shortcuts-also-bound = { $keys } est aussi lié à { $command }

egui-pop-shortcuts-bound-to = Lié à { $keys }

egui-pop-shortcuts-change = Modifier

egui-pop-shortcuts-change-name = Modifier les touches de { $command }

egui-pop-shortcuts-conflict-in-scope = { $keys } dans { $scope } : { $loser } cède à { $winner }

egui-pop-shortcuts-conflicts = Conflits

egui-pop-shortcuts-default-name = Touches par défaut de { $command }

egui-pop-shortcuts-from-user = utilisateur

egui-pop-shortcuts-from-workspace = espace de travail

egui-pop-shortcuts-key-from = { $keys } ({ $source })

egui-pop-shortcuts-goes-to = { $keys } va à { $command }

egui-pop-shortcuts-system-failed = { $keys } ne fonctionne pas hors de la fenêtre : { $reason }

egui-pop-shortcuts-system-no-plugin = { $keys } ne fonctionne que dans la fenêtre : aucun plugin n'enregistre de raccourcis globaux

egui-pop-shortcuts-system-on = { $keys } fonctionne aussi hors de la fenêtre

egui-pop-shortcuts-system-pending = { $keys } hors de la fenêtre : en attente du bureau

egui-pop-shortcuts-system-unavailable = { $keys } ne fonctionne que dans la fenêtre : { $reason }

egui-pop-shortcuts-target = Les touches changées ici valent pour

egui-pop-shortcuts-target-user = tous les espaces de travail (vos touches)

egui-pop-shortcuts-target-workspace = cet espace de travail

egui-pop-shortcuts-no-key = Aucune touche

egui-pop-shortcuts-no-key-name = Retirer toutes les touches de { $command }

egui-pop-shortcuts-press-new-keys = Appuyez sur les nouvelles touches de { $command }. Échap : annuler ; Retour arrière : aucune touche.

egui-pop-shortcuts-press-to-search = Appuyez sur les touches à chercher. Échap : annuler.

egui-pop-shortcuts-search-by-key = Chercher par touche

egui-pop-shortcuts-show-all = Afficher toutes les commandes

egui-pop-slider-maximum = { $label }, maximum

egui-pop-slider-minimum = { $label }, minimum

egui-pop-status-announce = { $item } : { $state }

egui-pop-status-errors = Erreurs : { $count }

egui-pop-status-hide = Masquer { $item }

egui-pop-status-more = Autres éléments d’état

egui-pop-status-move-end = Déplacer vers la fin

egui-pop-status-move-start = Déplacer vers le début

egui-pop-status-show = Afficher { $item }

egui-pop-status-warnings = Avertissements : { $count }

egui-pop-theme-editor-accent = Accent et focus

egui-pop-theme-editor-add = Ajouter

egui-pop-theme-editor-add-family-to = Une famille à ajouter à { $role }

egui-pop-theme-editor-add-font-file = Ajouter un fichier de police

egui-pop-theme-editor-adjust-of = Ajuster { $token }

egui-pop-theme-editor-alias-of = Alias de { $token }

egui-pop-theme-editor-apply = Appliquer

egui-pop-theme-editor-apply-fonts = Appliquer les polices

egui-pop-theme-editor-apply-fonts-hint = Reconstruit l’atlas des polices une fois, et applique le thème en cours d’édition.

egui-pop-theme-editor-borders = Bordures

egui-pop-theme-editor-cancel-fonts = Annuler les modifications des polices

egui-pop-theme-editor-change-of = La modification de { $token }

egui-pop-theme-editor-changed-count = Jetons modifiés : { $count }

egui-pop-theme-editor-components = Composants

egui-pop-theme-editor-contrast = Contraste

egui-pop-theme-editor-dark = Sombre

egui-pop-theme-editor-discard = Abandonner

egui-pop-theme-editor-edited = modifié

egui-pop-theme-editor-export = Exporter

egui-pop-theme-editor-fail = échec

egui-pop-theme-editor-families = Familles

egui-pop-theme-editor-families-hint = Par ordre de priorité : la première trouvée dessine, les autres la complètent.

egui-pop-theme-editor-family-missing = Absente ici : la famille suivante est utilisée

egui-pop-theme-editor-family-mono = Valeurs et code (font.family.mono)

egui-pop-theme-editor-family-ui = Texte de l’interface (font.family.ui)

egui-pop-theme-editor-file = Exporter et importer

egui-pop-theme-editor-file-text = Fichier de thème (.tokens.json)

egui-pop-theme-editor-filter = Filtrer les jetons

egui-pop-theme-editor-font-added = { $family } { $weight } ajoutée ; appliquez pour l’utiliser.

egui-pop-theme-editor-font-files = Fichiers de police

egui-pop-theme-editor-font-sample = Échantillon

egui-pop-theme-editor-font-sample-hint = Dessiné avec les polices modifiées, avant qu’elles soient appliquées ; laissez le pointeur sur une famille pour la prévisualiser en premier.

egui-pop-theme-editor-font-sample-text = Portez ce vieux whisky au juge blond qui fume. 0123456789

egui-pop-theme-editor-for-mono = Pour la chasse fixe

egui-pop-theme-editor-for-ui = Pour l’interface

egui-pop-theme-editor-generate = Générer { $scale }

egui-pop-theme-editor-generate-title = Générer cette gamme à partir d’une couleur

egui-pop-theme-editor-generate-variant = Écrite dans la variante éditée seulement ; l’autre garde sa propre gamme (basculez vers elle, ou générez l’autre variante).

egui-pop-theme-editor-generated = { $count } jetons de { $scale } générés ; une seule annulation les reprend.

egui-pop-theme-editor-from = Comme dans { $theme }

egui-pop-theme-editor-hex-of = Hexadécimal de { $token }

egui-pop-theme-editor-import = Importer

egui-pop-theme-editor-imported = Importé sur { $theme }.

egui-pop-theme-editor-installed = Familles installées

egui-pop-theme-editor-installed-hint = Laissez le pointeur sur une famille pour la prévisualiser ; choisissez-en une pour la mettre en premier. Un thème la nomme par son nom de famille : là où elle manque, la famille suivante de sa liste est utilisée.

egui-pop-theme-editor-lc-hint = Contraste de luminosité APCA, pour information : 60 pour du texte ordinaire, 75 pour du texte courant ; le thème est vérifié selon les WCAG.

egui-pop-theme-editor-light = Clair

egui-pop-theme-editor-missing = Le plugin de l’éditeur de thème n’est pas branché.

egui-pop-theme-editor-move-down = Descendre { $family }

egui-pop-theme-editor-move-up = Monter { $family }

egui-pop-theme-editor-name = Nom du thème

egui-pop-theme-editor-no-family-matches = Aucune famille ne correspond.

egui-pop-theme-editor-no-font-files = Aucun fichier de police ajouté.

egui-pop-theme-editor-no-pairs = Aucune paire de contraste n’utilise ce jeton.

egui-pop-theme-editor-not-a-value = Ni une valeur de type { $type }, ni l’alias d’un jeton

egui-pop-theme-editor-not-applicable = s.o.

egui-pop-theme-editor-not-applied = Pas encore appliqué : seul ce volet montre le brouillon.

egui-pop-theme-editor-open-file = Ouvrir un fichier

egui-pop-theme-editor-other = Générer l’autre variante

egui-pop-theme-editor-other-apply = Appliquer à { $theme }

egui-pop-theme-editor-other-cancel = Annuler

egui-pop-theme-editor-other-hint = Propose { $theme } à partir de cette variante : couleurs en miroir (même teinte et même saturation, luminosité inversée), alias et autres valeurs tels quels. Rien ne change avant d’appliquer.

egui-pop-theme-editor-other-none = Rien à changer : { $theme } correspond déjà à cette variante.

egui-pop-theme-editor-other-previewed = Le volet d’aperçu montre la proposition.

egui-pop-theme-editor-other-title = Proposition pour { $theme }

egui-pop-theme-editor-other-written = { $count } jetons de { $theme } écrits ; une annulation dans cette variante les reprend.

egui-pop-theme-editor-own = Modifié dans ce thème

egui-pop-theme-editor-pair = Paire

egui-pop-theme-editor-pair-on = { $foreground } sur { $background }

egui-pop-theme-editor-palette = Palette

egui-pop-theme-editor-pass = réussi

egui-pop-theme-editor-pick = Choisissez un jeton dans la liste.

egui-pop-theme-editor-preview = Aperçu

egui-pop-theme-editor-preview-only = Aperçu seulement

egui-pop-theme-editor-preview-only-hint = Les changements restent dans le volet d’aperçu jusqu’à ce qu’ils soient appliqués ; le reste de la fenêtre garde le thème installé.

egui-pop-theme-editor-problems = Problèmes dans ce thème

egui-pop-theme-editor-ratio = Rapport

egui-pop-theme-editor-redo = Rétablir

egui-pop-theme-editor-remove-family = Retirer { $family }

egui-pop-theme-editor-remove-font-file = Retirer le fichier de { $family }

egui-pop-theme-editor-reset = Revenir à { $theme }

egui-pop-theme-editor-sample-button = Un bouton

egui-pop-theme-editor-sample-checkbox = Une case à cocher

egui-pop-theme-editor-sample-danger = Supprimer

egui-pop-theme-editor-sample-disabled = Désactivé

egui-pop-theme-editor-sample-field = Un champ

egui-pop-theme-editor-sample-heading = Un titre

egui-pop-theme-editor-sample-muted = Du texte discret, pour les indications et les détails.

egui-pop-theme-editor-sample-option-a = Une option

egui-pop-theme-editor-sample-option-b = Une autre

egui-pop-theme-editor-sample-primary = Principal

egui-pop-theme-editor-sample-selected = Une ligne sélectionnée

egui-pop-theme-editor-sample-slider = Un curseur

egui-pop-theme-editor-sample-text = Du texte courant, comme l’essentiel d’une interface.

egui-pop-theme-editor-save-file = Enregistrer dans un fichier

egui-pop-theme-editor-saved = Enregistré dans { $path }.

egui-pop-theme-editor-search-families = Rechercher les familles

egui-pop-theme-editor-seed-clamped = Trop sombre, trop claire ou trop vive pour une gamme : { $color } est utilisée.

egui-pop-theme-editor-seed-hex-of = Hexadécimal de la couleur source de { $scale }

egui-pop-theme-editor-seed-of = Couleur source de { $scale }

egui-pop-theme-editor-shape = Forme, espace, texte et mouvement

egui-pop-theme-editor-skipped = Laissés de côté : { $tokens }.

egui-pop-theme-editor-start = Partir de

egui-pop-theme-editor-states = États

egui-pop-theme-editor-step = Modifier { $token }

egui-pop-theme-editor-step-many = Modifier { $token } et { $more } autres

egui-pop-theme-editor-surfaces = Surfaces

egui-pop-theme-editor-tab-fonts = Polices

egui-pop-theme-editor-tab-tokens = Jetons

egui-pop-theme-editor-text = Texte

egui-pop-theme-editor-theme-needs = Ce thème exige

egui-pop-theme-editor-type = Type : { $type }

egui-pop-theme-editor-undo = Annuler

egui-pop-theme-editor-value = Valeur

egui-pop-theme-editor-value-of = Valeur de { $token }

egui-pop-theme-editor-variant = Variante éditée

egui-pop-theme-editor-variant-hint = Les variantes claire et sombre sont éditées en paire, chacune avec ses modifications et son historique : basculer affiche et édite l’autre.

egui-pop-theme-editor-warnings = Lu avec des avertissements : { $warnings }.

egui-pop-tips-at = { $at } sur { $count }

egui-pop-tips-dont-show = Ne plus afficher

egui-pop-tips-got-it = Compris

egui-pop-tips-got-it-name = Compris : ne plus afficher { $tip }

egui-pop-tips-hint = Astuce

egui-pop-tips-next = Astuce suivante

egui-pop-tips-of-the-day = Astuce du jour

egui-pop-tips-previous = Astuce précédente

egui-pop-toast-close = Fermer : { $text }

egui-pop-toast-error = Erreur

egui-pop-toast-info = Information

egui-pop-toast-success = Terminé

egui-pop-toast-warning = Avertissement

egui-pop-value-held-by = En cours de modification par { $origin }

egui-pop-value-missing = aucune valeur à `{ $path }`

egui-pop-value-read-only = Lecture seule

egui-pop-viewport-curve-value = { $points } points

egui-pop-viewport-curve-value-selected = point { $point } sur { $points } : entrée { $input } %, sortie { $output } %

egui-pop-viewport-curve-editor = Éditeur de courbe

egui-pop-viewport-curve-secondary-click = Supprime le point sous le pointeur

egui-pop-viewport-graph-add-node = Ajouter un nœud

egui-pop-viewport-graph-collapse = Replier { $node }

egui-pop-viewport-graph-connected = { $from } relié à { $to }

egui-pop-viewport-graph-delete = Supprimer

egui-pop-viewport-graph-disconnect = Déconnecter

egui-pop-viewport-graph-duplicate = Dupliquer

egui-pop-viewport-graph-exists = le lien existe déjà

egui-pop-viewport-graph-expand = Déplier { $node }

egui-pop-viewport-graph-fold = Empiler { $group }

egui-pop-viewport-graph-label = Graphe de nœuds

egui-pop-viewport-graph-level-detail = détail

egui-pop-viewport-graph-level-overview = vue d’ensemble

egui-pop-viewport-graph-level-ports = ports

egui-pop-viewport-graph-minimap-label = Vue d’ensemble du graphe

egui-pop-viewport-graph-minimap-value = zoom { $zoom } %

egui-pop-viewport-graph-node-value = { $inputs } entrées, { $outputs } sorties, { $wires } liens

egui-pop-viewport-graph-port-input = { $node }, entrée { $port }

egui-pop-viewport-graph-port-output = { $node }, sortie { $port }

egui-pop-viewport-graph-port-value = { $type }, { $wires } liens

egui-pop-viewport-graph-refused = { $to } refuse { $from } : { $reason }

egui-pop-viewport-graph-role-input = port d’entrée

egui-pop-viewport-graph-role-node = nœud

egui-pop-viewport-graph-role-output = port de sortie

egui-pop-viewport-graph-role-stack = pile

egui-pop-viewport-graph-same-node = un nœud ne peut pas s’alimenter lui-même

egui-pop-viewport-graph-same-side = deux entrées ou deux sorties

egui-pop-viewport-graph-types-differ = les types diffèrent ({ $from }, { $to })

egui-pop-viewport-graph-unfold = Déplier { $group } ({ $count } nœuds)

egui-pop-viewport-graph-value = { $nodes } nœuds, { $wires } liens, { $level }

egui-pop-viewport-graph-wire-started = Lien depuis { $port } : choisissez un port

egui-pop-viewport-histogram-clipped-highlights = hautes lumières écrêtées : { $percent } %

egui-pop-viewport-histogram-clipped-shadows = ombres écrêtées : { $percent } %

egui-pop-viewport-histogram-label = Histogramme

egui-pop-viewport-histogram-value = { $channels } ({ $bins } classes)

egui-pop-viewport-histogram-value-log = { $channels } ({ $bins } classes, logarithmique)

egui-pop-viewport-label = Visionneuse d’image

egui-pop-viewport-loupe-display = affichage

egui-pop-viewport-loupe-none = aucune valeur

egui-pop-viewport-loupe-scene = scène

egui-pop-viewport-navigator-label = Navigateur

egui-pop-viewport-navigator-value = zoom { $zoom } %, centre { $x }, { $y }

egui-pop-viewport-wheel-luminance = Luminance { $wheel }

egui-pop-viewport-wheel-value = teinte { $hue }°, chroma { $chroma } %, luminance { $luminance }

egui-pop-viewport-zoom-actual = 100 %

egui-pop-viewport-zoom-fit = Ajuster

egui-pop-viewport-zoom-in = Zoom avant

egui-pop-viewport-zoom-out = Zoom arrière

egui-pop-views-add-filter = Ajouter un filtre

egui-pop-views-add-tag = Ajouter un mot-clé

egui-pop-views-blue = Bleu

egui-pop-views-clear-filters = Effacer la recherche

egui-pop-views-collapse-stack = Replier la pile de { $count }

egui-pop-views-compare = Comparaison

egui-pop-views-empty = Vide

egui-pop-views-expand-stack = Déplier la pile de { $count }

egui-pop-views-failed = Chargement impossible : { $reason }

egui-pop-views-filmstrip = Bande de vignettes

egui-pop-views-filter-dates = Du { $from } au { $to }

egui-pop-views-filter-flag = Marque : { $flag }

egui-pop-views-filter-from = Depuis le { $from }

egui-pop-views-filter-label = Étiquette : { $label }

egui-pop-views-filter-rating = Note ≥ { $rating }

egui-pop-views-filter-tag = Mot-clé : { $tag }

egui-pop-views-filter-to = Jusqu’au { $to }

egui-pop-views-flag = Marque

egui-pop-views-green = Vert

egui-pop-views-label = Étiquette

egui-pop-views-label-toggle = Étiquette { $label }

egui-pop-views-library = Photothèque

egui-pop-views-list = Liste

egui-pop-views-loading = Chargement…

egui-pop-views-mark-best = Meilleure de la comparaison

egui-pop-views-pick = Retenue

egui-pop-views-purple = Violet

egui-pop-views-rate-stars = Noter { $stars } sur 5

egui-pop-views-rating = Note

egui-pop-views-red = Rouge

egui-pop-views-reject = Rejetée

egui-pop-views-remove = Retirer { $item }

egui-pop-views-rename = Renommer { $name }

egui-pop-views-search = Rechercher

egui-pop-views-sorted-down = { $column }, tri décroissant

egui-pop-views-sorted-up = { $column }, tri croissant

egui-pop-views-table = Tableau

egui-pop-views-tree = Arborescence

egui-pop-views-unflagged = Sans marque

egui-pop-views-yellow = Jaune

egui-pop-window-exit-fullscreen = Quitter le plein écran (F11)

egui-pop-workbench-close-panel = Fermer { $panel }

egui-pop-workbench-detach = Détacher dans une fenêtre

egui-pop-workbench-float = Faire flotter

egui-pop-workbench-flyout-pin = Épingler { $panel }

egui-pop-workbench-fold = Replier { $area }

egui-pop-workbench-move-to = Déplacer vers { $area }

egui-pop-workbench-stays-out = { $panel } ne va pas dans { $area }

egui-pop-workbench-unfold = Déplier { $area }

egui-pop-workbench-window-title = { $panel } — { $application }

egui-pop-zen-hint = Mode zen : { $keys } pour quitter

egui-pop-zen-hint-no-key = Mode zen : le bord du haut montre la sortie

menu-app-menu-edit =
    .label = Édition

menu-app-menu-file =
    .label = Fichier

menu-app-menu-help =
    .label = Aide

menu-app-menu-view =
    .label = Affichage

panel-egui-pop-devtools =
    .title = Outils de développement

panel-egui-pop-jobs =
    .title = Tâches

plugin-egui-pop-arrangements =
    .title = Dispositions

plugin-egui-pop-clipboard =
    .title = Presse-papiers

plugin-egui-pop-contexts =
    .title = Pile de contextes

plugin-egui-pop-crash =
    .title = Rapports de plantage

plugin-egui-pop-devtools =
    .title = Développement

plugin-egui-pop-dialogs =
    .title = Dialogues

plugin-egui-pop-editors =
    .title = Éditeurs

plugin-egui-pop-files =
    .title = Fichiers

plugin-egui-pop-focus-mode =
    .title = Mode de focus

plugin-egui-pop-frame =
    .title = Cadre de la fenêtre

plugin-egui-pop-i18n =
    .title = Langue

plugin-egui-pop-instance =
    .title = Instance unique

plugin-egui-pop-job-consoles =
    .title = Consoles des tâches

plugin-egui-pop-jobs =
    .title = Tâches

plugin-egui-pop-library =
    .title = Photothèque

plugin-egui-pop-logs =
    .title = Journaux

plugin-egui-pop-notify =
    .title = Notifications

plugin-egui-pop-palette =
    .title = Palette de commandes

plugin-egui-pop-settings =
    .title = Réglages

plugin-egui-pop-strips =
    .title = Bandes

plugin-egui-pop-system-keys =
    .title = Raccourcis globaux

plugin-egui-pop-terminal =
    .title = Terminal

plugin-egui-pop-theme-editor =
    .title = Éditeur de thème

plugin-egui-pop-tips =
    .title = Astuces

plugin-egui-pop-toasts =
    .title = Notifications

plugin-egui-pop-tray =
    .title = Icône de la zone de notification

plugin-egui-pop-views =
    .title = Vues de données

plugin-egui-pop-window =
    .title = Fenêtre

setting-appearance-accent-color =
    .description = Une couleur (#rrggbb) d’où l’accent est généré, en clair et en sombre : douze nuances dont le texte et le contraste se lisent dans le thème. Vide garde l’accent du thème.
    .label = Couleur d’accent

setting-appearance-focus =
    .label = Focus du clavier

setting-appearance-focus-choice-click = au clic

setting-appearance-focus-choice-hover = au survol

setting-appearance-focus-choice-system = comme le système

setting-appearance-font-mono =
    .description = Une famille dessinée en premier pour les nombres et le code, celles du thème derrière. Vide garde celles du thème.
    .label = Police à chasse fixe

setting-appearance-font-ui =
    .description = Une famille dessinée en premier pour le texte de l’interface, celles du thème derrière. Vide garde celles du thème.
    .label = Police de l’interface

setting-appearance-interface-zoom =
    .description = Agrandit ou réduit toute l’interface, dans chaque fenêtre (Ctrl+Alt+= et Ctrl+Alt+-).
    .label = Zoom de l’interface

setting-appearance-interface-zoom-choice-100- = 100 %

setting-appearance-interface-zoom-choice-110- = 110 %

setting-appearance-interface-zoom-choice-125- = 125 %

setting-appearance-interface-zoom-choice-150- = 150 %

setting-appearance-interface-zoom-choice-175- = 175 %

setting-appearance-interface-zoom-choice-200- = 200 %

setting-appearance-interface-zoom-choice-67- = 67 %

setting-appearance-interface-zoom-choice-75- = 75 %

setting-appearance-interface-zoom-choice-80- = 80 %

setting-appearance-interface-zoom-choice-90- = 90 %

setting-appearance-language =
    .description = La langue de l’interface ; Système suit celle du système.
    .label = Langue

setting-appearance-language-choice-system = Système

setting-appearance-text-size =
    .description = Agrandit ou réduit le texte, sans les espacements (le zoom de l’interface agrandit tout). Rien n’a été lu du système : la valeur par défaut est 100 %.
    .label = Taille du texte

setting-appearance-text-size-choice-100- = 100 %

setting-appearance-text-size-choice-115- = 115 %

setting-appearance-text-size-choice-130- = 130 %

setting-appearance-text-size-choice-150- = 150 %

setting-appearance-text-size-choice-175- = 175 %

setting-appearance-text-size-choice-200- = 200 %

setting-appearance-text-size-choice-85- = 85 %

setting-appearance-theme-preference =
    .description = Quelle variante d’un thème qui en a une claire et une sombre est affichée : celle du système, ou toujours la claire ou la sombre. Un thème sans pendant garde la sienne.
    .label = Clair ou sombre

setting-appearance-theme-preference-choice-dark = sombre

setting-appearance-theme-preference-choice-light = clair

setting-appearance-theme-preference-choice-system = comme le système

setting-developer-devtools =
    .description = Permet d’ouvrir le panneau des outils de développement (F12) : résolutions des touches, journal, étapes, focus, échelle et chaque collecteur de débogage.
    .label = Outils de développement

setting-developer-log-level =
    .description = Ce que l’application écrit dans son fichier journal : des seules erreurs jusqu’au moindre détail.
    .label = Niveau de journalisation

setting-developer-log-level-choice-debug = débogage

setting-developer-log-level-choice-error = erreurs

setting-developer-log-level-choice-info = informations

setting-developer-log-level-choice-trace = traces

setting-developer-log-level-choice-warn = avertissements

setting-developer-show-keys =
    .description = Affiche la clé de traduction de chaque texte à la place du texte, pour trouver d’où il vient.
    .label = Afficher les clés de traduction

setting-dialogs-closable =
    .description = Un ✕ au bout de la ligne de titre de chaque dialogue, qui fait ce que fait Échap.
    .label = Bouton de fermeture sur chaque dialogue

setting-dialogs-movable =
    .description = Chaque dialogue a un bandeau de titre par lequel le faire glisser ; Alt+F7 le déplace avec les flèches.
    .label = Dialogues déplaçables

setting-files-native =
    .description = Choisir les fichiers avec le sélecteur du système quand il y en a un ; désactivé, avec celui d’egui-pop.
    .label = Utiliser le sélecteur de fichiers du système

setting-menus-icons =
    .description = Si les menus montrent les icônes des commandes dans une colonne à leur gauche (activé), aucune (désactivé), ou suivent le bureau (plateforme : désactivé sous GNOME, comme le dit KDE Plasma, activé ailleurs).
    .label = Icônes des menus

setting-menus-icons-choice-off = désactivé

setting-menus-icons-choice-on = activé

setting-menus-icons-choice-platform = plateforme

setting-notifications-desktop =
    .description = Où s’affichent les notifications : sur le bureau, sur le bureau seulement quand la fenêtre n’a pas le focus, ou dans la fenêtre.
    .label = Notifications du bureau

setting-notifications-desktop-choice-always = toujours

setting-notifications-desktop-choice-never = jamais

setting-notifications-desktop-choice-unfocused = hors focus

setting-palettes-fade-on-stroke =
    .description = Les palettes s’estompent pendant un tracé sur le canevas.
    .label = Estomper les palettes pendant le dessin

setting-palettes-idle-opacity =
    .description = L’opacité d’une palette quand le pointeur est ailleurs ; 1 la garde opaque.
    .label = Opacité des palettes loin du pointeur

setting-tips-dismissed =
    .description = Les astuces à ne plus afficher, par id ; vide pour toutes les afficher de nouveau.
    .label = Astuces écartées

setting-tips-show =
    .description = L’astuce du jour et les conseils à côté de ce qu’ils concernent.
    .label = Afficher les astuces

setting-window-bar-in-fullscreen =
    .description = Garde la barre de titre de l’application (ses boutons et ses éléments) en plein écran ; désactivé, le plein écran la masque.
    .label = Barre de titre en plein écran

setting-window-frame =
    .description = Qui dessine la barre de titre et les bords de la fenêtre : l’application (propre) ou le bureau (système).
    .label = Cadre de la fenêtre

setting-window-frame-choice-own = propre

setting-window-frame-choice-system = système

setting-workbench-panel-alignment =
    .description = Comment le panneau du bas s’aligne sur les barres latérales : toute la largeur (justifié), entre elles (centré), sous celle du début (début) ou sous celle de la fin (fin).
    .label = Alignement du panneau

setting-workbench-panel-alignment-choice-center = centré

setting-workbench-panel-alignment-choice-end = fin

setting-workbench-panel-alignment-choice-justify = justifié

setting-workbench-panel-alignment-choice-start = début

category-pointer = Pointeur

command-gesture-image-zoom =
    .description = Zoome l’image autour du pointeur, en marquant un arrêt à l’ajustement et à 100 %.
    .title = Image : zoomer au pointeur

command-gesture-image-zoom-free =
    .description = Zoome l’image autour du pointeur, sans arrêt.
    .title = Image : zoomer sans arrêt

command-gesture-image-pan =
    .description = Déplace l’image à la molette ou en la faisant glisser.
    .title = Image : déplacer

command-gesture-image-pan-x =
    .description = Déplace l’image latéralement à la molette.
    .title = Image : déplacer horizontalement

command-gesture-image-pan-y =
    .description = Déplace l’image vers le haut ou le bas à la molette.
    .title = Image : déplacer verticalement

command-gesture-image-zoom-cycle =
    .description = Passe à 100 %, puis 200 %, puis à l’ajustement, autour du pointeur.
    .title = Image : cycle de zoom

command-gesture-image-fit =
    .description = Ajuste toute l’image à la vue.
    .title = Image : ajuster

command-gesture-image-zoom-toggle =
    .description = Passe à 100 % autour du pointeur quand l’image est ajustée, sinon l’ajuste.
    .title = Image : ajuster ou 100 %

command-gesture-graph-zoom =
    .description = Zoome le graphe autour du pointeur.
    .title = Graphe de nœuds : zoomer au pointeur

command-gesture-graph-pan =
    .description = Déplace le graphe à la molette ou en le faisant glisser.
    .title = Graphe de nœuds : déplacer

command-gesture-graph-pan-x =
    .description = Déplace le graphe latéralement à la molette.
    .title = Graphe de nœuds : déplacer horizontalement

command-gesture-graph-pan-y =
    .description = Déplace le graphe vers le haut ou le bas à la molette.
    .title = Graphe de nœuds : déplacer verticalement

command-gesture-graph-fit =
    .description = Ajuste tout le graphe à la vue.
    .title = Graphe de nœuds : ajuster

command-gesture-value-step =
    .description = La molette avec ces touches fait avancer le curseur sous le pointeur, qu’il ait le focus ou non.
    .title = Valeurs : pas à la molette

command-gesture-value-fine =
    .description = Ces touches rendent le glissement, les flèches et la molette d’un curseur dix fois plus fins.
    .title = Valeurs : plus fin

command-gesture-value-coarse =
    .description = Ces touches rendent le glissement, les flèches et la molette d’un curseur dix fois plus grossiers.
    .title = Valeurs : plus grossier

command-gesture-value-panel =
    .description = La molette avec ces touches fait défiler le panneau, même au-dessus d’un curseur.
    .title = Valeurs : faire défiler le panneau

command-pointer-overrides =
    .description = Garde les gestes du pointeur que l’utilisateur a changés sur le profil en cours, ou les abandonne (`value` : keep ou drop).
    .title = Garder ou abandonner les gestes du pointeur changés

egui-pop-pointer-overrides-title = Profil de pointeur : { $profile }

egui-pop-pointer-overrides-detail = Vous avez changé { $count } gestes du pointeur. Les garder par-dessus ce profil, ou les abandonner ?

egui-pop-pointer-keep = Garder mes changements

egui-pop-pointer-drop = Abandonner mes changements

egui-pop-shortcuts-pointer-profile = Profil de pointeur : { $profile }

egui-pop-shortcuts-pointer-profile-changed = Profil de pointeur : { $profile } ({ $count } changés)

egui-pop-shortcuts-pointer-only = Gestes du pointeur seulement

egui-pop-shortcuts-pointer-add = Ajouter un geste

egui-pop-shortcuts-pointer-add-name = Ajouter un geste à { $command }

egui-pop-shortcuts-pointer-remove-name = Retirer { $keys } de { $command }

egui-pop-shortcuts-pointer-pad = Faites ici le geste pour { $command } : la molette, un pincement, un clic, un double clic ou un glissement, en tenant ses touches. Échap : annuler.

egui-pop-shortcuts-pointer-pad-name = Zone de geste

egui-pop-shortcuts-pointer-use = Utiliser { $keys }

egui-pop-shortcuts-pointer-alt = Alt avec un glissement ou la molette peut aller au gestionnaire de fenêtres sur certains bureaux Linux.

egui-pop-shortcuts-widget-uses = Gestes propres aux widgets

egui-pop-shortcuts-widget-uses-note = Ces widgets gardent ces gestes pour eux : les menus contextuels et le profil de pointeur les y laissent.

egui-pop-viewport-hint-zoom = zoomer

egui-pop-viewport-hint-zoom-free = zoomer sans arrêt

egui-pop-viewport-hint-pan = déplacer

egui-pop-viewport-hint-pan-x = déplacer latéralement

egui-pop-viewport-hint-pan-y = déplacer vers le haut ou le bas

egui-pop-viewport-hint-zoom-cycle = 100 %, 200 %, ajuster

egui-pop-viewport-hint-fit = ajuster

egui-pop-viewport-hint-zoom-toggle = ajuster ou 100 %

egui-pop-viewport-hint-leave = quitter

setting-input-pointer-profile =
    .description = Ce que font la molette, un pincement et les boutons de la souris sur l’image, le graphe de nœuds et les curseurs, comme dans darktable, Photoshop, Lightroom Classic, les outils de document (GIMP, Inkscape, le web), Blender et les éditeurs de nœuds, ou sur un pavé tactile. L’éditeur de raccourcis montre et change chaque geste.
    .label = Profil de pointeur

setting-input-pointer-profile-choice-document = document

setting-input-pointer-profile-choice-darktable = darktable

setting-input-pointer-profile-choice-photoshop = photoshop

setting-input-pointer-profile-choice-lightroom = lightroom

setting-input-pointer-profile-choice-blender = blender

setting-input-pointer-profile-choice-trackpad = pavé tactile

setting-input-slider-wheel =
    .description = Si la molette au-dessus d’un curseur fait défiler le panneau (protégée : le curseur n’avance qu’avec le focus ou les touches de pas) ou change la valeur (valeur) ; profil : comme le dit le profil de pointeur.
    .label = Molette sur les curseurs

setting-input-slider-wheel-choice-profile = profil

setting-input-slider-wheel-choice-guarded = protégée

setting-input-slider-wheel-choice-value = valeur

setting-input-image-double-click =
    .description = Ce que fait un double clic sur l’image : l’ajuster, quitter la vue (retour au navigateur, comme dans darktable) ou rien ; profil : comme le dit le profil de pointeur.
    .label = Double clic sur l’image

setting-input-image-double-click-choice-profile = profil

setting-input-image-double-click-choice-fit = ajuster

setting-input-image-double-click-choice-leave = quitter

setting-input-image-double-click-choice-nothing = rien

setting-input-wheel-device =
    .description = Ce qui envoie les événements de molette : auto distingue une molette de souris (lignes) d’un pavé tactile (points) ; certaines souris envoient aussi des points, et choisir souris ou pavé tactile tranche.
    .label = Appareil de la molette

setting-input-wheel-device-choice-auto = auto

setting-input-wheel-device-choice-mouse = souris

setting-input-wheel-device-choice-trackpad = pavé tactile

setting-workbench-side-bar-position =
    .description = Où se trouve la barre latérale principale : à gauche, la secondaire à droite (gauche), ou l’inverse (droite).
    .label = Position de la barre latérale principale

setting-workbench-side-bar-position-choice-left = gauche

setting-workbench-side-bar-position-choice-right = droite

command-workbench-toggle-primarySideBar =
    .description = Affiche ou masque la barre latérale principale : masquée, elle se replie sur sa bande et garde ses panneaux.
    .title = Afficher ou masquer la barre latérale principale

command-workbench-toggle-panel =
    .description = Affiche ou masque le panneau du bas : masqué, il garde ses panneaux.
    .title = Afficher ou masquer le panneau

command-workbench-toggle-secondarySideBar =
    .description = Affiche ou masque la barre latérale secondaire : masquée, elle se replie sur sa bande et garde ses panneaux.
    .title = Afficher ou masquer la barre latérale secondaire
