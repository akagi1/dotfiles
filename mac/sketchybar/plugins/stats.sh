#!/bin/sh
export PATH="/opt/homebrew/bin:/usr/local/bin:$PATH"
case "$1" in
  clock) value=$(date '+%a %d %b  %H:%M') ;;
  battery)
    value=$(pmset -g batt | awk '/%/ {for(i=1;i<=NF;i++) if ($i ~ /%/) {gsub(/;/,"",$i); print $i; exit}}')
    [ -n "$value" ] || value="N/A"
    if pmset -g batt | head -1 | /usr/bin/grep -q 'AC Power'; then value="$value +"; fi
    ;;
  cpu)
    value=$(top -l 2 -s 1 -n 0 | awk '/CPU usage/ {gsub(/%/,"",$7); idle=$7} END {printf "%.0f%%",100-idle}')
    ;;
  memory)
    # Physical pages excluding free, inactive and purgeable (an approximation).
    value=$(vm_stat | awk '
      NR==1 {size=$8; gsub(/[^0-9]/,"",size)}
      /Pages free:|Pages inactive:|Pages purgeable:/ {gsub(/\./,"",$NF); available+=$NF}
      END {printf "%.0f",available*size}')
    total=$(sysctl -n hw.memsize)
    value=$(awk -v a="$value" -v t="$total" 'BEGIN {printf "%.0f%%",100*(1-a/t)}')
    ;;
  network)
    value=$(route -n get default 2>/dev/null | awk '/interface:/ {print $2; exit}')
    value=${value:-offline}
    ;;
  *) exit 1 ;;
esac
sketchybar --set "$NAME" label="$value"
