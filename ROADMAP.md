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

## Étape 1 : spec v0

- `spec/protocol.md` décrit la forme d'événement NIP-01, le tag de version et la liste des `kind` utilisés (0 profil, 10002 liste de relais, 14/1059 MP, 9xxx/39xxx NIP-29). Il indique aussi quelles NIP on suit et où on s'en écarte.
- `spec/federation.md` décrit `.well-known/nostr.json` (NIP-05), la découverte des relais (NIP-65) et la migration via un nouvel événement de liste de relais.
- `spec/test-vectors/events.json` contient des cas `{privkey, event (non signé), serialized, id, sig}`, un par `kind`. Les signatures Schnorr utilisent un aléa auxiliaire fixe (permis par BIP-340), ce qui rend les vecteurs déterministes.
- **Terminé quand :** les vecteurs existent et les tests des étapes 2 et 3 les chargent.

## Étape 2 : core (Rust)

- Modules : `identity` (génération de clés, chiffrement et déchiffrement NIP-49), `protocol` (construction et vérification d'événements, fine couche au-dessus de `nostr`), `sync` (client relais via `nostr-sdk`), `store` (SQLite via la base sqlite de `nostr-sdk`, ou `rusqlite`).
- Les secrets vont dans le trousseau de l'OS via le crate `keyring`.
- **Terminé quand :** `cargo test` passe les `spec/test-vectors`, et `cargo clippy -- -D warnings` ne remonte rien.

## Étape 3 : serveur (Go)

- `server/internal/relay` : relais khatru avec stockage SQLite, authentification NIP-42 et limite de débit par clé et par IP.
- `server/internal/directory` : `POST /handle` (une requête signée réserve `pseudo` → clé publique) et `GET /.well-known/nostr.json?name=`.
- Fichier de règles de l'instance (`rules.yaml`) : mode liste noire par défaut, clés et instances bloquées, limites de débit. Appliqué dans les hooks `RejectEvent` de khatru.
- **Terminé quand :** `go test ./...` passe les mêmes vecteurs, et un client Nostr standard (par exemple `nak`) peut publier et lire sur le relais.

## Étape 4 : app (Tauri 2 + Angular)

- `app/src-tauri` expose des commandes qui enveloppent `core` : `register`, `login`, `search_handle`, `send_dm`, `list_dms`. Les événements remontent à l'UI via `emit`.
- Angular les appelle par un petit `TauriService` (`invoke` de `@tauri-apps/api`). Les composants existants de connexion, inscription, chat et liste d'utilisateurs y sont rebranchés.
- Inscription : générer la clé dans core, réserver le handle auprès de l'annuaire, publier les `kind` 0 et 10002, puis envoyer le `ncryptsec` au coffre (étape 5).
- L'UI affiche toujours `pseudo@instance` en entier.
- **Terminé quand :** deux comptes de la même instance se trouvent par `@pseudo` et échangent des MP NIP-17 sur desktop.

## Étape 5 : coffre et multi-appareil

- Le serveur stocke le blob `ncryptsec` sous le handle. On le récupère avec handle + mot de passe : le mot de passe ne sert qu'à déchiffrer la clé côté client, et le serveur ne le voit jamais. L'e-mail est facultatif et ne sert qu'à la récupération et aux notifications.
- Un nouvel appareil se connecte, déchiffre la clé et synchronise l'historique depuis les relais.

## Étape 6 : communautés

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
