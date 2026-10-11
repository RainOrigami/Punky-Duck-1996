#!/usr/bin/env bash
# Run the original game in DOSBox-X.
#   run.sh [--headless] [--limit SECONDS] [--capture] [--keys "AUTOTYPE ARGS"] [DOS command, default DUCK.BAT]
# --capture records video+audio of the command into disc/dosbox/capture/.
# --keys scripts input through DOSBox-X AUTOTYPE, e.g. --keys "-w 15 -p 1 enter down enter"
#   (-w seconds before the first key, -p seconds between keys; AUTOTYPE -list shows key names).
set -euo pipefail
cd "$(dirname "$0")/../.."

headless=0 limit="" capture="" keys=""
while [[ ${1:-} == --* ]]; do
    case $1 in
        --headless) headless=1 ;;
        --limit) limit=$2; shift ;;
        --capture) capture="DX-CAPTURE /V /A " ;;
        --keys) keys=$2; shift ;;
        *) echo "unknown option $1" >&2; exit 2 ;;
    esac
    shift
done
cmd=${*:-DUCK.BAT}

[[ -d disc/dosbox/c/PUNKY ]] || { mkdir -p disc/dosbox/c; cp -r disc/extracted/PUNKY disc/dosbox/c/; }
mkdir -p disc/dosbox/capture

args=(-conf tools/dosbox/punkyduck.conf -nomenu -fastlaunch
      -c "mount c disc/dosbox/c"
      -c "imgmount d disc/PUNKYDUCK.cue -t cdrom"
      -c "c:" -c "cd \\PUNKY")
[[ -n $keys ]] && args+=(-c "AUTOTYPE $keys")
args+=(-c "${capture}${cmd}" -c "exit")
[[ -n $limit ]] && args+=(-time-limit "$limit")
if (( headless )); then
    SDL_VIDEODRIVER=dummy SDL_AUDIODRIVER=dummy exec dosbox-x "${args[@]}"
else
    exec dosbox-x "${args[@]}"
fi
