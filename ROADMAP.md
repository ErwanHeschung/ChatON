# Feuille de route ChatON

## Contexte

ChatON partait d'une UI web Angular 21 et d'une petite API Go avec des comptes JWT détenus par le serveur. `CLAUDE.md` décrit la cible : une messagerie façon Discord, auto-hébergeable et fédérée. L'identité est une paire de clés, chaque événement est signé, les messages privés sont chiffrés de bout en bout et le serveur n'est qu'une aide remplaçable. La logique tourne en Rust (`core/`), le client est Tauri 2 et `spec/` fait foi.

**Décisions prises (2026-11-15) :**
- Protocole **compatible Nostr** : clés secp256k1/Schnorr et sérialisation NIP-01. La version du protocole passe dans un tag (`["v","0"]`).
- **On garde Angular** comme UI dans Tauri 2. Il ne fait qu'afficher et appeler `invoke()`.
- Refonte **sur place** : `frontend/` → `app/` et `backend/` → `server/` via `git mv`.

Règle pour toute la feuille de route : chaque étape part d'une NIP et d'une bibliothèque existantes, et le code maison ne comble que les manques. Bibliothèques : **rust-nostr** (`nostr`, `nostr-sdk`), plus **`fiatjaf.com/nostr`** côté Go (khatru, eventstore et nip29 réunis ; il remplace go-nostr et khatru depuis l'étape 6).

## Correspondance étapes ↔ NIP

| Étape | Standard réutilisé | Bibliothèque réutilisée |
|---|---|---|
| Événements, signature | NIP-01 | `nostr` (Rust), `fiatjaf.com/nostr` |
| Annuaire, `.well-known` | NIP-05 | ~20 lignes de Go |
| Liste de relais, migration | NIP-65 | gossip de `nostr-sdk` |
| Authentification au relais | NIP-42 | intégré à khatru |
| Liaison d'appareils | aucune NIP : kind éphémère 24135 + SPAKE2 + NIP-44 | `spake2` (RustCrypto), `nostr` (Rust) |
| Messages privés (déjà E2EE) | NIP-17 + NIP-44 + NIP-59 | `nostr` (Rust) |
| Communautés | NIP-29 (groupes hébergés par un relais) + NIP-51 (10009) | `fiatjaf.com/nostr/nip29` + règles maison |
| Fichiers et médias | Blossom (BUD-01, 02) + NIP-17 kind 15 + NIP-92 `imeta` | `fiatjaf.com/nostr/khatru/blossom`, `nostr-blossom` (Rust), `aes-gcm` |
| Notifications push | WebPush (RFC 8030, 8291, 8292) + UnifiedPush | `webpush-go`, connecteur UnifiedPush (Kotlin), `tauri-plugin-notification` |
| Auth HTTP (push, jeton LiveKit) | NIP-98 | `fiatjaf.com/nostr` |
| Groupes chiffrés | NIP-17 à plusieurs destinataires ; MLS (Marmot) plus tard | `nostr` (Rust) |

Avec NIP-17, **les messages privés sont chiffrés de bout en bout dès l'étape 4**. On avance donc une partie de l'étape 7, et on évite de construire des MP en clair qu'il faudrait jeter ensuite.

---

## Phase 0 : restructuration du dépôt ✅

- `frontend/` → `app/`, `backend/` → `server/`, `server/cmd/api` → `server/cmd/server`, la doc de refonte → `CLAUDE.md` (décisions reportées).
- Chemins corrigés dans `package.json` (lint-staged), `.github/dependabot.yml`, `server/go.mod` et `server/Dockerfile` (binaire `chaton-server`).
- Serveur : comptes JWT et Postgres supprimés (handlers, services, repository, entités, migrations). Il reste la config, le CORS, `GET /health` et les utilitaires `handlers/utils.go` et `dto/error_response.go`, réutilisés par l'annuaire. SQLite arrive avec khatru à l'étape 3.
- App : le code d'auth HTTP (`auth.service.ts`, store, formulaires) reste en place jusqu'à l'étape 4. L'appel `refresh` au démarrage échoue et l'app démarre déconnectée.
- `spec/` et `core/` sont créés aux étapes 1 et 2.
- **Terminé quand :** `cd app && npm run build` et `cd server && go vet ./... && go build ./...` passent.

## Étape 1 : spec v0 ✅

- [`spec/protocol.md`](spec/protocol.md) : format NIP-01, règles de sérialisation, tag de version (sauf sceau et emballage), kinds et NIP utilisés, MP NIP-17 (déni partiel, pas de confidentialité persistante), validation à la réception.
- [`spec/federation.md`](spec/federation.md) : `.well-known/nostr.json` (NIP-05), réservation de handle via NIP-98, relais NIP-65 et 10050, migration.
- [`spec/test-vectors/events.json`](spec/test-vectors/events.json) : 13 vecteurs valides (un par kind, plus les cas d'échappement) et 3 invalides. Signatures reproductibles (`aux_rand` nul). Générés par `server/cmd/genvectors`, vérifiés en Go et contre le vecteur officiel BIP-340 n°0.
- **Terminé quand :** les vecteurs existent et les tests Go passent. Le test Rust de l'étape 2 doit les reproduire à l'octet près.

## Étape 2 : core (Rust) ✅

- Crate `chaton-core` (le nom `core` entrerait en conflit avec la bibliothèque standard de Rust), fine couche au-dessus de `nostr` 0.45 :
  - `identity` : génération de clés, NIP-49 (`ncryptsec`), trousseau via `keyring-core` (le store de la plateforme est choisi par l'app) ;
  - `event` : tag `v`, nettoyage des caractères de contrôle, validation dans l'ordre de la spec § 6 ;
  - `dm` : NIP-17, avec un emballage pour le destinataire et un pour soi (multi-appareil).
- `tests/vectors.rs` reproduit à l'octet près les 13 sérialisations et ids et les 7 signatures générés en Go : le contrat Go↔Rust tient. Le vecteur officiel NIP-49 passe aussi.
- Client relais (`nostr-sdk`) et base SQLite (`nostr-sqlite`) **déplacés à l'étape 4**, quand les commandes Tauri en définiront le besoin.
- **Terminé quand :** `cargo test` passe les `spec/test-vectors`, et `cargo clippy --all-targets -- -D warnings` ne remonte rien. ✅

## Étape 3 : serveur (Go) ✅

- `server/internal/relay` : relais khatru avec stockage SQLite, authentification NIP-42 et limite de débit par IP. Écriture réservée aux utilisateurs locaux et aux MP qui leur sont adressés ; un gift wrap n'est servi qu'à son destinataire (`spec/federation.md` § 2).
- `server/internal/directory` : `POST /api/handle` (requête signée NIP-98, premier arrivé premier servi, un handle par clé) et `GET /.well-known/nostr.json?name=`. Même fichier SQLite que les événements.
- Clés bloquées via `SERVER_BLOCKEDPUBKEYS`. Le fichier de règles complet passe à l'étape 10.
- **Terminé quand :** `go test ./...` passe (vecteurs, annuaire, politique du relais), et un client Nostr standard peut publier et lire sur le relais.

## Étape 4 : app (Tauri 2 + Angular) ✅

- `core/src/session.rs` : client relais (`nostr-sdk`, authentification NIP-42 automatique) et base locale (`nostr-sqlite`). Sur le disque, il n'y a que les gift wraps chiffrés ; les messages sont déchiffrés à la lecture.
- Inscription : la clé est générée dans `core`, le handle est réservé par une requête NIP-98, la clé va dans le trousseau, puis les `kind` 0, 10002 et 10050 sont publiés. Au lancement, un compte déjà présent ouvre directement le chat.
- Connexion : import d'une clé `nsec` existante pour un handle, vérifiée par NIP-05. La liaison d'appareils arrive à l'étape 5.
- Recherche `@pseudo@instance` (NIP-05) et MP NIP-17. Un correspondant est affiché par son handle **vérifié** (son profil pointe vers un `nip05` qui renvoie bien sa clé), sinon par son npub abrégé.
- `app/src-tauri` : trousseau de l'OS (Secret Service sous Linux, Windows et macOS branchés mais pas testés), commandes `current_account`, `register`, `login`, `search_handle`, `peer`, `send_dm`, `conversations`, `messages`, et un événement `dm` pour chaque message reçu. `CHATON_PROFILE` permet de lancer un second compte sur la même machine.
- Angular : `AuthService` et `DmStore` appellent `invoke`, et une liste de conversations (`dm-list`) remplace la bannière dans la vue MP.
- Serveur : un filtre qui peut correspondre à un gift wrap doit être authentifié et viser `#p` = soi (correction de l'étape 3, qui laissait passer la diffusion en direct).
- Test de bout en bout : `core/tests/e2e.rs`, contre un vrai serveur.
- **Terminé quand :** deux comptes de la même instance se trouvent par `@pseudo` et échangent des MP NIP-17 sur desktop. Validé par le test de bout en bout de `core` et à la main dans deux fenêtres (Linux, WSL).

## Étape 5 : multi-appareil par liaison d'appareils ✅

- **Pas de coffre** (décision du 2026-11-15, voir `CLAUDE.md`) : avec un coffre protégé par mot de passe, un mot de passe fuité donne la clé pour toujours. Pas d'e-mail non plus.
- Liaison d'appareils (`spec/protocol.md` § 8) : l'appareil connecté affiche un code `XXXX-XXX-XXX` (5 minutes, un essai), le nouvel appareil saisit handle + code. SPAKE2 sur des événements éphémères (kind 24135) relayés sans être stockés, puis la clé chiffrée NIP-44 avec la clé partagée, après confirmation sur l'appareil connecté. Le nouvel appareil vérifie la clé contre l'annuaire.
- Relais : kind 24135 accepté de n'importe quelle clé (4 Kio max, un tag `c`), un filtre doit viser exactement un `#c`.
- `core/src/link.rs` (code, échange, canal relais) ; `session.rs` : `export_key`, `logout`, `login` renommé `import_key`, et historique complet en paginant avec `until` (le stockage SQLite d'eventstore limite chaque requête à 100 événements, négentropie comprise).
- App : accueil « S'inscrire | Se connecter avec un autre appareil | Importer une clé », écran « Enregistrez votre clé secrète » après l'inscription, fenêtre Compte (lier un appareil, afficher la clé, se déconnecter avec avertissement) ouverte depuis sa carte d'utilisateur.
- Conservation des MP sur le relais : gardée, le multi-appareil en a besoin. À revoir plus tard.
- Plus tard : rotation de clé (dans l'esprit de `did:plc`), la vraie réponse à une clé volée ; QR code du lien sur mobile (étape 8) ; cache local des messages en clair, chiffré au repos, si l'historique devient lent.
- **Terminé quand :** un second appareil se lie par handle + code, retrouve tout l'historique, et ses messages apparaissent en direct sur le premier. Validé par `core/tests/e2e.rs` (y compris code erroné, code expiré, déconnexion puis import de la clé).

## Étape 6 : communautés ✅

- Migration du serveur vers `fiatjaf.com/nostr` (successeur de go-nostr, khatru et eventstore), épinglé sur un commit faute de version taguée. Stockage en **bbolt** (pur Go, plus de cgo, build Raspberry Pi simple) : événements et handles dans le même fichier (`chaton.bolt`). Les bases SQLite de dev ne sont pas migrées. relay29 n'est pas utilisé (abandonné, ancienne pile, pas de sous-groupes).
- Bug de la bibliothèque : elle n'échappe pas U+0008 ni U+000C en calculant l'id (NIP-01 l'exige). Contournement : le relais refuse ces caractères, `core` les retire, `genvectors` a son propre sérialiseur (`spec/protocol.md` § 1). À signaler en amont.
- Modèle (`spec/protocol.md` § 9) : une communauté est un groupe NIP-29 `private`, `closed` et `restricted` ; ses salons sont des sous-groupes (`parent`/`child`). Membres et rôles (`admin`, `moderator`) sont ceux de la communauté. Entrée sur invitation seulement (`<hôte>'<communauté>/<CODE>`), pas d'annuaire public. Mes communautés sont dans ma liste 10009 (NIP-51), que tous mes appareils lisent.
- `server/internal/community` : applique les permissions, publie l'état (39000–39003) signé par **la clé du relais** (`SERVER_RELAYKEY`, sinon `relay.key` créé à côté de la base, annoncé en NIP-11 `self`), et le recharge au démarrage. Lecture réservée aux membres authentifiés, y compris la diffusion en direct (un membre retiré ne reçoit plus rien).
- `core/src/community.rs` : créer (avec un salon `general`), salons, invitations, rejoindre, quitter, membres, rôles, retrait, messages et suppression. L'état n'est accepté que signé par la clé NIP-11 du relais. Synchronisation à la connexion et abonnement en direct ; `listen` émet désormais un `Update` (MP, message de salon, suppression, changement de communauté).
- App : barre des communautés réelle avec « + » (créer | rejoindre), salons et invitation dans la colonne de gauche, salon ouvert dans le chat (suppression), membres avec badges et actions d'admin à droite. Les maquettes (catégories, 100 éléments factices) sont supprimées.
- **Terminé quand :** une communauté se crée, invite, discute dans deux salons, supprime un message, nomme un modérateur et retire un membre. Validé par `server/internal/relay/community_test.go`, `core/tests/e2e.rs` (y compris le second appareil lié qui retrouve la communauté) et à la main dans deux fenêtres.
- Plus tard : catégories, permissions par salon, rôles personnalisés, liste de bannis (un membre retiré a seulement besoin d'une nouvelle invitation), révocation d'invitation.

## Étape 6b : fichiers et médias ✅

- Modèle (`spec/protocol.md` § 10) : Blossom (BUD-01, 02, auth kind 24242). Chacun envoie sur **sa propre instance**, 25 Mio max par fichier. **Toute pièce jointe est chiffrée par le client** (AES-256-GCM, clé et nonce neufs par fichier, dans le message) : le serveur ne garde que du chiffré, lisible par hash sans le message qui porte la clé. Dans un MP : rumeur NIP-17 kind 15. Dans un salon : kind 9 avec un tag NIP-92 `imeta`. Les photos de profil et de communauté sont publiques, en clair.
- Pas de fuite d'IP : le client ne télécharge seul que depuis mon instance et celles de mes communautés ; ailleurs, il faut un clic. Une photo n'est affichée que si elle est sur l'instance de son propriétaire (domaine NIP-05, hôte de la communauté). La CSP de l'app n'autorise plus aucune image distante (`blob:` seulement) ; le cache local reste chiffré.
- `server/internal/media` : `khatru/blossom` avec les fichiers dans `SERVER_BLOBDIR` (sinon `blobs/` à côté de la base) et un index à part (`blobs.bolt`, jamais dans la base du relais). Envoi réservé aux handles locaux ; `Guard` refuse un `Content-Length` absent ou trop gros avant que la bibliothèque n'alloue, et coupe `/mirror`, `/media` et `/report`.
- Bugs de la bibliothèque, à signaler en amont : l'index (`EventStoreBlobIndexWrapper.Keep`/`Delete`) écrit pendant sa propre transaction de lecture, ce qui bloque bbolt (contourné par `blobIndex`) ; l'envoi alloue `Content-Length` octets avant `RejectUpload` ; le lecteur de `LoadBlob` n'est jamais fermé ; `/mirror` télécharge une URL distante avant toute vérification.
- `core/src/media.rs` : chiffrement, envoi, téléchargement plafonné avec vérification des hash (`x` du chiffré, `ox` du clair). Pièces jointes dans les salons et les MP, photos de profil (kind 0) et de communauté.
- App : trombone et collage d'image dans la barre de message, images affichées dans le fil, « Enregistrer » pour tout fichier, photos dans les cartes, la barre des communautés et leur bannière. Les octets passent par l'IPC brut de Tauri, sans JSON.
- **Terminé quand :** une image et un PDF circulent dans un salon et dans un MP, et les photos de profil et de communauté s'affichent chez l'autre. Validé par `server/internal/media/media_test.go`, `core/tests/e2e.rs` (le serveur ne voit que du chiffré, mauvaise clé et mauvais hash refusés, hôte étranger refusé sans clic, fichier trop gros) et à la main dans deux fenêtres.
- Plus tard : quotas par utilisateur, nettoyage des fichiers dont le message est supprimé, miniatures et dimensions, lecteur vidéo, progression de l'envoi, listes de serveurs Blossom (10063) et miroirs, téléchargement automatique entre instances (étape 10).

## Étape 7 : conversations de groupe chiffrées ✅

- Prévu avec MLS via Marmot, mais Marmot n'est pas utilisable proprement aujourd'hui (2026-11-15) : la crate publiée `mdk-core` 0.8 dépend de nostr 0.44 (ChatON est en 0.45, deux jeux de types incompatibles) et d'openmls 0.8.1, que l'équipe MDK a dépassé pour des correctifs de sécurité. La version courante de MDK n'est pas publiée, épingle openmls sur un commit git et change plusieurs fois par jour ; la spec Marmot reste un brouillon.
- Modèle (`spec/protocol.md` § 5, groupes) : groupes **NIP-17**, comme les groupes de MP de Discord. Le salon est {auteur} ∪ les `p` ; un emballage par membre, plus une copie pour moi. 10 membres au plus. Nom par le tag `subject` (créer un groupe, c'est le nommer). Les membres ne changent jamais : « ajouter des personnes » crée un nouveau groupe. Lisible par les autres clients NIP-17 (0xchat, Amethyst).
- Les communautés restent en clair : leur instance modère et sert l'historique.
- `core/src/dm.rs` : un message porte son `room` (membres triés) et son `subject`. `core/src/session.rs` : `send_dm`, `send_dm_file` et `messages` prennent les membres, `name_group` crée ou renomme, une conversation par salon. Si certains membres sont injoignables, ma copie part quand même et l'erreur le dit.
- App : « Nouveau groupe » dans la liste des MP (nom + handles), groupes avec leur nom et leur taille, membres, « Renommer » et « Ajouter des personnes » à droite, renommages affichés comme des notes dans le fil.
- **Terminé quand :** trois personnes discutent dans un groupe nommé (texte et fichier), le renomment, et un 1:1 entre deux d'entre elles reste à part. Validé par `core/src/dm.rs` (aller-retour à trois, plafonds, nom vide), `core/tests/e2e.rs` et à la main dans trois fenêtres.
- Limites connues : pas de confidentialité persistante (comme les MP), membres figés, coût linéaire en nombre de membres.
- Plus tard : MLS via Marmot quand il sera publié et stable (confidentialité persistante, membres modifiables) ; quitter ou masquer un groupe ; photo de groupe ; demandes de message des inconnus.

## Étape 8 : notifications et push ✅

- Portée (2026-11-15) : notifications sur desktop, première version Android avec un vrai push. iOS attend un Mac (Xcode, compte développeur Apple).
- Push Android par **UnifiedPush** : pas de compte Google, adapté à l'auto-hébergement. L'utilisateur installe un distributeur (ntfy, par exemple). Les plugins Tauri existants ne gèrent UnifiedPush que sur Linux, d'où un petit code Kotlin à nous.
- Modèle (`spec/federation.md`, notifications push) : l'appareil enregistre son adresse WebPush sur **sa propre instance** (`/api/push`, NIP-98). Un gift wrap pour un utilisateur local déclenche un WebPush chiffré (RFC 8291) au contenu constant, signé VAPID (`SERVER_VAPIDKEY` ou `vapid.key`). Pas de push pour la copie à soi-même publiée par un de mes appareils authentifiés. Un push par adresse toutes les 10 s au plus, adresses mortes (404/410) oubliées, adresses non publiques refusées (pas de SSRF).
- `server/internal/push` (NIP-98 déplacé dans `internal/nip98`, partagé avec l'annuaire). `core/src/push.rs` : clé VAPID, inscription, désinscription à la déconnexion.
- Desktop : notification système pour un MP reçu quand la fenêtre n'a pas le focus (`tauri-plugin-notification`).
- Android : projet `app/src-tauri/gen/android` versionné. Clé dans le Keystore (`android-native-keyring-store`). `PushPlugin.kt` (inscription UnifiedPush), `ChatonPushService.kt` (notification générique « Nouveaux messages », remplacée à chaque push, pas quand l'app est au premier plan). Réglages : état des notifications et lien vers ntfy.
- **Terminé quand :** un MP reçu application fermée réveille le téléphone, et ouvrir l'app l'affiche ; écrire depuis un de mes appareils ne réveille pas les autres. Validé par `server/internal/push/push_test.go` (déchiffrement du push, copie à soi, 410, plafond, règles d'adresse), `core/tests/e2e.rs` et à la main sur un téléphone.
- Plus tard : iOS (APNs, Mac nécessaire), repli FCM, aperçu du contenu sur Android (`core` dans le service de push), notifications des salons (mentions), sourdine par conversation.

## Étape 9 : voix ✅ (desktop ; Android compilé, à essayer sur un téléphone)

- Portée (2026-11-16) : la voix seule, dans les salons des communautés et en appel dans les MP (1:1 et groupes de 10 au plus). Vidéo et partage d'écran plus tard.
- **SDK Rust LiveKit** (`livekit` 0.11, feature `voice` de `core`) sur toutes les plateformes : WebKitGTK n'a pas WebRTC, donc pas de SDK JS. L'UI n'affiche que l'état (événement `voice`).
- Standards réutilisés (`spec/protocol.md` § 11) : NIP-29 « Live AV spaces » (tag `livekit` sur le salon, présence en 39004 signée par le relais), jetons LiveKit, et kind 14 pour les appels (`call`, `call-end`). Aucun kind propre.
- `server/internal/voice` : jetons de salon (`/.well-known/nip29/livekit/<salon>`, NIP-98, membres seulement) et d'appel (`/api/voice/call`, sans état), webhooks LiveKit pour la présence, éjection des membres retirés, réconciliation au démarrage. Voix coupée sans `SERVER_LIVEKIT*`.
- Appels en MP **chiffrés de bout en bout** (LiveKit E2EE, clé dans l'invitation chiffrée) : l'instance sait qui appelle qui, pas ce qui se dit. Un appel à deux se termine pour les deux quand l'un raccroche ; en groupe, quand le dernier part. La voix des salons n'est pas chiffrée de bout en bout, comme leur texte.
- App : salons vocaux avec présence sans rejoindre et anneau « parle », barre de voix (micro, son, raccrocher ; sous la conversation sur téléphone), bannière d'appel entrant, notes d'appel, sonnerie notifiée fenêtre en arrière-plan.
- **Partage d'écran** : un écran ou une fenêtre depuis le desktop (`DesktopCapturer` de libwebrtc : Windows, X11, portail Wayland, macOS), regardé partout, Android compris. Piste vidéo LiveKit `screen_share` dans la salle de voix, chiffrée de bout en bout dans les appels. On ne télécharge que l'écran regardé (`auto_subscribe` coupé). WebKitGTK n'ayant pas WebRTC, core encode les images en JPEG (15 par seconde au plus) et les passe à la webview par un canal Tauri binaire, dessinées dans un `<canvas>`.
- Android : `VoicePlugin.kt` (contexte de WebRTC, permission micro, mode appel), `libwebrtc.jar` dans `gen/android/app/libs`, symboles JNI de WebRTC exportés par `build.rs`.
- Dev : `npm run dev:livekit` (Docker) et `server/.env`. LiveKit réglé pour que l'anneau « parle » suive la voix (`audio.update_interval: 100`).
- **Terminé quand :** deux membres se parlent dans un salon vocal, voient qui parle, coupent micro et son ; un troisième voit qui y est sans rejoindre ; un appel 1:1 sonne, se connecte chiffré et se termine des deux côtés ; un écran partagé se regarde dans un salon et dans un appel. Validé par `server/internal/voice/voice_test.go`, `core/tests/e2e.rs` (`voice_channel_presence_and_an_encrypted_call`) et à la main sur desktop (deux profils).
- **Reste :** l'essai sur un téléphone (WSL en réseau « mirrored », adresse LAN pour l'instance et LiveKit, `rtc.node_ip`, pare-feu TCP 7880-7881 et UDP 7882).
- Plus tard : caméra, son de l'écran partagé, partage depuis Android (MediaProjection), « en direct » visible sans rejoindre, plusieurs écrans à la fois, sonnerie app fermée (service Android au premier plan), son de sonnerie, choix du micro et des haut-parleurs, « appel manqué », TURN/TLS pour les réseaux stricts, limites de débit sur les jetons, appels entre instances (étape 10), iOS.

## Étape 10 : fédération

- Fichier de règles de l'instance (`rules.yaml`), reporté de l'étape 3 : mode liste noire ou liste blanche, clés et instances bloquées, limites de débit configurables. Appliqué dans le hook `OnEvent` de khatru.
- Outbox model via NIP-65 : le client lit sur les relais de ses contacts, et NIP-05 résout les handles d'autres instances. Le blocage se fait à trois niveaux (règles d'instance, modération de communauté, listes de mise en sourdine NIP-51 côté utilisateur). Les messages d'inconnus arrivent dans « demandes de message ». Migrer, c'est publier une nouvelle liste de relais 10002.

## Étape 11 : (plus tard) P2P direct

- iroh dans core, avec les relais en secours.

---

## Vérification

- Phase 0 : les deux builds passent, et lint-staged tourne toujours sur `app/`.
- Chaque étape a son critère « Terminé quand » ci-dessus. Le contrôle entre Rust et Go repose sur les `spec/test-vectors` partagés, chargés à la fois par `cargo test` et `go test`.
- De bout en bout à partir de l'étape 4 : lancer le serveur en local, puis deux instances `npm run tauri dev` avec des profils séparés. S'inscrire, chercher `@pseudo`, échanger un MP, et vérifier avec `nak req` que le relais ne stocke que les gift wraps chiffrés.

## Encore ouvert (à trancher en arrivant à l'étape)

- Desktop avec GPUI en plus de Tauri : attendre une raison concrète.
