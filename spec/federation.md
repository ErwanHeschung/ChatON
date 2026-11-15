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

Tout le reste est refusé : autres kinds, `14` publié nu, `created_at` à plus de 15 min dans le futur, clés bloquées (`SERVER_BLOCKEDPUBKEYS`). Un gift wrap n'est **servi qu'à son destinataire** : tout filtre qui peut correspondre à un `1059` (`kinds` vide ou contenant `1059`) exige une authentification NIP-42 (sinon `CLOSED` « `auth-required:` », et le client s'authentifie puis réessaie) et un `#p` égal à la seule clé authentifiée (sinon « `restricted:` »). Comme la règle porte sur le filtre et non sur les résultats, elle couvre aussi la diffusion en direct. Le relais n'expose pas de `COUNT`.

Communautés : les `39000`–`39003` ne sont acceptés que du relais lui-même, et restent lisibles par tous (un invité voit le nom avant d'entrer). Un filtre qui peut correspondre à un message ou une action de groupe (`9`, `9000`–`9022`, ou un `#h`) exige une authentification NIP-42 et un `#h` dont **chaque** groupe est une communauté (ou un salon d'une communauté) dont la clé authentifiée est membre. La diffusion en direct est vérifiée à chaque événement : un membre retiré ne reçoit plus rien sur ses abonnements déjà ouverts.

### Clé du relais

Le relais signe l'état des groupes avec sa propre clé, publiée dans son document NIP-11 (`self`). Elle vient de `SERVER_RELAYKEY` (hexadécimal) ; sinon le serveur en génère une au premier démarrage, dans `relay.key` (droits `0600`) à côté de la base. La perdre oblige les clients à faire confiance à une nouvelle clé : la sauvegarder avec la base.

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
