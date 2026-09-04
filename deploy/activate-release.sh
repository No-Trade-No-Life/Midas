#!/bin/sh
set -eu
: "${MIDAS_DEPLOY_SHA:?MIDAS_DEPLOY_SHA is required}"
: "${MIDAS_DEPLOY_DIR:=/opt/midas}"
: "${MIDAS_DATA_DIR:=/var/lib/midas}"
: "${MIDAS_SERVICE_NAME:=midas}"
: "${MIDAS_ARCHIVE_NAME:=midas-linux-x86_64.tar.gz}"
: "${MIDAS_METADATA_NAME:=midas-linux-x86_64.metadata.json}"
release_base="https://github.com/No-Trade-No-Life/Midas/releases/download/latest"
release_dir="$MIDAS_DEPLOY_DIR/releases/$MIDAS_DEPLOY_SHA"

DEBIAN_FRONTEND=noninteractive apt-get install -y caddy curl ca-certificates
if ! id -u midas >/dev/null 2>&1; then
  useradd --system --home-dir "$MIDAS_DATA_DIR" --shell /usr/sbin/nologin midas
fi
mkdir -p "$MIDAS_DEPLOY_DIR/releases" "$MIDAS_DATA_DIR"
chown midas:midas "$MIDAS_DATA_DIR"
chmod 700 "$MIDAS_DATA_DIR"
curl -fsSL "$release_base/$MIDAS_ARCHIVE_NAME" -o "/tmp/$MIDAS_ARCHIVE_NAME"
curl -fsSL "$release_base/$MIDAS_ARCHIVE_NAME.sha256" -o "/tmp/$MIDAS_ARCHIVE_NAME.sha256"
curl -fsSL "$release_base/$MIDAS_METADATA_NAME" -o "/tmp/$MIDAS_METADATA_NAME"
(cd /tmp && sha256sum -c "$MIDAS_ARCHIVE_NAME.sha256")
test "$(python3 -c "import json; print(json.load(open('/tmp/$MIDAS_METADATA_NAME'))['git_sha'])")" = "$MIDAS_DEPLOY_SHA"
rm -rf "$release_dir"
mkdir -p "$release_dir"
tar -xzf "/tmp/$MIDAS_ARCHIVE_NAME" -C "$release_dir" --strip-components=1
ln -sfn "$release_dir" "$MIDAS_DEPLOY_DIR/current"
cat > "/etc/systemd/system/$MIDAS_SERVICE_NAME.service" <<'UNIT'
[Unit]
Description=Midas payment infrastructure
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=midas
Group=midas
WorkingDirectory=/opt/midas/current
ExecStart=/opt/midas/current/midas
Restart=always
RestartSec=5
NoNewPrivileges=true
PrivateTmp=true
ProtectHome=true
ProtectSystem=strict
ReadWritePaths=/var/lib/midas

[Install]
WantedBy=multi-user.target
UNIT
cat > /etc/caddy/Caddyfile <<'CADDY'
midas.ntnl.io {
  encode zstd gzip
  reverse_proxy 127.0.0.1:8787
}
CADDY
systemctl daemon-reload
systemctl enable --now "$MIDAS_SERVICE_NAME"
systemctl enable --now caddy
systemctl restart caddy
for _ in $(seq 1 30); do
  version="$(curl -fsS --max-time 2 http://127.0.0.1:8787/api/version || true)"
  if [ -n "$version" ] && VERSION="$version" EXPECTED="$MIDAS_DEPLOY_SHA" python3 -c 'import json, os; assert json.loads(os.environ["VERSION"])["git_sha"] == os.environ["EXPECTED"]'; then
    curl -fsS --max-time 2 http://127.0.0.1:8787/api/health
    rm -f "/tmp/$MIDAS_ARCHIVE_NAME" "/tmp/$MIDAS_ARCHIVE_NAME.sha256" "/tmp/$MIDAS_METADATA_NAME"
    exit 0
  fi
  sleep 1
done
exit 1
