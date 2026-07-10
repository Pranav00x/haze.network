#!/usr/bin/env bash
# One-time VPS setup for the Haze testnet node + wallet + marketplace, all
# on one box (Docker for the node, nginx serving the two static frontends
# and reverse-proxying the node's API). Run as root on a fresh Ubuntu 22.04
# box, from the repo root: sudo ./deploy/setup.sh
#
# Idempotent-ish: safe to re-run after a `git pull` to rebuild/redeploy, but
# it will NOT overwrite /etc/haze/node.env once it exists (that's where the
# real secrets live - see below).
set -euo pipefail

if [[ $EUID -ne 0 ]]; then
  echo "Run this as root (sudo ./deploy/setup.sh)." >&2
  exit 1
fi

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

if [[ ! -f deploy/config.env ]]; then
  echo "deploy/config.env not found - copy deploy/config.env.example to deploy/config.env and fill in your real domains first." >&2
  exit 1
fi
# shellcheck disable=SC1091
source deploy/config.env

for var in NODE_DOMAIN WALLET_DOMAIN MARKETPLACE_DOMAIN CERTBOT_EMAIL; do
  if [[ -z "${!var:-}" ]]; then
    echo "deploy/config.env is missing $var." >&2
    exit 1
  fi
done

echo "==> Installing Docker, nginx, certbot"
apt-get update -qq
apt-get install -y -qq docker.io nginx certbot python3-certbot-nginx >/dev/null
systemctl enable --now docker >/dev/null

echo "==> Building the node Docker image"
docker build -t haze-node:latest .

echo "==> Preparing data directory"
mkdir -p /var/lib/haze-data

echo "==> Setting up secrets (/etc/haze/node.env)"
mkdir -p /etc/haze
if [[ -f /etc/haze/node.env ]]; then
  echo "    /etc/haze/node.env already exists - leaving it alone. Delete it first if you need to rotate secrets."
else
  echo "    Paste the treasury secret (from your secure storage, NOT typed elsewhere), then press enter:"
  read -rs TREASURY_SECRET
  echo
  echo "    Paste the genesis validator secret, then press enter:"
  read -rs VALIDATOR_SECRET
  echo
  umask 077
  cat > /etc/haze/node.env <<EOF
HAZE_TREASURY_BLINDING=${TREASURY_SECRET}
HAZE_GENESIS_VALIDATOR_BLINDING=${VALIDATOR_SECRET}
EOF
  chmod 600 /etc/haze/node.env
  unset TREASURY_SECRET VALIDATOR_SECRET
  echo "    Written, permissions locked to root-only (600)."
fi

echo "==> Installing systemd service"
cp deploy/haze-node.service /etc/systemd/system/haze-node.service
systemctl daemon-reload
systemctl enable haze-node
systemctl restart haze-node

echo "==> Deploying wallet static site"
mkdir -p /var/www/haze-wallet
rsync -a --delete haze-wallet-web/ /var/www/haze-wallet/

echo "==> Deploying marketplace static site"
mkdir -p /var/www/haze-marketplace
rsync -a --delete nft-marketplace/ /var/www/haze-marketplace/

echo "==> Installing nginx configs"
sed "s/{{NODE_DOMAIN}}/${NODE_DOMAIN}/g" deploy/nginx/node.conf.tmpl > /etc/nginx/sites-available/haze-node
sed -e "s/{{DOMAIN}}/${WALLET_DOMAIN}/g" -e "s#{{WEBROOT}}#/var/www/haze-wallet#g" deploy/nginx/static-site.conf.tmpl > /etc/nginx/sites-available/haze-wallet
sed -e "s/{{DOMAIN}}/${MARKETPLACE_DOMAIN}/g" -e "s#{{WEBROOT}}#/var/www/haze-marketplace#g" deploy/nginx/static-site.conf.tmpl > /etc/nginx/sites-available/haze-marketplace

ln -sf /etc/nginx/sites-available/haze-node /etc/nginx/sites-enabled/haze-node
ln -sf /etc/nginx/sites-available/haze-wallet /etc/nginx/sites-enabled/haze-wallet
ln -sf /etc/nginx/sites-available/haze-marketplace /etc/nginx/sites-enabled/haze-marketplace
rm -f /etc/nginx/sites-enabled/default

nginx -t
systemctl reload nginx

echo "==> Requesting TLS certificates (certbot)"
certbot --nginx --non-interactive --agree-tos -m "${CERTBOT_EMAIL}" \
  -d "${NODE_DOMAIN}" -d "${WALLET_DOMAIN}" -d "${MARKETPLACE_DOMAIN}"

echo
echo "==> Done."
echo "    Node:        https://${NODE_DOMAIN}  (also serves the built-in explorer at /)"
echo "    Wallet:      https://${WALLET_DOMAIN}"
echo "    Marketplace: https://${MARKETPLACE_DOMAIN}"
echo
echo "Check node status:  curl https://${NODE_DOMAIN}/v1/status"
echo "Check node logs:    docker logs -f haze-node"
echo
echo "Remaining manual step: the wallet/marketplace still default to"
echo "http://localhost:8332 until DEFAULT_API_BASE is updated to point at"
echo "https://${NODE_DOMAIN} - do that in the source and re-run this script,"
echo "or use each site's 'change node' UI in the meantime."
