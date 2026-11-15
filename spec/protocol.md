# Protocole ChatON — v0

ChatON est un client **compatible Nostr**. Ce document ne redéfinit pas Nostr : il renvoie aux NIP (https://github.com/nostr-protocol/nips) et ne fixe que les choix propres à ChatON. En cas de doute, la NIP fait foi, sauf écart écrit ici.

Contrat entre implémentations : [`test-vectors/events.json`](test-vectors/events.json). `core` (Rust) et `server` (Go) doivent reproduire **à l'octet près** `serialized`, `id` et `sig` de chaque vecteur.

## 1. Événement

Format NIP-01, sans modification :

```json
{
  "id": "<sha256 hex de la sérialisation>",
  "pubkey": "<clé publique x-only, 32 octets hex>",
  "created_at": <secondes unix>,
  "kind": <entier>,
  "tags": [["nom", "valeur", ...], ...],
  "content": "<chaîne>",
  "sig": "<signature Schnorr BIP-340, 64 octets hex>"
}
```

- Courbe secp256k1, signatures **Schnorr BIP-340**. Tout l'hexadécimal est en minuscules.
- `sig` signe l'`id` (32 octets), pas le JSON.

### Sérialisation (calcul de l'`id`)

`id = sha256(UTF-8( [0,<pubkey>,<created_at>,<kind>,<tags>,<content>] ))`

- JSON **sans aucun espace** ni retour à la ligne.
- Dans les chaînes, échapper **uniquement** : `"` → `\"`, `\` → `\\`, saut de ligne → `\n`, retour chariot → `\r`, tabulation → `\t`, retour arrière (U+0008) → `\b`, saut de page (U+000C) → `\f`.
- Tout le reste est écrit **tel quel en UTF-8**, y compris les emoji, les accents, `/`, `<`, `>`, `&`, U+2028 et U+2029. Pas de `\uXXXX`.
- Les autres caractères de contrôle (U+0000–U+001F hors ceux listés) ne sont pas définis par NIP-01 et les implémentations divergent. **ChatON ne les produit jamais** : `core` les retire du contenu et des tags avant de signer.

Les vecteurs `escapes`, `unicode_raw`, `no_html_escape`, `empty_content_no_tags` et `tags_escaped` couvrent ces règles.

### Signature reproductible

Les vecteurs sont signés avec `aux_rand` = 32 octets nuls (BIP-340). En production, `core` utilise un `aux_rand` **aléatoire**. La valeur fixe ne sert qu'aux tests, pour que Rust (`sign_schnorr_with_aux_rand`) et Go (`schnorr.Sign` + `CustomNonce`) obtiennent les mêmes octets.

## 2. Version du protocole

- Tout événement produit par ChatON porte le tag `["v","0"]`.
- **Exceptions** : le sceau (13) et l'emballage (1059). NIP-59 impose des tags vides sur le sceau et seulement `p` sur l'emballage, et un tag en plus révélerait le client utilisé. La version est portée par le message intérieur (14).
- Un événement **sans** tag `v` est traité comme v0, pour accepter les autres clients Nostr.
- Un `v` inconnu : l'UI ne l'affiche pas, mais les relais le stockent et le relaient quand même.

## 3. Kinds utilisés en v0

| Kind | Usage | NIP | Signé ? | Étape |
|---|---|---|---|---|
| 0 | Profil (`name`, `display_name`, `picture`, `nip05`) | 01 | oui | 4 |
| 10002 | Liste de relais (outbox) | 65 | oui | 4 |
| 10050 | Relais préférés pour les MP | 17 | oui | 4 |
| 14 | Message privé (« rumeur ») | 17 | **non** | 4 |
| 13 | Sceau (contenu chiffré NIP-44) | 59 | oui | 4 |
| 1059 | Emballage (clé jetable) | 59 | oui | 4 |
| 22242 | Authentification au relais | 42 | oui | 3 |
| 27235 | Authentification HTTP (annuaire, coffre, envois) | 98 | oui | 3 |
| 9, 9000–9022, 39000–39003 | Communautés | 29 | oui | 6, *réservé, pas de vecteurs* |

**Kinds propres à ChatON** : autorisés seulement quand aucune NIP ne convient (par exemple la configuration d'un salon vocal). Ils sont **documentés ici avant tout code**, avec un vecteur. Les autres clients Nostr les ignorent.

## 4. NIP prises en charge

| NIP | Rôle | Étape |
|---|---|---|
| 01 | Événements, signature, protocole relais | 1 |
| 05 | `pseudo@instance` → clé publique | 3 |
| 11 | Document d'information du relais | 3 |
| 42 | Authentification au relais | 3 |
| 98 | Authentification HTTP | 3 |
| 17, 44, 59 | Messages privés chiffrés | 4 |
| 19 | Encodages `npub`, `nprofile`… (affichage seulement) | 4 |
| 65 | Liste de relais | 4 |
| 49 | Clé privée chiffrée par mot de passe (coffre) | 5 |
| 09, 25, 29 | Suppression, réactions, communautés | 6 |
| Blossom | Fichiers et médias | 6 |
| 51 | Listes (sourdine, blocage) | 10 |

Rejetées : **04** (ancien format de MP, qui fuit les métadonnées : remplacé par 17), **28** et **72** (remplacées par 29), **07** (les clés vivent dans `core`, pas dans une extension de navigateur).

## 5. Messages privés (NIP-17)

```
1059 emballage   signé par une clé jetable, chiffré clé jetable ↔ destinataire
 └─ 13 sceau     signé par l'auteur, chiffré auteur ↔ destinataire
     └─ 14 message   NON signé
```

- Chiffrement **NIP-44 v2** uniquement, fait **exclusivement dans `core`**. Les vecteurs officiels de NIP-44 (`nip44.vectors.json` dans le dépôt des NIP) font partie des tests de `core`. On ne les recopie pas ici.
- Le serveur ne voit que l'emballage : le destinataire (`p`), une clé d'expéditeur jetable, un horodatage légèrement aléatoire et un contenu chiffré.

### Pourquoi le message (14) n'est pas signé

Le sceau, signé par l'auteur, prouve au destinataire qui a écrit. Le message intérieur n'a pas de signature, donc **le message seul ne prouve rien** : une copie du texte déchiffré, une capture ou une base locale volée sans les sceaux ne peut pas être attribuée à l'auteur.

**Ce n'est qu'un déni partiel, pas celui de Signal.** Un destinataire qui conserve le sceau et révèle la clé de conversation NIP-44 peut prouver à un tiers que l'auteur a bien envoyé ce contenu. Le sceau signé couvre le texte chiffré, et le MAC empêche de le faire déchiffrer en autre chose. Pour un déni complet, il faudrait des clés éphémères (type Signal ou MLS) : on y reviendra à l'étape 7.

Conséquence : un kind 14 **portant une `sig` est rejeté** (vecteur `signed_rumor`), car une rumeur signée serait une preuve autonome.

### Limite connue : pas de confidentialité persistante

NIP-17/44 utilisent les clés à long terme. **Si une clé privée fuit, tous les MP passés stockés par les relais deviennent déchiffrables.** On l'accepte pour la v0. MLS (étape 7) apporte la confidentialité persistante pour les groupes, et on réévaluera alors les MP.

## 6. Validation à la réception

Dans cet ordre, et tout échec rejette l'événement :

1. JSON bien formé, champs aux bons types, hexadécimal en minuscules et de la bonne longueur.
2. `kind == 14` et `sig` présente → rejet (`signed_rumor`).
3. `id` recalculé ≠ `id` reçu → rejet (`bad_id`).
4. Signature invalide pour `pubkey` → rejet (`bad_sig`). Ne s'applique pas à un kind 14, qui n'est validé qu'à l'intérieur d'un sceau valide.
5. `created_at` plus de **15 minutes** dans le futur → rejet. Constante à ajuster.

Pour un kind 14 extrait d'un sceau : son `pubkey` doit être **égal** à celui du sceau, sinon c'est une usurpation et le message est rejeté.

## 7. Régénérer les vecteurs

```bash
cd server && go run ./cmd/genvectors > ../spec/test-vectors/events.json
go test ./cmd/genvectors/   # vérifie le fichier commité et le vecteur officiel BIP-340 n°0
```

Ajouter un kind = ajouter un cas dans `server/cmd/genvectors/main.go`, régénérer, commiter le JSON.
