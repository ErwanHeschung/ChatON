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
- Réponses : `201` réservé, `409` déjà pris, `400` pseudo invalide, `401` authentification invalide.
- **Ouvert, à trancher à l'étape 3** : la politique d'attribution (premier arrivé, premier servi, ou vérification par e-mail), et l'abandon ou le transfert d'un handle.

## 3. Relais d'un utilisateur (NIP-65 et NIP-17)

- `10002` : relais où l'utilisateur **publie** (`write`) et **lit** (`read`). Sans marqueur, le relais sert aux deux.
- `10050` : relais où il **reçoit ses MP**.
- Modèle « outbox » : pour lire quelqu'un, le client lit sur **ses** relais d'écriture. Pour lui envoyer un MP, il publie sur **ses** relais `10050`. Les serveurs n'ont pas à se parler entre eux.

## 4. Migration de compte

L'utilisateur garde sa clé et change d'instance :

1. Il réserve un handle sur la nouvelle instance (§ 2).
2. Il publie de nouveaux `10002` et `10050` pointant vers les nouveaux relais, **sur les anciens et les nouveaux relais**, pour que ses contacts voient le changement.
3. Il republie son profil (`0`) avec le nouveau `nip05`.
4. Optionnel : l'ancienne instance garde le `.well-known` en redirigeant vers les nouveaux relais.

Rien d'autre ne change : ses contacts le suivent par sa clé, et le handle n'est qu'un affichage.

## 5. Coffre (étape 5)

La clé privée est stockée **chiffrée par le mot de passe** de l'utilisateur au format NIP-49 (`ncryptsec`), et récupérée via un appel authentifié NIP-98. Le serveur ne voit jamais le mot de passe ni la clé en clair. Détails à l'étape 5.

## 6. Règles d'instance (étape 10)

Le blocage d'instances et de clés, le mode liste noire ou liste blanche et les limites de débit sont décrits à l'étape 3 (`rules.yaml`) puis complétés à l'étape 10. Le mode par défaut (liste noire proposé) reste à confirmer, voir `CLAUDE.md`.
