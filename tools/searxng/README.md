# Local SearXNG for research

Research mode expects a SearXNG instance at `http://127.0.0.1:8080` with JSON
output enabled. The app never receives a search endpoint from the webview, so
this URL is fixed.

## Option 1: Docker (if installed)

```bash
cd tools/searxng
docker compose up -d
```

`docker-compose.yml` publishes the port on loopback only and mounts
`settings.yml`, which enables `json` in `search.formats` and disables the
Google CSE engine.

## Option 2: Native install inside WSL (no Docker)

Tested with WSL 2 + Ubuntu 26.04. This avoids `sudo` by using `uv` for a
Python 3.12 environment.

```bash
# inside WSL
git clone --depth 1 https://github.com/searxng/searxng.git ~/searxng
cd ~/searxng

# uv manages a private Python build; no apt/system Python changes needed
curl -LsSf https://astral.sh/uv/install.sh | sh
export PATH="$HOME/.local/bin:$PATH"
uv python install 3.12
uv venv --python 3.12 .venv
uv pip install --python .venv/bin/python -r requirements.txt
```

The launcher script copies `settings.yml` from this folder, generates a random
`secret_key`, and runs the development server on port 8080:

```bash
cp /mnt/c/path/to/repo/tools/searxng/settings.yml ~/searxng/settings.local.yml
# edit secret_key, or use the script below which fills one in

~/start-searxng.sh    # created for you during setup; see below
```

`start-searxng.sh` (installed at `~/start-searxng.sh`):

```bash
#!/usr/bin/env bash
set -euo pipefail
cd "$HOME/searxng"
export SEARXNG_SETTINGS_PATH="$HOME/searxng/settings.local.yml"
export SEARXNG_BASE_URL="http://127.0.0.1:8080/"
export SEARXNG_BIND_ADDRESS="0.0.0.0"
export SEARXNG_PORT="8080"
exec .venv/bin/python -m searx.webapp
```

Start it detached and verify from Windows:

```bash
nohup ~/start-searxng.sh > ~/searxng.log 2>&1 &
```

```powershell
Invoke-WebRequest "http://127.0.0.1:8080/search?q=earth&format=json" -UseBasicParsing
```

WSL 2 forwards `localhost:8080` to Windows, so the desktop app can reach it
without extra configuration. Restart the script after a reboot (`wsl --shutdown`
also stops it).

### Known engine behaviour on some networks

DuckDuckGo may answer with a CAPTCHA and Wikipedia may rate-limit a burst of
queries from one IP. SearXNG then suspends those engines for a while; results
from Brave, Yep, Mwmbl, and Wiby still arrive. The app needs any non-empty set
of results, not a specific engine.
