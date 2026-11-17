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
| 14 | Message privé (« rumeur »), y compris l'invitation à un appel (`call`, `call-end`, § 11) | 17 | **non** | 4, 9 |
| 15 | Fichier en message privé (« rumeur », § 10) | 17 | **non** | 6b |
| 13 | Sceau (contenu chiffré NIP-44) | 59 | oui | 4 |
| 1059 | Emballage (clé jetable) | 59 | oui | 4 |
| 22242 | Authentification au relais | 42 | oui | 3 |
| 27235 | Authentification HTTP (annuaire, push, jetons de voix) | 98 | oui | 3 |
| **24135** | **Liaison d'appareils** (éphémère, propre à ChatON, § 8) | — | oui (clé jetable) | 5 |
| 9 | Message dans un salon (`h` = salon) | 29 | oui | 6 |
| 9000, 9001, 9002, 9005, 9007, 9008, 9009 | Modération : rôles, retrait, métadonnées, suppression, création, invitation (§ 9) | 29 | oui | 6 |
| 9021, 9022 | Demande pour rejoindre, quitter | 29 | oui | 6 |
| 39000–39003 | État d'un groupe : métadonnées, admins, membres, rôles. **Signés par le relais** | 29 | oui (relais) | 6 |
| 39004 | Participants d'un salon vocal (`participant`, § 11). **Signé par le relais** | 29 | oui (relais) | 9 |
| 10009 | Mes communautés (`group`) | 51 | oui | 6 |
| 24242 | Autorisation Blossom (envoi d'un fichier, § 10) | Blossom BUD-01 | oui | 6b |

**Kinds propres à ChatON** : autorisés seulement quand aucune NIP ne convient (aujourd'hui, seule la liaison d'appareils). Ils sont **documentés ici avant tout code**, avec un vecteur. Les autres clients Nostr les ignorent.

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
| 92 | Pièce jointe dans un message (`imeta`, § 10) | 6b |
| Blossom (BUD-01, 02) | Fichiers et médias (§ 10) | 6b |
| 29 (salons « livekit ») | Salons vocaux et leurs participants (§ 11) | 9 |

Rejetées : **04** (ancien format de MP, qui fuit les métadonnées : remplacé par 17), **28** et **72** (remplacées par 29), **07** (les clés vivent dans `core`, pas dans une extension de navigateur).

## 5. Messages privés (NIP-17)

```
1059 emballage   signé par une clé jetable, chiffré clé jetable ↔ destinataire
 └─ 13 sceau     signé par l'auteur, chiffré auteur ↔ destinataire
     └─ 14 message   NON signé
```

- Chiffrement **NIP-44 v2** uniquement, fait **exclusivement dans `core`**. `core` s'appuie sur `rust-nostr`, dont la suite de tests passe déjà les vecteurs officiels de NIP-44 (`nip44.vectors.json`). `core` teste l'aller-retour complet d'un MP et le rejet d'un sceau falsifié (`core/src/dm.rs`).
- Le serveur ne voit que l'emballage : le destinataire (`p`), une clé d'expéditeur jetable, un horodatage légèrement aléatoire et un contenu chiffré.

### Groupes

Une conversation de groupe est un MP NIP-17 à plusieurs destinataires, comme les groupes de MP de Discord. Pas de MLS pour l'instant (voir plus bas).

- **Salon** : l'ensemble {auteur} ∪ les `p` d'un kind 14 ou 15. On l'identifie par la liste triée des clés de ses membres, la même pour tous. Un MP à deux est le salon [moi, lui].
- **Taille** : **10 membres au plus**, auteur compris. `core` refuse d'envoyer au-delà et ignore un message reçu pour un salon plus grand.
- **Envoi** : un emballage par membre, vers ses relais, plus un vers moi pour mes autres appareils. La rumeur, donc son id, est la même pour tous.
- **Nom** : tag `subject` de NIP-17, 100 caractères au plus. Le dernier `subject` vu dans le salon l'emporte. Nommer ou renommer envoie un kind 14 au contenu vide avec un `subject` (vecteur `group_dm_rumor`). Créer un groupe exige un nom et envoie cet événement, pour que les membres voient le groupe tout de suite. Un contenu vide n'est accepté qu'avec un `subject` ou un fichier.
- **Membres** : la liste ne change jamais. Ajouter quelqu'un crée un nouveau salon, à l'historique vide. Personne ne peut être retiré.
- Le coût grandit avec le nombre de membres (un emballage chacun), d'où la limite.

### Pourquoi le message (14) n'est pas signé

Le sceau, signé par l'auteur, prouve au destinataire qui a écrit. Le message intérieur n'a pas de signature, donc **le message seul ne prouve rien** : une copie du texte déchiffré, une capture ou une base locale volée sans les sceaux ne peut pas être attribuée à l'auteur.

**Ce n'est qu'un déni partiel, pas celui de Signal.** Un destinataire qui conserve le sceau et révèle la clé de conversation NIP-44 peut prouver à un tiers que l'auteur a bien envoyé ce contenu. Le sceau signé couvre le texte chiffré, et le MAC empêche de le faire déchiffrer en autre chose. Pour un déni complet, il faudrait des clés éphémères (type Signal ou MLS) : MLS est reporté (voir ci-dessous).

Conséquence : un kind 14 (ou 15) **portant une `sig` est rejeté** (vecteur `signed_rumor`), car une rumeur signée serait une preuve autonome.

### Limite connue : pas de confidentialité persistante

NIP-17/44 utilisent les clés à long terme. **Si une clé privée fuit, tous les MP et groupes passés stockés par les relais deviennent déchiffrables.** On l'accepte pour la v0.

MLS (via Marmot) apporterait la confidentialité persistante et des membres modifiables. Il est **reporté** (2026-11-15) : la crate publiée `mdk-core` 0.8 dépend de nostr 0.44 et d'openmls 0.8.1, dont les correctifs de sécurité sont postérieurs, et la version courante de MDK n'est pas publiée et son spec reste un brouillon. On y reviendra quand Marmot sera publié et stable.

## 6. Validation à la réception

Dans cet ordre, et tout échec rejette l'événement :

1. JSON bien formé, champs aux bons types, hexadécimal en minuscules et de la bonne longueur.
2. `kind` 14 ou 15 et `sig` présente → rejet (`signed_rumor`).
3. `id` recalculé ≠ `id` reçu → rejet (`bad_id`).
4. Signature invalide pour `pubkey` → rejet (`bad_sig`). Ne s'applique pas à un kind 14 ou 15, qui n'est validé qu'à l'intérieur d'un sceau valide.
5. `created_at` plus de **15 minutes** dans le futur → rejet. Constante à ajuster.

Pour un kind 14 ou 15 extrait d'un sceau : son `pubkey` doit être **égal** à celui du sceau, sinon c'est une usurpation et le message est rejeté.

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

## 10. Fichiers et médias (Blossom)

Les fichiers ne passent pas par le relais. Chaque instance expose un serveur **Blossom** sur le même port (BUD-01, BUD-02) ; un message ne transporte que l'adresse du fichier et, pour une pièce jointe, sa clé.

### Envoi

- Toujours vers **sa propre instance** : `PUT https://<instance>/upload`, avec une autorisation `24242` signée (`["t","upload"]`, `["x", sha256]`, `["expiration", …]`). Seule une clé qui a un handle sur l'instance peut envoyer.
- Taille maximale : **25 Mio** par fichier. Le serveur refuse un `Content-Length` absent ou trop grand avant de lire le corps.
- Le fichier est ensuite servi par son empreinte : `GET https://<instance>/<sha256>`, sans authentification. `/mirror`, `/media` et `/report` sont désactivés.

### Pièces jointes : toujours chiffrées

- **AES-256-GCM**, une clé de 32 octets et un nonce de 12 octets tirés au hasard **par fichier**, en hexadécimal. Le chiffrement se fait dans `core` ; le serveur ne stocke que du texte chiffré.
- `x` = sha256 du fichier **chiffré** (son adresse Blossom), `ox` = sha256 du fichier **en clair**. Le client vérifie `x` avant de déchiffrer et `ox` après.
- Comme le blob est illisible sans le message, il reste lisible par empreinte sans contrôle d'accès.

**En message privé** : une rumeur NIP-17 de kind **15**, scellée et emballée comme un kind 14 (§ 5). Contenu = URL du fichier. Tags : un `p` par destinataire (§ 5, groupes), `file-type` (type MIME), `encryption-algorithm` = `aes-gcm`, `decryption-key`, `decryption-nonce`, `x`, `ox`, `size` (octets, en clair), et `name` (nom d'origine, ajout ChatON).

**Dans un salon** : un `9` (`["h", S]`) avec un tag NIP-92 `imeta` ; le contenu est une légende facultative.

```
["imeta", "url https://a.fr/<x>", "m image/png", "x <x>", "ox <ox>", "size 1234",
          "name chat.png", "encryption-algorithm aes-gcm",
          "decryption-key <64 hex>", "decryption-nonce <24 hex>"]
```

### Images de communauté et de profil

- En clair (elles sont publiques, comme les `39000` et les kind 0) : `picture` dans le `9002` de la communauté, `picture` dans le profil (kind 0).
- Affichée seulement si l'URL est sur **sa propre instance** (l'hôte du relais de la communauté, le domaine NIP-05 de l'utilisateur), de la forme `https://<hôte>/<sha256>[.ext]`, et si les octets reçus ont cette empreinte.

### Téléchargement

- `core` télécharge (25 Mio au plus, empreinte vérifiée) et l'interface n'affiche que des octets rendus par `core` : la vue web ne charge jamais une URL distante.
- Automatique seulement depuis **mon instance** et **l'instance de la communauté** ; ailleurs, il faut cliquer. Un expéditeur ne peut donc pas apprendre l'adresse IP de ses destinataires avec son propre serveur.
- Le cache local garde le fichier **chiffré** ; il est déchiffré à la lecture.

## 11. Voix (LiveKit)

L'audio ne passe ni par le relais ni par Nostr. Chaque instance qui active la voix a, à côté d'elle, un serveur **LiveKit** (un SFU : il relaie les flux sans les mélanger). Nostr ne sert qu'à dire **où** parler et **qui** est là ; l'instance délivre les **jetons** LiveKit, après une authentification NIP-98. Voix seule en v0 : ni vidéo ni partage d'écran.

### Salons vocaux

- Un **salon vocal** est un salon (§ 9) dont le `9002` porte aussi `["livekit"]` (drapeau NIP-29), donc son `39000` aussi. Il se crée comme un salon texte (§ 9, étape 3). Son type est fixé à la création : le client renvoie `livekit` dans chaque `9002`, comme les autres tags (vecteur `voice_channel_edit_metadata`).
- Pas de messages texte dans un salon vocal en v0 (l'interface n'en propose pas ; le relais ne l'interdit pas).
- **Instance compatible** : `GET https://<instance>/.well-known/nip29/livekit` répond `204` si la voix est activée, `404` sinon.
- **Jeton** : `POST https://<instance>/.well-known/nip29/livekit/<salon>`, signé en NIP-98 (`u` = cette URL exacte, `method` = `POST`, `payload` = sha256 du corps), corps `{}`. Réponse `201` `{"server_url": "wss://…", "participant_token": "<JWT>"}`. `401` si l'authentification échoue, `403` si la clé n'est pas membre de la communauté, `404` si le groupe est inconnu, n'est pas un salon vocal, ou si la voix est coupée.
- Le jeton est un JWT LiveKit (HS256, signé avec le secret d'API de l'instance) : `iss` = clé d'API, `sub` = `<clé publique hex>:<8 hex au hasard>` (les 64 premiers caractères sont la clé ; le suffixe permet plusieurs appareils), `nbf` et `exp` (10 minutes ; LiveKit le renouvelle pendant la session), et `video` = `{"room": <salon>, "roomJoin": true, "canPublish": true, "canSubscribe": true, "canPublishData": false, "canPublishSources": ["microphone"]}`.
- **Présence** : le relais publie un `39004` signé par sa clé, `["d", <salon>]` puis un `["participant", <clé>]` par clé présente (plusieurs appareils comptent une fois) (vecteur `group_voice_participants`). Il le met à jour à chaque entrée et sortie, et il est lisible comme les `39000`–`39003`. Un client ne l'accepte que signé par la clé `self` du relais. On voit donc qui est dans un salon sans le rejoindre.
- Un membre retiré (`9001`) ou parti (`9022`) est éjecté des salons vocaux de la communauté.
- La voix des communautés **n'est pas chiffrée de bout en bout**, comme leurs messages : le serveur LiveKit de l'instance relaie un audio qu'il pourrait écouter.

### Appels en message privé (chiffrés de bout en bout)

Un MP ou un groupe (§ 5) peut lancer un appel. Le serveur LiveKit relaie l'audio **chiffré par les clients** (LiveKit E2EE, AES-GCM) avec une clé qu'il ne voit jamais.

- **Invitation** : une rumeur **kind 14** envoyée comme un MP, à tous les membres du salon (§ 5), contenu `📞 Appel vocal` (ce que montrent les autres clients NIP-17 ; ChatON l'ignore), avec le tag `["call", <id>, <url>, <clé>]` (vecteur `call_rumor`) :
  - `id` : 16 octets au hasard, en hexadécimal ;
  - `url` : l'adresse `https` de **l'instance de l'appelant**, qui délivre les jetons ;
  - `clé` : 32 octets au hasard, en hexadécimal, la clé de chiffrement de l'audio.
- **Fin** : une rumeur kind 14 `["call-end", <id>]`, contenu `Appel terminé` (vecteur `call_end_rumor`). Dans un appel à deux, envoyée par celui qui raccroche le premier (ou par l'appelant sans réponse) ; l'autre, en la recevant, quitte l'appel sans en renvoyer. Dans un groupe, envoyée par le dernier participant qui part, ou par l'appelant sans réponse. Les deux côtés ont ainsi le début (l'invitation) et la fin de l'appel dans la conversation.
- **Sonnerie** : une invitation venue de quelqu'un d'autre, dont le `created_at` a moins de **60 s**, sonne jusqu'à ce que je rejoigne, refuse, reçoive son `call-end` ou que 60 s passent. Rejoindre demande toujours un geste de l'utilisateur.
- Le client refuse une invitation dont `url` n'est pas l'instance de l'appelant (domaine de son NIP-05), pour la même raison qu'au § 10 : un expéditeur ne doit pas pouvoir faire contacter son propre serveur.
- **Jeton** : `POST <url>/api/voice/call`, signé en NIP-98, corps `{"id": <id>, "members": [<clé hex>…]}` (tout le salon, moi compris). L'instance vérifie que la clé signataire est dans `members`, qu'il y a de 2 à 10 clés distinctes, et qu'**au moins une a un handle** chez elle. La salle LiveKit est `call-` + hex(sha256(`"chaton-call-v0|" + id + "|" + clés triées jointes par ","`)) : la même pour tous, sans rien stocker. Réponse au même format que pour un salon vocal.
- **Chiffrement** : les 32 octets de `clé` sont la clé partagée de LiveKit E2EE (AES-GCM, options par défaut). **La clé ne voyage que dans la rumeur NIP-17** : elle n'est jamais dans la demande de jeton, ni nulle part ailleurs côté serveur.
- **Ce que l'instance apprend** : qui appelle qui (`members`), quand et depuis quelles adresses IP. Jamais le contenu. Les participants ne voient pas les adresses IP les uns des autres (tout passe par le SFU).
- Un appelé sur une autre instance utilise l'instance de l'appelant (`url`) : la règle « au moins un membre local » le couvre (étape 10).

