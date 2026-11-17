# CLAUDE.md

Ce fichier guide Claude Code sur ce dépôt. Lis-le avant toute modification.

## Le projet

Une messagerie communautaire **façon Discord** (serveurs/communautés, salons texte, salons vocaux, messages privés), mais :

- **auto-hébergeable** : n'importe qui peut lancer une instance sur un petit VPS ou un Raspberry Pi ;
- **décentralisée et fédérée** : les instances communiquent entre elles ;
- **infra minimale** : les serveurs sont des *aides remplaçables*, pas des autorités ;
- **confort Discord** : inscription en un clic, recherche `@pseudo`, multi-appareil (liaison par code), notifications, historique.

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
| `server/` | Go | Relais, annuaire, passerelle push, hôte de communauté |
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
│ server (Go)                  │  relais, annuaire, push, communautés
└──────────────────────────────┘
        ⇅ fédération avec d'autres instances
```

### Rôles du serveur (chacun optionnel et remplaçable)

1. **Relais / boîte aux lettres** : stocke et transmet des événements signés et chiffrés, sans pouvoir les lire.
2. **Amorçage** : point d'entrée pour découvrir le réseau.
3. **Relais NAT** : sert de pont quand une connexion directe est impossible (phase P2P ultérieure).
4. **Annuaire** : `pseudo@instance → clé publique + relais`, exposé via `/.well-known/`.
5. **Liaison d'appareils** : transporte, sans les stocker, les messages éphémères qui lient un nouvel appareil (PAKE, `spec/protocol.md` § 8). Le serveur ne stocke **jamais** la clé privée, même chiffrée.
6. **Hôte de communauté** : historique, application des permissions et des bans, fichiers et médias, intégration LiveKit.
7. **Passerelle push** : envoie des notifications **vides ou chiffrées** via APNs, FCM et UnifiedPush. Le téléphone se réveille, récupère et déchiffre localement.

Le serveur ne fait **jamais** ceci : posséder un compte, lire un message privé, modifier un événement signé.

## Identité et nommage

- **Identité** = clé publique (ed25519 ou secp256k1, voir les décisions ouvertes). Elle est permanente.
- **Nom affiché** : libre, non unique.
- **Handle** : `pseudo@instance`, unique *par instance*, attribué par l'annuaire de l'instance. Un domaine perso est possible en option, après vérification via `.well-known`.
- **Multi-appareil et récupération** : un nouvel appareil reçoit la clé d'un appareil déjà connecté (code court + SPAKE2), et l'utilisateur exporte sa clé (`nsec`) pour la garder en lieu sûr. Pas de coffre. Plus tard, éventuellement : identité avec clé de rotation (dans l'esprit de `did:plc`).
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
- Le chiffrement (NIP-17 pour les messages privés et les groupes ; MLS plus tard) est **uniquement dans `core/`**. Rien de tel côté Go.

## Arborescence prévue

```
.
├── CLAUDE.md
├── DESIGN.md                # design system de l'app (tokens, composants, règles)
├── spec/
│   ├── protocol.md          # formats d'événements, signature, versions
│   ├── federation.md        # .well-known, découverte, migration
│   └── test-vectors/        # JSON partagé Rust/Go
├── core/                    # crate Rust `chaton-core` (lib)
│   ├── src/{identity,event,dm,session,link,community,media,push}.rs
│   └── tests/{vectors,e2e}.rs  # vecteurs à l'octet près, bout en bout contre un serveur
├── app/                     # Tauri 2
│   ├── src/                 # UI web (TS)
│   └── src-tauri/           # commandes Tauri → core ; gen/android versionné (Kotlin du push)
└── server/                  # Go
    ├── cmd/server/
    └── internal/{relay,directory,nip98,push,community,media,federation}/
```

## Commandes

```bash
# app (Tauri 2 + Angular). Linux/WSL : un Secret Service avec un trousseau par défaut est requis.
# WSL n'a pas de session graphique qui crée et déverrouille le trousseau « login ». Dans un vrai terminal
# (pas via `!`), une seule instance du démon, mot de passe non vide ; à refaire après chaque redémarrage de WSL :
#   sudo apt install gnome-keyring   # une fois
#   pkill -u "$USER" -f gnome-keyring-daemon
#   read -rsp "Mot de passe : " PW && printf %s "$PW" | gnome-keyring-daemon --daemonize --unlock --components=secrets >/dev/null; unset PW
# Vérifier : busctl --user call org.freedesktop.secrets /org/freedesktop/secrets org.freedesktop.Secret.Service ReadAlias s default
#   → doit renvoyer …/collection/login
# Notifications sur WSL : pas de serveur de notifications. `sudo apt install dunst libnotify-bin`, puis `dunst &` dans un
# terminal à chaque démarrage de WSL (lancé par systemd, il ne voit pas l’écran et plante).
# L'app embarque LiveKit (voix) : clang requis pour compiler (Linux/WSL : `sudo apt install clang libclang-dev`).
# Voix en dev : `npm run dev:livekit` (Docker) dans un autre terminal, et server/.env (ignoré par git) avec les
# SERVER_LIVEKIT* listées plus bas ; le serveur lit ce fichier au démarrage.
# Windows : `.cargo/config.toml` de src-tauri met le CRT en statique (libwebrtc).
cd app && npm ci && npm run tauri dev
# Android (téléphone en débogage USB ou adb Wi-Fi ; le SDK et le NDK 28 dans ~/Android/Sdk). Le téléphone
# joint l'instance par l'adresse LAN du PC : WSL en réseau « mirrored » ou redirection de port.
# Notifications : installer ntfy (distributeur UnifiedPush) sur le téléphone.
#   export NDK_HOME=$ANDROID_HOME/ndk/28.2.13676358 ANDROID_NDK_HOME=$ANDROID_HOME/ndk/28.2.13676358
# (ANDROID_NDK_HOME : webrtc-sys, la voix. Son libwebrtc.jar est copié dans gen/android/app/libs, à mettre à jour avec lui.)
cd app && npm run tauri android dev
cd app && npm run tauri android build -- --apk --debug --target aarch64
# second compte sur la même machine (réutilise le serveur de dev de la première fenêtre)
cd app && CHATON_PROFILE=bob src-tauri/target/debug/chaton-app
cd app && npm run build && ./node_modules/.bin/eslint src

# server
cd server && go vet ./... && go test ./...
cd server && SERVER_PORT=:8080 SERVER_PUBLICURL=http://localhost:8080 go run ./cmd/server
# autres variables : SERVER_DBPATH (chaton.bolt, bbolt), SERVER_NAME, SERVER_BLOCKEDPUBKEYS (hex, séparées par des virgules),
# SERVER_RELAYKEY (hex, clé qui signe l'état des communautés ; sinon relay.key à côté de la base, à sauvegarder avec elle),
# SERVER_BLOBDIR (fichiers envoyés, Blossom ; sinon blobs/ à côté de la base, index dans blobs.bolt),
# SERVER_VAPIDKEY (base64url, signe les push ; sinon vapid.key à côté de la base, à sauvegarder avec elle),
# SERVER_PUSHALLOWPRIVATE=1 (dev seulement : push vers des adresses privées, requis par les tests e2e)
# SERVER_LIVEKITURL, SERVER_LIVEKITKEY, SERVER_LIVEKITSECRET (voix, les trois ou aucun ; en dev : ws://localhost:7880,
#   devkey, chaton-dev-secret-not-for-production, avec `npm run dev:livekit` à la racine, qui demande Docker)

# vecteurs de test (spec/test-vectors), à régénérer après tout changement de cas
cd server && go run ./cmd/genvectors > ../spec/test-vectors/events.json
```

```bash
# core
cd core && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
# bout en bout, serveur lancé sur :8099 (SERVER_PORT=:8099 SERVER_PUBLICURL=http://localhost:8099 SERVER_PUSHALLOWPRIVATE=1)
cd core && CHATON_TEST_SERVER=http://localhost:8099 cargo test --test e2e -- --ignored
# voix (feature `voice`, LiveKit compilé : clang requis, voir plus haut). LiveKit + serveur sur :8080 avec SERVER_LIVEKIT*
cd core && cargo clippy --all-targets --features voice -- -D warnings
cd core && CHATON_TEST_SERVER=http://localhost:8080 cargo test --features voice --test e2e voice -- --ignored
```

iOS (`npm run tauri ios dev`) attend un Mac.

## Conventions

- Rust : `cargo fmt`, `cargo clippy -- -D warnings`. Pas de `unwrap()` hors des tests.
- Go : `gofmt`, `go vet`. Erreurs enveloppées avec `%w`.
- UI : aucune logique métier ni crypto en TypeScript. Tout passe par les commandes Tauri vers `core`.
- UI : style selon [`DESIGN.md`](DESIGN.md). Toute valeur vient des tokens (`app/src/styles/_tokens.scss`) ; réutiliser `.btn`, `.list-row`, `.field`, `.badge` et `app-avatar` avant d'écrire du style.
- Secrets : jamais en clair sur disque. Utiliser Keychain, Keystore ou le trousseau de l'OS. `core` passe par `keyring-core` : l'app enregistre le store de sa plateforme au démarrage (`keyring_core::set_default_store`), les tests utilisent `keyring_core::mock`.
- Le code et les commentaires peuvent être en anglais. La doc du projet est en français.

## Feuille de route

Le détail par étape est dans [`ROADMAP.md`](ROADMAP.md).

1. **Spec v0** : format d'événement, signature, vecteurs de test.
2. **core** : génération et stockage des clés, signature/vérification, client relais WebSocket, base SQLite.
3. **server** : relais (stockage et diffusion d'événements signés), annuaire, `.well-known`.
4. **app** : inscription, connexion, recherche `@pseudo`, messages privés.
5. Multi-appareil : liaison d'appareils et export de la clé (pas de coffre).
6. Communautés : salons, rôles et permissions (événements signés), modération.
7. Chiffrement de bout en bout : messages privés, puis groupes (NIP-17 ; MLS plus tard).
8. Notifications push (plugin Tauri Swift/Kotlin + passerelle Go).
9. Voix et vidéo (LiveKit).
10. Fédération multi-instances, règles de blocage, migration de compte.
11. (Plus tard) P2P direct quand c'est possible (iroh/libp2p), avec les relais en secours.

## Décisions ouvertes

Tranchées le 2026-11-15 :

- [x] **Compatible Nostr** : on réutilise NIP-01, 05, 17, 29, 42, 44, 49, 51, 59, 65, 92 et 98, plus Blossom (BUD-01, 02) pour les fichiers. La version du protocole passe dans un tag `["v","0"]`.
- [x] Courbe : secp256k1 (Schnorr, BIP-340).
- [x] Sérialisation : celle de NIP-01 (`sha256([0,pubkey,created_at,kind,tags,content])`).
- [x] Framework UI web : Angular (existant), dans Tauri 2.
- [x] Desktop : Tauri uniquement. GPUI seulement si une raison concrète apparaît.
- [x] Attribution des handles : premier arrivé, premier servi, un handle par clé. Pas d'e-mail.
- [x] Écriture sur le relais : utilisateurs locaux + MP qui leur sont adressés.
- [x] **Groupes en NIP-17, MLS reporté** : Marmot (MLS sur Nostr) n'a pas de version publiée compatible (`mdk-core` 0.8 : nostr 0.44 et un openmls dépassé pour des raisons de sécurité). Groupes de 10 au plus, membres figés. On y reviendra quand Marmot sera stable.
- [x] **Pas de coffre de clés** : un mot de passe fuité donnerait la clé pour toujours (une clé ne se révoque pas), et l'opérateur pourrait attaquer hors ligne les mots de passe faibles. Multi-appareil par liaison d'appareils, sauvegarde par export de la clé. Perdre tous ses appareils sans export = perdre le compte.

Demande avant de trancher l'une de ces questions :

- [ ] Mode par défaut des instances : liste noire (proposé) ou liste blanche ?
