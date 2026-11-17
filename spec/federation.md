# Fédération ChatON — v0

Comment une instance publie ses utilisateurs, comment un client trouve quelqu'un sur une autre instance, et comment un compte déménage. Tout repose sur NIP-05 et NIP-65. Le format des événements est dans [`protocol.md`](protocol.md).

## 1. Handle et découverte (NIP-05)

Un handle s'écrit `pseudo@instance`. Il est unique **par instance**, et l'UI affiche **toujours** l'instance, pour qu'on ne confonde jamais `erwan@a.fr` et `erwan@b.fr`.

```
GET https://<instance>/.well-known/nostr.json?name=<pseudo>
```

```json
{
  "names":  { "erwan": "<pubkey hex>" },
  "relays": { "<pubkey hex>": ["wss://relay.<instance>"] }
}
```

- `pseudo` : `a-z0-9-_.`, en minuscules, 3 à 32 caractères. La recherche est insensible à la casse.
- `_@instance` est réservé à la clé de l'instance elle-même.
- Réponse avec `Access-Control-Allow-Origin: *` (exigé par NIP-05).
- Le client vérifie que le `pubkey` renvoyé est bien celui de l'événement ou du profil affiché. Un handle n'est **qu'un pointeur** vers une clé : l'identité, c'est la clé.

## 2. Réserver un handle

```
POST https://<instance>/api/handle
Authorization: Nostr <base64(événement kind 27235)>
Content-Type: application/json

{"name":"erwan"}
```

- L'événement NIP-98 porte les tags `u` (URL exacte), `method` (`POST`), `payload` (sha256 hex du corps) et `v`. Vecteur : `http_auth`.
- Le serveur vérifie l'événement (signature, `created_at` à ±60 s, `u`, `method`, `payload`), puis associe `name` à la `pubkey` de l'événement.
- Réponses : `201` réservé, `409` pseudo déjà pris **ou clé ayant déjà un handle**, `400` pseudo invalide, `401` authentification invalide.
- Politique : **premier arrivé, premier servi**, un seul handle par clé. Pas d'e-mail ni de coffre (§ 5).
- Un pseudo inconnu renvoie `200` avec `{"names":{},"relays":{}}`.
- Pas encore d'abandon ni de transfert de handle : il faudra une requête signée de libération.

## 3. Relais d'un utilisateur (NIP-65 et NIP-17)

- `10002` : relais où l'utilisateur **publie** (`write`) et **lit** (`read`). Sans marqueur, le relais sert aux deux.
- `10050` : relais où il **reçoit ses MP**.
- Modèle « outbox » : pour lire quelqu'un, le client lit sur **ses** relais d'écriture. Pour lui envoyer un MP, il publie sur **ses** relais `10050`. Les serveurs n'ont pas à se parler entre eux.

### Politique d'écriture du relais

Le relais d'une instance n'est pas un dépôt ouvert. Il accepte :

- de la part d'une clé **qui a un handle ici** : son profil (`0`) et ses listes de relais (`10002`, `10050`) ;
- de la part d'une clé **qui a un handle ici** : sa liste de communautés (`10009`) et la création d'une communauté (`9007` sans parent) ;
- de la part de **n'importe qui**, y compris d'une autre instance : un gift wrap (`1059`) dont le tag `p` désigne un utilisateur local, et les événements de communauté (`9`, `9000`–`9022`) que les permissions de `protocol.md` § 9 autorisent : un membre de n'importe quelle instance peut écrire.

Le kind éphémère `24135` (liaison d'appareils) est accepté de **n'importe quelle clé**, puisque le nouvel appareil n'a pas encore de compte. Il n'est jamais stocké, doit porter exactement un tag `c`, et son contenu est limité à 4 Kio. Un filtre qui le demande doit donner exactement un `#c` : on ne peut pas écouter toutes les liaisons en cours.

Tout le reste est refusé : autres kinds, `14` ou `15` publié nu, `created_at` à plus de 15 min dans le futur, clés bloquées (`SERVER_BLOCKEDPUBKEYS`). Un gift wrap n'est **servi qu'à son destinataire** : tout filtre qui peut correspondre à un `1059` (`kinds` vide ou contenant `1059`) exige une authentification NIP-42 (sinon `CLOSED` « `auth-required:` », et le client s'authentifie puis réessaie) et un `#p` égal à la seule clé authentifiée (sinon « `restricted:` »). Comme la règle porte sur le filtre et non sur les résultats, elle couvre aussi la diffusion en direct. Le relais n'expose pas de `COUNT`.

Communautés : les `39000`–`39003` ne sont acceptés que du relais lui-même, et restent lisibles par tous (un invité voit le nom avant d'entrer). Un filtre qui peut correspondre à un message ou une action de groupe (`9`, `9000`–`9022`, ou un `#h`) exige une authentification NIP-42 et un `#h` dont **chaque** groupe est une communauté (ou un salon d'une communauté) dont la clé authentifiée est membre. La diffusion en direct est vérifiée à chaque événement : un membre retiré ne reçoit plus rien sur ses abonnements déjà ouverts.

### Clé du relais

Le relais signe l'état des groupes avec sa propre clé, publiée dans son document NIP-11 (`self`). Elle vient de `SERVER_RELAYKEY` (hexadécimal) ; sinon le serveur en génère une au premier démarrage, dans `relay.key` (droits `0600`) à côté de la base. La perdre oblige les clients à faire confiance à une nouvelle clé : la sauvegarder avec la base.

### Fichiers (Blossom)

L'instance sert aussi les fichiers (`protocol.md` § 10), sur le même port que le relais mais hors du relais : `PUT /upload`, `GET|HEAD /<sha256>`, `DELETE /<sha256>`, `GET /list/<clé>`.

- **Envoi** : réservé aux clés qui ont un handle ici, 25 Mio au plus. Un membre d'une communauté venu d'une autre instance envoie ses fichiers sur la sienne.
- **Lecture** : par empreinte, sans authentification. Les pièces jointes sont chiffrées par le client ; seules les images de profil et de communauté sont en clair, et elles sont publiques.
- **Liste** : seulement la sienne. **Suppression** : par celui qui a envoyé.
- `/mirror` (le serveur irait chercher une URL distante), `/media` et `/report` sont désactivés.
- Stockage : un fichier par blob dans `SERVER_BLOBDIR` (par défaut `blobs/` à côté de la base), et l'index « qui a envoyé quoi » dans `blobs.bolt`, séparé des événements du relais.

### Notifications push (WebPush)

L'instance réveille les téléphones de ses utilisateurs quand un MP leur arrive. Le push ne contient rien d'utile : le téléphone affiche une notification générique, puis récupère et déchiffre les messages quand on ouvre l'app. Sur Android, le push passe par **UnifiedPush** : l'utilisateur installe un distributeur (ntfy, par exemple) qui donne une adresse WebPush à l'app.

- **Clé VAPID** (RFC 8292, P-256) : `SERVER_VAPIDKEY` (privée, base64url), sinon `vapid.key` (droits `0600`) créé à côté de la base, à sauvegarder avec elle. `GET /api/push` renvoie `{"vapid": "<clé publique, base64url>"}`.
- **Inscription** : `POST /api/push` et `DELETE /api/push`, signés en NIP-98 par une clé qui a un handle ici. Corps : `{"endpoint", "p256dh", "auth"}` (clés WebPush en base64url). L'adresse doit être en `https`, faire 1000 octets au plus et pointer vers une IP publique. 10 adresses au plus par clé : la plus ancienne part.
- **Quand** : un gift wrap (`1059`) enregistré pour un `p` local qui a des adresses. **Sauf** si la connexion qui l'a publié est authentifiée (NIP-42) avec ce même `p` : c'est la copie à soi-même d'un de mes appareils.
- **Quoi** : un WebPush chiffré (RFC 8291, `aes128gcm`) au contenu constant `{"v":0}`, avec `Topic: chaton` (le service ne garde que le dernier), `Urgency: high` et `TTL` d'un jour. Un push par adresse toutes les 10 secondes au plus. Une réponse 404 ou 410 supprime l'adresse.
- **Vie privée** : le service de push ne voit qu'un message chiffré et son heure, jamais l'auteur ni le contenu. L'instance savait déjà qui reçoit un gift wrap.
- Le serveur refuse les adresses qui mènent à une IP non publique, à chaque envoi (pas de SSRF). Pour le développement seulement, `SERVER_PUSHALLOWPRIVATE=1` lève cette règle.

### Voix (LiveKit)

L'instance peut faire tourner un serveur LiveKit à côté d'elle (`protocol.md` § 11). Sans configuration, la voix est coupée.

- **Configuration** : `SERVER_LIVEKITURL` (l'adresse `wss://` que les clients joignent), `SERVER_LIVEKITKEY` et `SERVER_LIVEKITSECRET` (la paire de clés d'API du fichier de LiveKit, secret de 32 caractères au moins, jamais journalisé). Les trois, ou aucun.
- **Routes** : `GET /.well-known/nip29/livekit` (voix activée ?), `POST /.well-known/nip29/livekit/<salon>` (jeton d'un salon vocal), `POST /api/voice/call` (jeton d'un appel en MP), et `POST /api/voice/webhook`, que seul LiveKit appelle (signé par son JWT, avec l'empreinte du corps).
- **Présence** : les webhooks de LiveKit (`participant_joined`, `participant_left`, `room_finished`) tiennent à jour la liste des participants, que le relais publie en `39004`. Au démarrage, l'instance la relit chez LiveKit.
- **Éjection** : quand un membre est retiré ou part, l'instance le sort des salons vocaux de la communauté (API de LiveKit).
- **Vie privée** : pour un salon vocal, LiveKit relaie un audio en clair, comme les messages de la communauté. Pour un appel en MP, l'audio est chiffré par les clients ; l'instance apprend qui appelle qui, pas ce qui se dit.
- **Déploiement** : LiveKit (binaire ou image Docker, arm64 compris) à côté du serveur. Le proxy (Caddy) sert `wss://` vers le port 7880 ; ouvrir TCP 7881 et UDP 7882 (`rtc.use_external_ip: true`). Le webhook de LiveKit pointe vers `http://<serveur>:8080/api/voice/webhook`. Pour que l'anneau « parle » suive la voix, régler `audio: { update_interval: 100, smooth_intervals: 1 }` (par défaut 400 ms et 2 échantillons, soit jusqu'à 0,8 s de retard ; voir `server/livekit.dev.yaml`). Un TURN/TLS pour les réseaux stricts viendra plus tard.

## 4. Migration de compte

L'utilisateur garde sa clé et change d'instance :

1. Il réserve un handle sur la nouvelle instance (§ 2).
2. Il publie de nouveaux `10002` et `10050` pointant vers les nouveaux relais, **sur les anciens et les nouveaux relais**, pour que ses contacts voient le changement.
3. Il republie son profil (`0`) avec le nouveau `nip05`.
4. Optionnel : l'ancienne instance garde le `.well-known` en redirigeant vers les nouveaux relais.

Rien d'autre ne change : ses contacts le suivent par sa clé, et le handle n'est qu'un affichage.

## 5. Multi-appareil, sans coffre

Le serveur ne stocke **aucune** clé privée, même chiffrée. Un nouvel appareil reçoit la clé d'un appareil déjà connecté par la liaison d'appareils (`protocol.md` § 8), à travers le relais. La sauvegarde, c'est l'export de la clé par l'utilisateur. Raison : avec un coffre protégé par mot de passe, un mot de passe fuité donne la clé pour toujours (une clé ne se révoque pas), et l'opérateur peut attaquer hors ligne les mots de passe faibles.

L'historique d'un nouvel appareil vient du relais : le client pagine (`until`) ses gift wraps, car le relais limite chaque requête à 100 événements.

## 6. Règles d'instance (étape 10)

Le blocage d'instances et de clés, le mode liste noire ou liste blanche et les limites de débit arrivent à l'étape 10 (`rules.yaml`). Pour l'instant, seules des clés peuvent être bloquées, via `SERVER_BLOCKEDPUBKEYS`. Le mode par défaut (liste noire proposé) reste à confirmer, voir `CLAUDE.md`.
