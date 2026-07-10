# VPS deployment (Contabo, Ubuntu 22.04)

Everything on one box: the node runs in Docker under systemd, nginx serves
the wallet and marketplace static sites and reverse-proxies the node's API,
certbot handles TLS for all three domains.

## First-time setup

1. Point all three domains' DNS `A` records at the VPS's IP before running
   anything - certbot needs them resolving correctly to issue certificates.
2. On the VPS, clone this repo and `cd` into it.
3. `cp deploy/config.env.example deploy/config.env` and fill in your real
   domains (this file is gitignored - never commit it).
4. Have the treasury secret and genesis validator secret ready (from your
   secure storage - see `testnet_secrets_DO_NOT_COMMIT.txt` from earlier,
   move those into real storage and delete that file if you haven't yet).
5. `sudo ./deploy/setup.sh` - installs Docker/nginx/certbot, builds the
   node image, prompts once for the two secrets (never echoed, never
   passed as a CLI arg, written straight to a root-only `/etc/haze/node.env`
   file), sets up the systemd service, deploys both static sites, installs
   nginx configs, and requests TLS certs.

Takes a few minutes (mostly the Docker build). At the end it prints the
three live HTTPS URLs.

## Redeploying after a code change

```
git pull
sudo ./deploy/setup.sh
```

Safe to re-run - it won't touch `/etc/haze/node.env` if it already exists
(so your secrets survive), rebuilds the Docker image, redeploys the static
sites, and reloads nginx.

## Day-to-day operations

```
docker logs -f haze-node          # tail the node's output
systemctl status haze-node        # is it running
systemctl restart haze-node       # restart without rebuilding
curl https://<node-domain>/v1/status
```

Chain data lives in `/var/lib/haze-data` on the host (bind-mounted into the
container) - it survives container restarts and rebuilds. Deleting that
directory resets the node to genesis.

## Known follow-up

`DEFAULT_API_BASE` in `haze-wallet-web/index.html` and
`nft-marketplace/index.html` still points at `http://localhost:8332` in
the committed source - update both to `https://<your node domain>` once
you know it, then re-run `setup.sh` (or just `rsync` the two directories
again) to pick up the change. Until then, each site's "change node" UI
lets a user manually point at the right one, but new visitors won't get it
by default.
