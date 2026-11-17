# Héberger une instance ChatON

Une instance, c'est le serveur ChatON derrière Caddy, qui s'occupe du HTTPS. La voix (LiveKit) est en option. Le tout tient sur un petit VPS ou sur un Raspberry Pi (l'image existe en amd64 et en arm64).

## Prérequis

- Une machine avec Docker et Docker Compose.
- Un domaine (par exemple `chaton.example.org`) dont le DNS pointe vers cette machine.
- Les ports **80** et **443** ouverts. Caddy obtient le certificat Let's Encrypt tout seul.

Le HTTPS n'est pas optionnel : les clients ne joignent une instance qu'en `https`/`wss`, et la vérification des pseudos (NIP-05) passe par `https://<domaine>/.well-known/nostr.json`.

## Démarrer

```bash
cp .env.example .env     # puis remplir DOMAIN (et SERVER_NAME, SERVER_COUNTRY si tu veux)
docker compose up -d
```

L'instance répond sur `https://<domaine>`. Dans l'app, à l'inscription, on tape cette adresse.

**Image :** `compose.yaml` utilise `erwanheschung/chaton-server:latest`. `CHATON_IMAGE` change l'image ; `CHATON_VERSION=0.1.0` fige la version, ce qui est conseillé.

## Voix (optionnel)

1. Dans `.env`, `LIVEKIT_SECRET` reçoit le résultat de `openssl rand -hex 32`.
2. Ouvrir **7881/tcp** et **7882/udp** : tous les appels passent par ces deux ports.
3. Lancer :

   ```bash
   docker compose --profile voice up -d
   ```

LiveKit répond sur le même domaine : Caddy lui envoie `/rtc` et `/twirp`. Il n'y a pas de second domaine à configurer.

## Mettre à jour

```bash
docker compose pull && docker compose up -d
```

## Sauvegarder

Toute l'instance est dans le volume `data`. Le sauvegarder régulièrement :

```bash
docker compose stop chaton
docker run --rm -v "$(basename "$PWD")_data:/data" -v "$PWD:/backup" busybox tar czf /backup/chaton-data.tgz -C /data .
docker compose start chaton
```

Ce qu'il contient :

- **`chaton.bolt`** : les événements, les pseudos, les communautés ;
- **`relay.key`** : la clé qui signe l'état des communautés. **La perdre**, c'est que les clients ne reconnaissent plus les communautés hébergées ici ;
- **`vapid.key`** : la clé des notifications push. La perdre, c'est que les téléphones doivent se réabonner ;
- **`blobs/`** : les fichiers envoyés, chiffrés ;
- **`rules.yaml`** : les règles de l'instance, si tu en as écrit.

Les messages privés sont chiffrés de bout en bout. Même avec la sauvegarde, l'admin ne peut pas les lire.

## Règles de l'instance

Le fichier `rules.yaml` se place dans le volume `data`. Il sert à bloquer des instances ou des clés et à régler les limites de débit (format : `spec/federation.md` § 6). Pour le recharger sans redémarrer :

```bash
docker compose kill -s HUP chaton
```

## Bon à savoir

- **Toujours derrière Caddy.** Le serveur lit l'IP du client dans `X-Forwarded-For`, que Caddy réécrit. Exposé directement sur Internet, un client pourrait y mettre n'importe quelle IP et contourner les limites de débit. Le serveur n'expose donc aucun port dans `compose.yaml`.
- **Les variables** `SERVER_*` du serveur sont listées dans `CLAUDE.md` (section Commandes). On peut en ajouter dans la section `environment` du service `chaton`.
- **Changer de domaine** casse les pseudos (`pseudo@ancien-domaine`). Les utilisateurs devront migrer leur compte (Réglages → Compte).
