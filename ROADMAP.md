# Feuille de route ChatON

## Contexte

ChatON partait d'une UI web Angular 21 et d'une petite API Go avec des comptes JWT détenus par le serveur. `CLAUDE.md` décrit la cible : une messagerie façon Discord, auto-hébergeable et fédérée. L'identité est une paire de clés, chaque événement est signé, les messages privés sont chiffrés de bout en bout et le serveur n'est qu'une aide remplaçable. La logique tourne en Rust (`core/`), le client est Tauri 2 et `spec/` fait foi.

**Décisions prises (2026-11-15) :**
- Protocole **compatible Nostr** : clés secp256k1/Schnorr et sérialisation NIP-01. La version du protocole passe dans un tag (`["v","0"]`).
- **On garde Angular** comme UI dans Tauri 2. Il ne fait qu'afficher et appeler `invoke()`.
- Refonte **sur place** : `frontend/` → `app/` et `backend/` → `server/` via `git mv`.

Règle pour toute la feuille de route : chaque étape part d'une NIP et d'une bibliothèque existantes, et le code maison ne comble que les manques. Bibliothèques : **rust-nostr** (`nostr`, `nostr-sdk`), plus **khatru** et **go-nostr** côté Go.

## Correspondance étapes ↔ NIP

| Étape | Standard réutilisé | Bibliothèque réutilisée |
|---|---|---|
| Événements, signature | NIP-01 | `nostr` (Rust), `go-nostr` |
| Annuaire, `.well-known` | NIP-05 | ~20 lignes de Go |
| Liste de relais, migration | NIP-65 | gossip de `nostr-sdk` |
| Authentification au relais | NIP-42 | intégré à khatru |
| Coffre (clé chiffrée) | NIP-49 (`ncryptsec`) | `nostr` (Rust) |
| Messages privés (déjà E2EE) | NIP-17 + NIP-44 + NIP-59 | `nostr` (Rust) |
| Communautés | NIP-29 (groupes hébergés par un relais) | khatru + relay29 |
| Auth HTTP (envois, jeton LiveKit) | NIP-98 | go-nostr |
| E2EE des groupes | MLS sur Nostr (NIP-EE / Marmot) | openmls |

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
- Connexion : import d'une clé `nsec` existante pour un handle, vérifiée par NIP-05. L'e-mail et le mot de passe arrivent avec le coffre (étape 5).
- Recherche `@pseudo@instance` (NIP-05) et MP NIP-17. Un correspondant est affiché par son handle **vérifié** (son profil pointe vers un `nip05` qui renvoie bien sa clé), sinon par son npub abrégé.
- `app/src-tauri` : trousseau de l'OS (Secret Service sous Linux, Windows et macOS branchés mais pas testés), commandes `current_account`, `register`, `login`, `search_handle`, `peer`, `send_dm`, `conversations`, `messages`, et un événement `dm` pour chaque message reçu. `CHATON_PROFILE` permet de lancer un second compte sur la même machine.
- Angular : `AuthService` et `DmStore` appellent `invoke`, et une liste de conversations (`dm-list`) remplace la bannière dans la vue MP.
- Serveur : un filtre qui peut correspondre à un gift wrap doit être authentifié et viser `#p` = soi (correction de l'étape 3, qui laissait passer la diffusion en direct).
- Test de bout en bout : `core/tests/e2e.rs`, contre un vrai serveur.
- **Terminé quand :** deux comptes de la même instance se trouvent par `@pseudo` et échangent des MP NIP-17 sur desktop. Validé par le test de bout en bout de `core` et à la main dans deux fenêtres (Linux, WSL).

## Étape 5 : coffre et multi-appareil

- Le serveur stocke le blob `ncryptsec` sous le handle. On le récupère avec handle + mot de passe : le mot de passe ne sert qu'à déchiffrer la clé côté client, et le serveur ne le voit jamais. L'e-mail est facultatif et ne sert qu'à la récupération et aux notifications.
- Un nouvel appareil se connecte, déchiffre la clé et synchronise l'historique depuis les relais.
- Reportés de l'étape 4 : déconnexion (sans coffre, supprimer la clé ferait perdre le compte), export de la clé, durée de conservation des MP sur le relais.
- Plus tard, si l'historique devient lent à charger : cache local des messages en clair, chiffré au repos.

## Étape 6 : communautés

- D'abord, migrer le serveur de `nbd-wtf/go-nostr`, `fiatjaf/khatru` et `fiatjaf/eventstore` (archivés ou en maintenance depuis 2025) vers le module unifié `fiatjaf.com/nostr`. Il n'a pas de backend SQLite : les événements passent en bbolt ou LMDB (pur Go, fin de cgo), et la table des handles a besoin de son propre stockage. Vérifier qu'il existe des versions taguées avant de migrer.
- Groupes NIP-29 hébergés par l'instance (relay29 sur khatru). Les salons sont des groupes ou des sous-groupes. Rôles, permissions et bans sont des événements de modération signés.
- Fichiers et médias via un envoi façon Blossom ou NIP-96, authentifié par NIP-98.
- Brancher les composants existants `server-channels` et `server-banner`.

## Étape 7 : E2EE des groupes

- Les groupes privés utilisent MLS (openmls dans core), en suivant NIP-EE / Marmot plutôt que d'inventer un format. Les communautés publiques restent en clair et sont modérées par leur hôte.

## Étape 8 : notifications push

- Passerelle Go : l'appareil enregistre un jeton chiffré, et la passerelle envoie des push vides pour le réveiller (APNs, FCM, UnifiedPush). L'app récupère et déchiffre en local. Le plugin mobile Tauri est écrit en Swift et en Kotlin.

## Étape 9 : voix et vidéo

- LiveKit déployé à côté du serveur. L'hôte de communauté délivre les jetons LiveKit après une authentification NIP-98 et une vérification des permissions.

## Étape 10 : fédération

- Fichier de règles de l'instance (`rules.yaml`), reporté de l'étape 3 : mode liste noire ou liste blanche, clés et instances bloquées, limites de débit configurables. Appliqué dans les hooks `RejectEvent` de khatru.
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
- Règle d'attribution des handles (premier arrivé, premier servi, ou vérification par e-mail) : à trancher à l'étape 3.
