#!/usr/bin/env bash
# The watcher in Telegram mode during Option T on testnet: alerts for one coin go to the builder's
# private chat. Start it in its own terminal before scripts/option-t.sh --network testnet. It waits
# until the coin exists (the run launches it), then watches it from the head at that moment on, so
# it sees every sell. Stop it with Ctrl-C after the run.
#
#   scripts/option-t-watcher.sh <coin account>     for example bag.factory.fairchance.testnet
#
# It loads TELEGRAM_BOT_TOKEN and FC_TELEGRAM_CHAT_ID from ~/.config/fair-chance/telegram.env
# (FC_TELEGRAM_ENV overrides the path) and never prints them. Reads use the public testnet RPC
# and, for events, public FastNEAR (FC_EVENT_SOURCE=block-scan reads blocks over RPC instead).
# The public /check stays off. It holds no NEAR key and signs nothing. Needs Go.
set -euo pipefail
cd "$(dirname "$0")/.."

coin="${1:-}"
case "$coin" in
  *.testnet) ;;
  *) echo "option-t-watcher: give the coin account, a .testnet account" >&2; exit 2 ;;
esac
rpc="https://rpc.testnet.near.org"
env_file="${FC_TELEGRAM_ENV:-$HOME/.config/fair-chance/telegram.env}"
[ -r "$env_file" ] || { echo "option-t-watcher: cannot read the Telegram env file" >&2; exit 1; }

(cd watcher && go build -o ../target/watcher ./cmd/watcher)

echo "option-t-watcher: waiting for $coin"
# Stops on a 403 or an unknown answer; backs off on 429 and 5xx with a limit on tries.
scripts/wait-for-coin.sh "$rpc" "$coin"
echo "option-t-watcher: alerts for $coin go to the private chat"

set -a
# shellcheck disable=SC1090
. "$env_file"
set +a
unset FC_PUBLIC_FACTORY FC_PAGE_URL
export FC_SENDER=telegram FC_RPC_URL="$rpc" FC_COINS="$coin" FC_EVENT_SOURCE="${FC_EVENT_SOURCE:-fastnear}"
exec target/watcher -poll 2s
