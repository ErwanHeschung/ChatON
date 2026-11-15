# CLAUDE.md

Ce fichier guide Claude Code sur ce dépôt. Lis-le avant toute modification.

## Le projet

Une messagerie communautaire **façon Discord** (serveurs/communautés, salons texte, salons vocaux, messages privés), mais :

- **auto-hébergeable** : n'importe qui peut lancer une instance sur un petit VPS ou un Raspberry Pi ;
- **décentralisée et fédérée** : les instances communiquent entre elles ;
- **infra minimale** : les serveurs sont des *aides remplaçables*, pas des autorités ;
- **confort Discord** : inscription e-mail/mot de passe, recherche `@pseudo`, multi-appareil, notifications, historique.

### Principes non négociables

1. **L'identité est une paire de clés.** Le compte appartient à l'utilisateur, pas à une instance.
2. **Tout est signé.** Chaque événement qui transite (message, action, changement de rôle) est signé par son auteur. Un serveur ne peut rien falsifier. Seule exception : le message intérieur d'un MP (kind 14) n'est pas signé lui-même, il est authentifié par le sceau signé qui l'enveloppe (NIP-17, voir `spec/protocol.md` § 5).
3. **Les messages privés sont chiffrés de bout en bout.** Le serveur ne déchiffre jamais rien. Tout le chiffrement se fait côté client.
4. **Aucun serveur n'est indispensable.** Si une instance disparaît, les utilisateurs migrent ailleurs avec leur identité.
5. **La fédération est prévue dès le départ**, même si on commence avec une seule instance.

## Stack

| Composant | Techno | Rôle |
|---|---|---|
| `core/` | Rust (crate lib) | Toute la logique client : identité, clés, signatures, chiffrement (openmls), protocole, sync, stockage local (SQLite) |
| `app/` | Tauri 2 + UI web (TypeScript) | Client unique : Windows, macOS, Linux, iOS, Android |
| `server/` | Go | Relais, annuaire, coffre de clés, passerelle push, hôte de communauté |
| Voix/vidéo | LiveKit (SFU, Go), déployé à côté du serveur | Salons vocaux au-delà de 5 ou 6 personnes |
| `spec/` | Markdown + vecteurs de test | Spécification du protocole, **source de vérité** entre Rust et Go |

> Alternative envisagée pour le desktop : GPUI. Si on l'adopte, ce ne sera qu'une « coque » au-dessus de `core/`, comme Tauri.

## Architecture

```
┌──────────────────────────────┐
│ UI web (app/src)             │  affichage uniquement
└──────────────┬───────────────┘
               │ invoke() / événements Tauri
┌──────────────▼───────────────┐
│ app/src-tauri → core (Rust)  │  identité, chiffrement, sync, base locale
└──────────────┬───────────────┘
               │ protocole (WebSocket/HTTPS)
┌──────────────▼───────────────┐
│ server (Go)                  │  relais, annuaire, coffre, push, communautés
└──────────────────────────────┘
        ⇅ fédération avec d'autres instances
```

### Rôles du serveur (chacun optionnel et remplaçable)

1. **Relais / boîte aux lettres** : stocke et transmet des événements signés et chiffrés, sans pouvoir les lire.
2. **Amorçage** : point d'entrée pour découvrir le réseau.
3. **Relais NAT** : sert de pont quand une connexion directe est impossible (phase P2P ultérieure).
4. **Annuaire** : `pseudo@instance → clé publique + relais`, exposé via `/.well-known/`.
5. **Coffre** : stocke la clé privée **chiffrée par le mot de passe** de l'utilisateur (connexion classique, multi-appareil, récupération).
6. **Hôte de communauté** : historique, application des permissions et des bans, fichiers et médias, intégration LiveKit.
7. **Passerelle push** : envoie des notifications **vides ou chiffrées** via APNs, FCM et UnifiedPush. Le téléphone se réveille, récupère et déchiffre localement.

Le serveur ne fait **jamais** ceci : posséder un compte, lire un message privé, modifier un événement signé.

## Identité et nommage

- **Identité** = clé publique (ed25519 ou secp256k1, voir les décisions ouvertes). Elle est permanente.
- **Nom affiché** : libre, non unique.
- **Handle** : `pseudo@instance`, unique *par instance*, attribué par l'annuaire de l'instance. Un domaine perso est possible en option, après vérification via `.well-known`.
- **Récupération** : clé privée chiffrée par mot de passe (dans l'esprit de NIP-49) et stockée dans le coffre. Plus tard, éventuellement : identité avec clé de rotation (dans l'esprit de `did:plc`).
- L'UI doit **toujours** distinguer clairement `erwan@a.fr` de `erwan@b.fr`, pour éviter les usurpations.

## Fédération et modération

- Découverte : `https://<instance>/.well-known/...` renvoie la clé publique et la liste des relais d'un handle.
- Modèle **piloté par le client** (dans l'esprit du « outbox model » de Nostr) : le client publie sur ses relais et lit sur les relais de ses contacts. Les serveurs restent simples.
- Migration de compte : l'utilisateur publie un événement signé « mes relais sont maintenant X ».
- Blocage à trois niveaux : **instance** (admin), **communauté** (modérateurs), **utilisateur**.
- Les clés étant portables, on bloque **les instances ET les clés**. Les messages d'inconnus vont dans des « demandes de message ».
- Chaque instance a un fichier de règles : mode liste noire ou liste blanche, instances et clés bloquées, limites de débit.

## Protocole : règles pour le code

- Toute modification du format des événements passe **d'abord** par `spec/`.
- La sérialisation signée doit être **déterministe** (JSON canonique ou CBOR, voir les décisions ouvertes). Ne jamais signer un JSON sérialisé « au hasard ».
- `spec/test-vectors/` contient des cas « entrée → octets sérialisés → signature attendue ». **Les tests Rust ET Go doivent les passer.** Ajoute un vecteur pour chaque nouveau type d'événement.
- Chaque événement contient un champ de **version du protocole**.
- Le chiffrement (MLS pour les groupes, chiffrement 1:1 pour les messages privés) est **uniquement dans `core/`**. Pas de MLS côté Go.

## Arborescence prévue

```
.
├── CLAUDE.md
├── spec/
│   ├── protocol.md          # formats d'événements, signature, versions
│   ├── federation.md        # .well-known, découverte, migration
│   └── test-vectors/        # JSON partagé Rust/Go
├── core/                    # crate Rust (lib)
│   └── src/{identity,crypto,protocol,sync,store}/
├── app/                     # Tauri 2
│   ├── src/                 # UI web (TS)
│   └── src-tauri/           # commandes Tauri → core
└── server/                  # Go
    ├── cmd/server/
    └── internal/{relay,directory,vault,push,community,federation}/
```

## Commandes

```bash
# app (Angular, en attendant Tauri à l'étape 4)
cd app && npm ci && npm run build && npm start

# server
cd server && go vet ./... && go test ./... && go run ./cmd/server   # SERVER_PORT=:8080

# vecteurs de test (spec/test-vectors), à régénérer après tout changement de cas
cd server && go run ./cmd/genvectors > ../spec/test-vectors/events.json
```

À venir : `cd core && cargo test` (étape 2), `npm run tauri dev | android dev | ios dev` (étape 4).

## Conventions

- Rust : `cargo fmt`, `cargo clippy -- -D warnings`. Pas de `unwrap()` hors des tests.
- Go : `gofmt`, `go vet`. Erreurs enveloppées avec `%w`.
- UI : aucune logique métier ni crypto en TypeScript. Tout passe par les commandes Tauri vers `core`.
- Secrets : jamais en clair sur disque. Utiliser Keychain, Keystore ou le trousseau de l'OS.
- Le code et les commentaires peuvent être en anglais. La doc du projet est en français.

## Feuille de route

Le détail par étape est dans [`ROADMAP.md`](ROADMAP.md).

1. **Spec v0** : format d'événement, signature, vecteurs de test.
2. **core** : génération et stockage des clés, signature/vérification, client relais WebSocket, base SQLite.
3. **server** : relais (stockage et diffusion d'événements signés), annuaire, `.well-known`.
4. **app** : inscription, connexion, recherche `@pseudo`, messages privés.
5. Coffre (clé chiffrée par mot de passe) et multi-appareil.
6. Communautés : salons, rôles et permissions (événements signés), modération.
7. Chiffrement de bout en bout : messages privés, puis groupes (openmls).
8. Notifications push (plugin Tauri Swift/Kotlin + passerelle Go).
9. Voix et vidéo (LiveKit).
10. Fédération multi-instances, règles de blocage, migration de compte.
11. (Plus tard) P2P direct quand c'est possible (iroh/libp2p), avec les relais en secours.

## Décisions ouvertes

Tranchées le 2026-11-15 :

- [x] **Compatible Nostr** : on réutilise NIP-01, 05, 17, 29, 42, 44, 49, 59, 65 et 98. La version du protocole passe dans un tag `["v","0"]`.
- [x] Courbe : secp256k1 (Schnorr, BIP-340).
- [x] Sérialisation : celle de NIP-01 (`sha256([0,pubkey,created_at,kind,tags,content])`).
- [x] Framework UI web : Angular (existant), dans Tauri 2.
- [x] Desktop : Tauri uniquement. GPUI seulement si une raison concrète apparaît.

Demande avant de trancher l'une de ces questions :

- [ ] Mode par défaut des instances : liste noire (proposé) ou liste blanche ?
- [ ] Attribution des handles : premier arrivé, premier servi, ou vérification par e-mail (étape 3) ?
