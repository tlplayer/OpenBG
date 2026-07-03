#!/usr/bin/env bash
set -euo pipefail

# Manual acceptance launcher for the current M2/M3 Candlekeep slice.
# Usage: ./scripts/validate-area-viewer.sh [game-root] [area-resref]
# The game root may instead be supplied through OPENBG_GAME.

game_root="${1:-${OPENBG_GAME:-}}"
area="${2:-AR2600}"

if [[ -z "${game_root}" ]]; then
    echo "usage: $0 <game-root> [area-resref]" >&2
    echo "       or set OPENBG_GAME" >&2
    exit 2
fi

if [[ ! -f "${game_root}/chitin.key" ]]; then
    echo "error: ${game_root}/chitin.key was not found" >&2
    exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

echo "Validating ${area}: animated ARE backgrounds, pathfinding, selection, fog, and regions"
echo "Visual controls: left-click move; right-click NPC talk; Esc close talk; F fog; R regions"
echo "Detailed checklist: ${repo_root}/VALIDATION.md"

exec cargo run --offline -p openbg-area -- "${game_root}" "${area}"
