---
name: ChatON
description: Messagerie communautaire auto-hébergée. Un chat dans la nuit ; le vert, ce sont ses yeux.
colors:
  surface-frame: "#09081a"
  surface-sunken: "#0c0b20"
  surface-panel: "#100f27"
  surface-base: "#14132e"
  surface-raised: "#211f45"
  surface-hover: "#2b2956"
  accent: "#3ee07f"
  accent-hover: "#6cf0a0"
  accent-ink: "#04230f"
  accent-text: "#6cf0a0"
  content-primary: "#f2f0ff"
  content-secondary: "#a19dc8"
  content-tertiary: "#6c6896"
  danger: "#f87171"
  border-subtle: "#24224a"
  border-strong: "#3a3770"
typography:
  display:
    fontFamily: "Bricolage Grotesque Variable, Inter Variable, sans-serif"
    fontSize: "6rem"
    fontWeight: 700
    lineHeight: 1.25
    letterSpacing: "-0.02em"
  headline:
    fontFamily: "Bricolage Grotesque Variable, Inter Variable, sans-serif"
    fontSize: "1.375rem"
    fontWeight: 700
    lineHeight: 1.25
  title:
    fontFamily: "Bricolage Grotesque Variable, Inter Variable, sans-serif"
    fontSize: "1rem"
    fontWeight: 700
    lineHeight: 1.25
  body:
    fontFamily: "Inter Variable, system-ui, sans-serif"
    fontSize: "0.9375rem"
    fontWeight: 400
    lineHeight: 1.5
  label:
    fontFamily: "Inter Variable, system-ui, sans-serif"
    fontSize: "0.875rem"
    fontWeight: 500
    lineHeight: 1.25
  caption:
    fontFamily: "Inter Variable, system-ui, sans-serif"
    fontSize: "0.75rem"
    fontWeight: 500
    lineHeight: 1.25
  mono:
    fontFamily: "ui-monospace, SFMono-Regular, Cascadia Mono, Menlo, Consolas, monospace"
    fontSize: "0.875rem"
    fontWeight: 400
rounded:
  sm: "6px"
  md: "10px"
  lg: "16px"
  full: "9999px"
spacing:
  "1": "0.25rem"
  "2": "0.5rem"
  "3": "0.75rem"
  "4": "1rem"
  "5": "1.25rem"
  "6": "1.5rem"
  "8": "2rem"
  "12": "3rem"
components:
  button:
    backgroundColor: "{colors.surface-raised}"
    textColor: "{colors.content-primary}"
    typography: "{typography.label}"
    rounded: "{rounded.md}"
    padding: "0 1rem"
    height: "2.5rem"
  button-hover:
    backgroundColor: "{colors.surface-hover}"
  button-primary:
    backgroundColor: "{colors.accent}"
    textColor: "{colors.accent-ink}"
    typography: "{typography.label}"
    rounded: "{rounded.md}"
    padding: "0 1rem"
    height: "2.5rem"
  button-primary-hover:
    backgroundColor: "{colors.accent-hover}"
  button-ghost:
    backgroundColor: "transparent"
    textColor: "{colors.content-secondary}"
  button-danger:
    backgroundColor: "transparent"
    textColor: "{colors.danger}"
  input:
    backgroundColor: "{colors.surface-sunken}"
    textColor: "{colors.content-primary}"
    typography: "{typography.body}"
    rounded: "{rounded.md}"
    padding: "0 0.75rem"
    height: "2.5rem"
  list-row:
    textColor: "{colors.content-secondary}"
    rounded: "{rounded.md}"
    padding: "0.25rem 0.5rem"
    height: "2.5rem"
  badge:
    backgroundColor: "{colors.surface-raised}"
    textColor: "{colors.content-secondary}"
    typography: "{typography.caption}"
    rounded: "{rounded.sm}"
    padding: "0 0.5rem"
    height: "1.25rem"
  message:
    backgroundColor: "transparent"
    textColor: "{colors.content-primary}"
    typography: "{typography.body}"
    padding: "2px 1rem"
  sheet:
    backgroundColor: "{colors.surface-base}"
    rounded: "{rounded.lg}"
  dialog:
    backgroundColor: "{colors.surface-raised}"
    rounded: "{rounded.lg}"
    padding: "2rem 1.5rem 1.5rem"
---

# Design System : ChatON

Source de vérité du code : `app/src/styles/_tokens.scss` (valeurs), `_base.scss` (éléments), `_components.scss` (classes partagées), plus le composant `app-avatar`. Ce document explique quand utiliser quoi. Si les deux divergent, le code a raison et ce fichier est à mettre à jour.

## Overview

**Étoile du nord : « Nocturne ».** ChatON, c'est un chaton et « chat on » : un chat éveillé dans la nuit. La nuit est bleu-violet, calme, faite pour discuter longtemps ; la seule lumière est verte, là où il faut agir ou regarder.

L'identité tient dans des motifs de chat, chacun avec un sens :
- **Le chat** : le logo (`/chatON-logo.svg`) : bouton des messages privés, intro des salons, écran vide. Les yeux restent des trous dans la tête blanche : pas de lueur.
- **Oreilles** : **mon** avatar est découpé en tête de chat (`app-avatar [ears]`, masque SVG, même encombrement que le cercle) : une photo remplit aussi les oreilles. Panneau du compte, mes messages, listes de membres. Les oreilles, c'est moi ; nulle part ailleurs.
- **Moustaches** : de part et d'autre du jour dans l'historique.
- **Pattes** (`app-paw-trail`, icône Lucide `PawPrint`) : un chat est passé par là. États vides (rien d'ouvert, pas de conversation, pas de communauté), attente (création d'invitation), 404. Les empreintes s'allument une à une, sans bouger si le mouvement est réduit.
- **Lucioles** : accueil, bas du rail.
- **Texte** : ton geek, culture mème : écrans vides (« It's dangerous to go alone! », « Pas un chat ici. »), attentes (« Loading the loading screen… »), 404 (« Must have been the wind. ») et erreur générique (« it works on my machine »). Un clin d'œil par endroit, et la phrase dit toujours quoi faire. Jamais dans les erreurs précises (clé, réseau, identifiant), ni sur les boutons d'action hors de la 404.

ChatON est un outil qu'on garde ouvert des heures : l'interface s'efface derrière les conversations. La navigation (rail, colonne) est posée sur un **cadre** plus sombre ; la conversation ouverte est une **feuille** arrondie, encastrée dans ce cadre. Une seule famille de caractères (Inter), des surfaces qui se distinguent par leur valeur (plus clair = plus proche) plutôt que par des ombres. Le vert `#3ee07f` est l'identité du produit (le dégradé de l'accueil, les lucioles) et la seule couleur d'accent.

Un réseau fédéré expose des identités qui se ressemblent : `erwan@a.fr` et `erwan@b.fr` sont deux personnes. L'interface affiche donc **toujours** l'instance, en atténué, à côté du pseudo.

**Caractéristiques :**
- Sombre par conception (usage prolongé, souvent le soir), pas de thème clair pour l'instant.
- Un seul accent, rare : action principale, sélection, focus.
- Densité moyenne, façon Discord : listes compactes, messages à plat groupés par auteur.
- Références : Discord (rail, messages groupés), Linear (« la structure se sent, ne se voit pas », navigation atténuée), Slack (contenu encastré dans un cadre), Element X (une liste à la fois sur téléphone). Pas une copie : pas de pastille de sélection Discord, pas d'icônes partout.
- Profondeur par la valeur des surfaces ; ombre réservée à ce qui flotte.

## Colors

Nuit bleu-violet en six paliers, un vert franc, un rouge doux pour le danger. Le cadre est uni : une seule couleur de fond autour de la feuille, barre de titre comprise.

### Primary
- **Vert** (#3ee07f) `--accent` : bouton principal (un par écran au plus), anneau de la communauté ouverte, focus clavier. Texte posé dessus : `--accent-ink` (#04230f, 10:1). `--accent-rgb` pour les transparences (`rgb(var(--accent-rgb) / .3)`).
- **Vert clair** (#6cf0a0) `--accent-hover` / `--accent-text` : survol du bouton principal ; texte vert (mon propre nom dans les messages, nom du salon dans l'intro, icône du salon actif, liens).
- **Vert voilé** (`rgb(62 224 127 / .14)`) `--accent-subtle` : ligne sélectionnée (en dégradé qui s'éteint vers la droite), onglet actif, halo des champs au focus.

### Neutral
- **Cadre** (#09081a) `--surface-frame` : rail des communautés et colonne de navigation, autour de la feuille.
- **Puits** (#0c0b20) `--surface-sunken` : intérieur des champs, barre de saisie, panneau du compte, clés affichées.
- **Panneau** (#100f27) `--surface-panel` : membres, dans la feuille, un cran sous la conversation.
- **Nuit** (#14132e) `--surface-base` : les pages publiques. La feuille (conversation ouverte) garde le fond du cadre : une seule couleur de fond, la bordure dessine la feuille.
- **Nuit levée** (#211f45) `--surface-raised` : boutons, cartes de fichier, dialogues, menus, infobulles, sélecteur d'émojis.
- **Nuit survolée** (#2b2956) `--surface-hover` : survol d'un élément levé.
- **Lune** (#f2f0ff) `--content-primary` : texte (16:1).
- **Brume** (#a19dc8) `--content-secondary` : méta, aides, instance d'un handle, placeholders (6,9:1).
- **Ombre violette** (#6c6896) `--content-tertiary` : désactivé et icônes décoratives seulement (3,4:1, **jamais** pour du petit texte).
- **Bordures** : `--border-subtle` (#24224a) entre colonnes ; `--border-strong` (#3a3770) autour des contrôles, moustaches.
- **États neutres** : `--state-hover` et `--state-active` (voiles lavande) pour tout ce qui n'est pas sélectionné.

### Status
- **Corail** (#f87171) `--danger` : erreurs, actions destructrices (6,4:1), avec `--danger-subtle` (fond) et `--danger-border`.

### Named Rules
**La règle du vert.** Le vert ne décore jamais. S'il n'indique ni l'action principale, ni la sélection, ni le focus, ni « c'est moi », il n'a rien à faire là. Seule exception : les lucioles, qui sont la marque.

**La règle du survol neutre.** Le survol est gris (`--state-hover`) ; seul ce qui est sélectionné devient vert. Un survol vert ferait croire à une sélection.

## Typography

**Lecture :** Inter variable avec taille optique (`@fontsource-variable/inter`, `opsz.css`), repli `system-ui`. Variantes `cv11` (a à un étage) et `ss01` (chiffres ouverts). Messages, listes, boutons, champs.
**Noms et titres :** Bricolage Grotesque variable (`@fontsource-variable/bricolage-grotesque`, `opsz.css`), `--font-display`, 700, resserrée (-0,015em). Tous les `h1` et `h2` : nom de la communauté, titre de la conversation, intro, dialogues. Elle donne le caractère ; elle ne descend jamais dans le texte courant.
**Mono :** pile système (`ui-monospace`…) pour les clés, codes et invitations.

**Caractère :** une sans neutre et lisible, qui porte titres, boutons, données et messages sans changer de voix.

### Hierarchy
Échelle fixe en rem (ratio ~1,2), pas de taille fluide : `--text-xs` 12, `--text-sm` 14, `--text-body` 15, `--text-md` 16, `--text-lg` 18, `--text-xl` 22, `--text-2xl` 28, `--text-display` 96 px.
- **Display** (700, 6rem) : le « 404 », rien d'autre.
- **Intro** (display 700, 1,75rem) : « Bienvenue dans #général », en tête d'historique.
- **Headline** (display 700, 1,375rem) : titre de dialogue (`h2`).
- **Title** (display 600 à 700, 1rem) : nom de la communauté, titre de la conversation dans l'en-tête.
- **Body** (400, 0,9375rem, 1,45) : messages (75ch au plus), lignes de liste.
- **Label** (500, 0,875rem) : boutons, labels, champs de la colonne.
- **Auteur** (600, body) : nom au-dessus d'un groupe de messages.
- **Section** (`.section-label`, 600, 0,75rem, majuscules, +0,04em) : « SALONS », « MEMBRES — 3 ».
- **Caption** (500, 0,75rem) : badges, heure, séparateur de jour.

### Named Rules
**La règle du handle.** Un handle s'affiche en deux temps : `@pseudo` en couleur normale, `@instance` en `--content-secondary`. Jamais le pseudo seul. (`splitHandle` dans `Dm.model.ts`, utilisé par `app-user-card` et les messages.)

## Layout

- **Grille de 4 px** : `--space-1` (4) à `--space-12` (48). Petit écart dans un groupe (`--space-1`/`-2`), grand entre les groupes (`--space-4`/`-6`).
- **Coquille de chat** (`chat.layout`) : `rail` (`--rail-width`, 4,5rem) | `sidebar` (`--sidebar-width`, 15rem) | feuille, hauteur `100dvh`, sur `--surface-frame`. La feuille a une marge `--space-2`, un rayon lg et une bordure `--border-subtle` ; elle contient l'en-tête (`--header-height`, 3rem), la conversation et les membres (`--members-width`, 15rem). Le haut de la colonne (nom de la communauté ou recherche) s'aligne sur l'en-tête de la feuille.
- **Ce qui est ouvert** : `ShellService` (`core/services`) tient la conversation ouverte (pour l'en-tête, l'intro, le placeholder), `chatOpen` (téléphone) et `members` (panneau affiché, par défaut à partir de 1024 px, là où il ne recouvre pas la conversation).
- **Listes** : lignes de 32 à 40 px, 2 px entre elles, `--space-2` de marge intérieure ; regroupées sous des `.section-label`.
- **Messages** : à plat, avatar 40 px + texte, puis la pièce jointe. Une image s'affiche seule (Enregistrer dans la barre d'actions au survol) ; un autre fichier, en carte. Heure seule : le séparateur donne le jour. Un message du même auteur à moins de 7 min, le même jour, continue le groupe (ni avatar ni nom, heure au survol dans la gouttière). `--space-4` avant chaque groupe. Mon nom en `--accent-text`. Séparateur de jour entre deux paires de moustaches. Intro en tête de l'historique. Liste ancrée en bas (`column-reverse`).
- **Téléphone (< 768)** : une vue à la fois. Rail + colonne ; un tap sur un salon ou une conversation ouvre la feuille en plein écran, avec ← dans l'en-tête.
- **Tablette (< 1024)** : les membres glissent par-dessus la conversation, avec un voile ; un tap sur le voile les ferme.
- **Tailles de contrôle** : `--control-sm` 32, `--control-md` 40, `--control-lg` 48 ; avatars 32 / 40 / 48.
- **Points de rupture** (`_variables.scss`) : 640, 768, 1024, 1280.

## Elevation & Depth

Profondeur par la valeur : plus une surface est claire, plus elle est proche (cadre → panneau → feuille → nuit levée). Pas d'ombre au repos. Une seule ombre, `--shadow-popover` (`0 12px 32px rgb(0 0 0 / .45), 0 2px 8px rgb(0 0 0 / .3)`), pour ce qui flotte au-dessus de l'app : dialogues, menus, infobulles, barre d'actions d'un message, panneau des membres en tablette, sélecteur d'émojis. Le fond des dialogues est `--surface-overlay`.

### Named Rules
**La règle du plat au repos.** Si ça ne flotte pas, ça n'a pas d'ombre.

## Shapes

- `--radius-sm` (6 px) : badges, petits boutons.
- `--radius-md` (10 px, le rayon historique) : boutons, champs, lignes, tuiles de communauté.
- `--radius-lg` (16 px) : la feuille, dialogues, barre de saisie.
- `--radius-full` : avatars de personnes, pastille « chiffré ».
- Une personne est **ronde**, une communauté est **carrée** (`app-avatar shape="square"`).
- Bordures d'un pixel, jamais de bordure colorée épaisse sur un côté.

## Mouvement

Court, en douceur, et seulement à l'apparition : ce qui se ferme part tout de suite, l'action suivante n'attend jamais. Trois gestes, définis une fois (`styles/_animations.scss`) :

- `pop-in` (fondu, zoom de 0,96, 4 px) : ce qui sort d'un déclencheur. Menus (`--duration-fast`), carte de profil, sélecteur d'émojis, dialogues (`--duration-base`). `transform-origin` du côté du déclencheur.
- `rise-in` (fondu, 6 px) : le contenu qui change sur place. Nouveau message, section des paramètres.
- `slide-in` (fondu, 1,5 rem, côté donné par `--slide`) : les panneaux, depuis le côté où ils vivent. Membres depuis la droite ; sur téléphone, on avance depuis la droite et on revient depuis la gauche.

Boutons : léger enfoncement au clic (`scale(0.97)`). Survols : transitions de couleur en `--duration-fast`. Courbes : `--ease-out` pour entrer, `--ease-in` pour sortir. `prefers-reduced-motion` coupe tout (`_base.scss`).

## Components

### Buttons (`.btn`)
- `.btn` : secondaire (nuit levée, bordure forte). Le choix par défaut.
- `.btn-primary` : vert, **un par vue** (Créer, S'inscrire, Se connecter, Lier).
- `.btn-ghost` : sans fond, pour les actions d'en-tête et les bascules (`aria-pressed="true"` = actif, en vert voilé).
- `.btn-danger` : contour corail (Quitter, Se déconnecter).
- Tailles `.btn-sm` (32) et `.btn-lg` (48) ; `.btn-icon` le rend carré. Une icône seule a toujours un `title`.
- `:disabled` : opacité .45.

### Text buttons (`.link`)
Action discrète dans une ligne de méta (Supprimer, Enregistrer, Nommer modérateur). `.link.danger` pour le destructif.

### Inputs
Les `input`, `textarea` et `select` sont stylés sans classe : puits, bordure forte, rayon md, halo vert au focus, `.input-error` ou `aria-invalid="true"` en corail. `.field` empile label, champ et `.hint` / `.error`. La barre de saisie du chat dessine elle-même le cadre et dépouille son champ.

### Navigation (`.list-row`)
Salons, conversations, membres. En `<button>` elle devient interactive : survol neutre, `aria-current="true"` en vert voilé avec texte en 500 et icône verte. En `<div>` ou `<li>`, c'est une simple ligne.

### Section label (`.section-label`)
Nomme un groupe de lignes (« SALONS », « ADMINS — 1 »), avec au besoin un `.btn-ghost.btn-icon.btn-sm` à droite (+ salon, + groupe, + personnes). Ce n'est pas un titre de page.

### Menu (`.menu`, `.menu-item`)
Avec `CdkMenu` : actions de la communauté (Inviter, Changer l'image, Quitter). Nuit levée, ombre popover ; `.menu-item.danger` pour le destructif. Un élément de menu ferme toujours le menu : une confirmation passe par un dialogue (`LeaveDialog`).

### Rail (`app-community-rail`)
Le chat (`/chatON-logo.svg`) mène aux messages privés, puis les communautés en tuiles carrées lg, puis « + ». Sélection = un anneau vert de 2 px autour de la tuile, détaché par 2 px de cadre, sans lueur. L'image n'est jamais recouverte. Quelques lucioles fixes en bas du rail. Infobulle à droite.

### Barre de titre (`app-titlebar`)
Desktop seulement : les décorations natives sont désactivées (`decorations: false`), l'app dessine ses propres contrôles. Transparente, elle flotte au-dessus de la page (au-dessus des dialogues aussi) ; les layouts réservent `--titlebar-height` en `padding-top` (0 sans barre, ce qui laisse téléphone et navigateur intacts), leur fond passe donc dessous ; tout le vide sert à déplacer la fenêtre (double-clic : agrandir). À droite, Réduire, Agrandir, Fermer en `.btn-ghost.btn-icon.btn-sm` (Lucide `Minus`, `Square`, `X`) ; Fermer survole en corail.

### En-tête de conversation (`app-chat-header`)
`#` + nom pour un salon ; avatar + handle + pastille « Chiffré de bout en bout » (cadenas vert) pour un MP ; nom + nombre de personnes pour un groupe. À droite, bascule des membres (`aria-pressed`). ← sur téléphone.

### Messages (`app-chat-bubble`)
Pas de bulle. Survol = `--state-hover` sur toute la ligne, barre d'actions flottante en haut à droite (supprimer, confirmation au second clic). Fichiers en carte levée, images rayon md.

### Sélecteur d'émojis (`app-emoji-select`)
Émojis **Twemoji** (dépôt officiel maintenu `jdecked/twemoji`), en images SVG servies par l'app : `scripts/twemoji.mjs` les récupère avant `start` et `build` (tag épinglé et sha256) dans `public/twemoji`, non versionné. Pas de CDN (CSP, vie privée, hors ligne). Même dessin sur tous les systèmes. Dans les messages, `EmojiTextPipe` (`@twemoji/parser`) découpe le texte et remplace chaque émoji par son image (taille d'un glyphe, `alt` = l'émoji) ; le champ de saisie, lui, garde l'émoji du système. Le sélecteur ne propose que ce que Twemoji dessine. Noms et recherche dans la langue de l'app (`emojibase-data` fr/en, chargé à la première ouverture). Recherche en tête, onglets en icônes Lucide (jamais un émoji comme icône) qui suivent le défilement, sections sous des `.section-label` collants, aperçu du survol en pied. Un clic insère au curseur ; le sélecteur reste ouvert (dehors ou Échap le ferment). Crédit Twemoji (CC BY 4.0) dans les paramètres du compte.

### Paramètres (`app-settings`)
Plein écran, pas une modale : la grammaire de la coquille. Les sections (Profil, Compte et clé, Appareils, Notifications sur Android, Préférences, À propos) en `.list-row` à la place du rail et de la colonne, la section ouverte dans une feuille. Fermeture par ✕ (avec `Esc` en `kbd`) ou Échap ; les contrôles de fenêtre restent au-dessus. Chaque réglage est une ligne dans un groupe (`--surface-panel`, bordure subtile, rayon lg) : ce que c'est et pourquoi à gauche, l'action à droite ; `.stack` quand l'action prend la largeur (clé secrète, code de liaison). La déconnexion vit dans un groupe à bordure corail, confirmation en place. Téléphone : la liste, puis une section avec ←. Entrée en fondu et léger zoom ; chaque section monte en place quand on la choisit. `ShellService.settings` l'ouvre.

### Carte de profil (`app-profile-card`)
L'objet que l'on personnalisera : bannière, avatar `2xl` (`--avatar-2xl`) à moitié dessus, handle en titre (le `@` en Inter, celui de Bricolage est cerclé ; l'instance en `--content-secondary`). Sans bannière personnalisée, la couleur de fond de l'app (`--surface-frame`). `mine` : oreilles ; `editable` (paramètres) : l'avatar devient le bouton pour changer de photo (appareil photo au survol).

Cliquer sur quelqu'un (membre du panneau, avatar ou nom d'un auteur de message) ouvre sa carte à côté (directive `appProfile`, `app-profile-popover`) : « Envoyer un message » ouvre le MP, « Modifier le profil » sur la sienne ouvre les paramètres. Clic dehors ou Échap ferme, le focus revient au déclencheur.

### Voix (`app-voice-bar`, `app-call-banner`)
Tout l'audio est dans core ; l'UI affiche l'état (`VoiceStore`, événement `voice`).
- **Salon vocal** : ligne `.list-row` avec l'icône Lucide `Volume2` ; un clic rejoint (pas de conversation ouverte). `aria-current` sur le salon où je suis : icône et nom verts, **sans** le fond du salon ouvert (une seule ligne « ouverte » à la fois). Dessous, en retrait, les présents (`app-user-card` dans un `.list-row`, carte de profil au clic, mes oreilles) : en direct quand j'y suis, sinon d'après le relais (39004), donc visibles sans rejoindre. Qui parle : anneau vert de 2 px détaché par 2 px de cadre, comme le rail, autour de l'avatar (pas dessus : le masque des oreilles le couperait). L'anneau tient `--speaking-hold` (300 ms) après la dernière syllabe, pour ne pas clignoter.
- **Création** : l'admin choisit Texte ou Vocal (deux `.btn-ghost` `aria-pressed`) si l'instance a la voix.
- **Barre de voix** : au-dessus de mon compte, même puits, tant que je suis dans un salon ou un appel. Sur téléphone, elle reste sous la conversation ouverte (sans mon compte) : couper le micro et raccrocher ne sont jamais hors d'atteinte. Dans un appel, « Ça sonne… » tant que personne n'a rejoint, puis les autres en avatars sm, anneau compris. « Voix connectée » en `--accent-text` (c'est moi, en direct : la règle du vert), le lieu en dessous, cadenas si chiffré, « Écoute seule » sans micro. Micro, casque, raccrocher (« Raccrocher » dans un appel, « Quitter le vocal » dans un salon) en `.btn-ghost.btn-icon.btn-sm` ; coupé = corail voilé (un avertissement, pas une sélection), et le micro l'est aussi en sourdine ; raccrocher survole en corail. Erreur : ce qui ne va pas et quoi faire, Réessayer si ça peut marcher. Monte en place (`rise-in`).
- **Appel entrant** : bannière flottante en haut au centre (nuit levée, ombre popover, `pop-in`, `--z-call` au-dessus des dialogues), qui appelle (le nom du groupe, sinon les autres appelés), Refuser (`.btn-danger`) et Répondre (`.btn-primary`), et « Répondre vous fera quitter le vocal en cours » si j'y suis. `alertdialog` nommé par son texte ; elle prend le focus (elle-même, pas un bouton : une frappe destinée au message ne doit pas décrocher), Échap refuse, le focus revient où il était. Elle part d'elle-même au bout de 60 s ou si l'appelant raccroche.
- **Dans un MP** : bouton téléphone dans l'en-tête ; les appels sont des notes centrées dans le fil : « @pseudo@instance a lancé un appel » (« Vous avez lancé un appel » pour moi), « Appel terminé ». Un appel en cours a son téléphone vert et un vrai bouton `.btn.btn-sm` Rejoindre.

### Avatar (`app-avatar`)
Image de l'instance (via core) ou initiale sur une teinte dérivée de `seed` (clé publique, id de communauté) : la même clé garde toujours la même couleur. `size` sm / md / lg / xl / 2xl, `shape` circle / square.

### Badges (`.badge`, `.badge-accent`)
Rôles et comptes. Dans la liste des membres, les rôles sont des sections plutôt que des badges.

### Dialogs (`app-modal`)
Nuit levée, rayon lg, ombre popover, fermeture par la croix, un clic dehors ou Échap. Le contenu se plafonne en `min(Nrem, 100vw - 5rem)` (pas de `%` : le dialogue s'ajuste à son contenu).

### Key text (`.key-text`)
npub, nsec, invitations : mono, puits, coupure n'importe où, sélection en un clic.

## Do's and Don'ts

### Do:
- **Do** prendre chaque valeur dans `_tokens.scss` ; une valeur qui manque devient un token, pas un littéral.
- **Do** réutiliser `.btn`, `.list-row`, `.field`, `.badge`, `app-avatar` avant d'écrire un style de composant.
- **Do** garder un seul `.btn-primary` par vue.
- **Do** afficher l'instance de tout handle (`app-user-card`, `splitHandle`).
- **Do** mettre la conversation dans la feuille, la navigation dans le cadre, mon compte en bas sur toute la largeur du rail et de la colonne.
- **Do** laisser `:focus-visible` global faire le focus ; ne pas l'écraser.

### Don't:
- **Don't** `all: unset` sur un bouton : partir de `.btn-ghost` ou `.link`.
- **Don't** de vert au survol, ni en décoration (règle du vert).
- **Don't** de `--content-tertiary` pour du texte lisible.
- **Don't** d'ombre sur ce qui ne flotte pas.
- **Don't** de glyphe Unicode ou d'émoji comme icône : Lucide uniquement (`PawPrint` et `Cat` compris).
- **Don't** de motif de chat sans sens : des oreilles marquent « moi » ou « à l'écoute » ; des pattes, une absence ou une attente.
- **Don't** de `100vh` (préférer `100dvh`), ni de largeur en `%` dans un dialogue.
