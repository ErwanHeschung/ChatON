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
- U+0008 et U+000C font partie de la règle, mais certaines bibliothèques ne les échappent pas (`fiatjaf.com/nostr`, celle du serveur, au commit épinglé) et calculent alors un autre `id`. `core` les retire donc aussi, et le relais ChatON **refuse** tout événement qui en contient. Les vecteurs restent conformes à NIP-01 : `genvectors` a son propre sérialiseur.

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
| 27235 | Authentification HTTP (annuaire, envois) | 98 | oui | 3 |
| **24135** | **Liaison d'appareils** (éphémère, propre à ChatON, § 8) | — | oui (clé jetable) | 5 |
| 9 | Message dans un salon (`h` = salon) | 29 | oui | 6 |
| 9000, 9001, 9002, 9005, 9007, 9008, 9009 | Modération : rôles, retrait, métadonnées, suppression, création, invitation (§ 9) | 29 | oui | 6 |
| 9021, 9022 | Demande pour rejoindre, quitter | 29 | oui | 6 |
| 39000–39003 | État d'un groupe : métadonnées, admins, membres, rôles. **Signés par le relais** | 29 | oui (relais) | 6 |
| 10009 | Mes communautés (`group`) | 51 | oui | 6 |

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
| 49 | Clé privée chiffrée par mot de passe (export chiffré, plus tard) | — |
| 29 | Communautés et salons (§ 9) | 6 |
| 51 | Listes : mes communautés (`10009`) ; sourdine et blocage plus tard | 6, 10 |
| 09, 25 | Suppression d'un message privé, réactions | plus tard |
| Blossom | Fichiers et médias | 6b |

Rejetées : **04** (ancien format de MP, qui fuit les métadonnées : remplacé par 17), **28** et **72** (remplacées par 29), **07** (les clés vivent dans `core`, pas dans une extension de navigateur).

## 5. Messages privés (NIP-17)

```
1059 emballage   signé par une clé jetable, chiffré clé jetable ↔ destinataire
 └─ 13 sceau     signé par l'auteur, chiffré auteur ↔ destinataire
     └─ 14 message   NON signé
```

- Chiffrement **NIP-44 v2** uniquement, fait **exclusivement dans `core`**. `core` s'appuie sur `rust-nostr`, dont la suite de tests passe déjà les vecteurs officiels de NIP-44 (`nip44.vectors.json`). `core` teste l'aller-retour complet d'un MP et le rejet d'un sceau falsifié (`core/src/dm.rs`).
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

## 8. Liaison d'appareils (kind 24135)

Il n'y a **pas de coffre** : le serveur ne détient jamais la clé privée, même chiffrée (voir `CLAUDE.md`, décisions). Un nouvel appareil **B** reçoit la clé d'un appareil **A** déjà connecté, à travers le relais, protégée par un code court et un PAKE. Aucune NIP ne couvre ce besoin, d'où un kind propre à ChatON.

### Le code

- 10 caractères en base32 de Crockford (`0123456789ABCDEFGHJKMNPQRSTVWXYZ`), affichés `XXXX-XXX-XXX`. Les 4 premiers forment le **canal** (20 bits), les 6 derniers le **secret** (30 bits).
- Saisie tolérante : majuscules, tirets et espaces ignorés, `I`/`L` → `1`, `O` → `0`.
- Généré par A, valable **5 minutes** et pour **une seule tentative** : après un échange PAKE, réussi ou non, il est brûlé.

### Échange

Tous les messages sont des événements **éphémères** de kind `24135` (le relais ne les stocke pas), signés par une **clé jetable** propre à chaque côté, avec les tags `["v","0"]` et `["c", <canal>]`, où `<canal> = hex(sha256("chaton-link-v0|" + handle + "|" + canal du code))`. Le contenu est un objet JSON `{"type": …}`.

| # | De | `type` | Champ | Contenu |
|---|---|---|---|---|
| 1 | B | `pake` | `msg` | message SPAKE2 de B, base64 |
| 2 | A | `pake` | `msg` | message SPAKE2 de A, base64 |
| 3 | B | `hello` | `data` | nom de l'appareil, chiffré avec `K` |
| 4 | A | `key` | `data` | clé privée (`nsec`), chiffrée avec `K`, après confirmation de l'utilisateur sur A |
| 4 | A | `refused` | — | l'utilisateur a refusé |
| 4 | A | `failed` | — | `hello` indéchiffrable : mauvais code |
| 5 | B | `done` | `data` | `"ok"`, chiffré avec `K` |

- **SPAKE2** sur Ed25519 (crate RustCrypto `spake2`) : mot de passe = le secret du code (6 caractères), identités `"chaton-link-v0|A|" + handle` et `"chaton-link-v0|B|" + handle`. La sortie (32 octets) est la clé `K`.
- **Chiffrement** : NIP-44 v2, avec `K` utilisée directement comme clé de conversation.
- **Ordre** : un événement éphémère n'est livré qu'aux abonnés présents. A s'abonne au canal dès qu'il affiche le code ; B s'abonne **avant** de publier, puis parle en premier. A répond au premier message sans action de l'utilisateur : si B n'a pas de réponse en 10 secondes, aucun appareil n'affiche ce code (expiré ou faux). L'abonnement de B compte comme un auditeur, le relais ne signale donc pas de diffusion « dans le vide ».
- A n'accepte que la **première** clé jetable de B qui se présente ; chaque côté ignore ses propres événements.
- B vérifie que la clé reçue correspond à la clé publique que l'annuaire associe au handle (NIP-05) avant de l'enregistrer.

### Ce que ça protège

- Le relais (et quiconque enregistre le trafic) ne voit que des messages SPAKE2 et des données chiffrées. Il **ne peut pas tester des codes hors ligne** : un mauvais code ne donne qu'une tentative en ligne, et le code est ensuite brûlé.
- Un code volé (par-dessus l'épaule) ne suffit pas : **A demande confirmation** en affichant le nom de l'appareil qui se lie.
- Limite : si toutes les sessions d'un utilisateur sont perdues sans **export de la clé**, le compte est perdu. L'interface pousse cet export juste après l'inscription.

## 9. Communautés (NIP-29)

Une **communauté** (un « serveur » façon Discord) est un groupe NIP-29 hébergé par le relais d'une instance. Ses **salons** sont des sous-groupes : chacun porte `parent` = la communauté, et la communauté liste ses salons dans l'ordre (`child`). Le relais applique les permissions et publie l'état de chaque groupe, signé avec **sa propre clé** (NIP-11 `self`).

### Identifiants et état

- Identifiant d'un groupe : 12 caractères `[a-z0-9]`, tirés au hasard par le client qui le crée. Adresse NIP-29 : `<hôte>'<id>`.
- Une communauté ChatON est toujours `private` (seuls les membres lisent), `closed` (on entre avec un code d'invitation) et `restricted` (seuls les membres écrivent). Le relais impose ces trois drapeaux, quoi que dise un `9002`.
- Les membres et les rôles sont ceux de la **communauté** ; ses salons en héritent. Pas de permissions par salon en v0.
- L'état publié par le relais : `39000` (`d`, `name`, `picture`, drapeaux, `parent` ou `child`), `39001` (`p` + rôles), `39002` (`p` des membres), `39003` (`role`). Un client n'accepte ces événements que signés par la clé `self` du relais qui héberge le groupe.

### Rôles et permissions

| Action | `admin` | `moderator` | membre |
|---|---|---|---|
| Écrire un message (`9`) | oui | oui | oui |
| Supprimer un message (`9005`) | oui | oui | le sien |
| Créer une invitation (`9009`) | oui | oui | non |
| Retirer un membre (`9001`) | oui | sauf un admin | non |
| Donner un rôle (`9000`) | oui | non | non |
| Renommer, gérer les salons (`9002`, `9007`, `9008`) | oui | non | non |
| Quitter (`9022`) | oui | oui | oui |

Le créateur d'une communauté en est `admin`. Les rôles personnalisés viendront plus tard.

### Créer une communauté et ses salons

1. `9007` `["h", C]` : seule une clé qui a un handle sur l'instance peut créer une communauté. Le relais en fait l'`admin` en publiant lui-même un `9000` `["h", C], ["p", créateur, "admin"]`.
2. `9002` `["h", C], ["name", …]` (et `picture`).
3. Pour chaque salon S : `9007` `["h", S]`, puis `9002` `["h", S], ["name", …], ["parent", C]` (l'auteur doit être admin de C), puis `9002` sur C avec **toute** la liste `child`.

Un `9002` remplace **toutes** les métadonnées du groupe (NIP-29) : le client renvoie toujours chaque tag (`name`, `picture`, `parent`, tous les `child`).

### Invitations

- Un admin ou un modérateur publie `9009` `["h", C], ["code", X]`, X = 8 caractères en base32 de Crockford. Le code est réutilisable et n'expire pas en v0.
- Texte partagé : `<hôte>'<C>/<X>`, par exemple `chaton.example'k3j9x2m8q0aa/AB12CD34`. Le relais est `wss://<hôte>` (`ws://` pour `localhost`).
- Pour entrer : `9021` `["h", C], ["code", X]`. Si le code est bon, le relais ajoute la clé en publiant un `9000` `["h", C], ["p", clé]`. Un membre retiré (`9001`) a besoin d'une nouvelle invitation (pas de liste de bannis en v0).
- Après être entré, le client ajoute `["group", C, "<url du relais>"]` à sa liste `10009`, que tous ses appareils lisent.

### Messages

- `9` `["h", S]` + `["v","0"]`, contenu en clair, signé par l'auteur. Les communautés ne sont **pas** chiffrées de bout en bout (étape 7 pour les groupes privés).
- Suppression : `9005` `["h", S], ["e", id]`. Le relais efface l'événement visé et diffuse le `9005` ; un client masque tout message visé par un `9005` accepté.

