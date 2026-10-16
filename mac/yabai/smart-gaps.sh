#!/bin/sh
export PATH="/opt/homebrew/bin:/usr/local/bin:$PATH"

spaces=$(yabai -m query --spaces) || exit 0
windows=$(yabai -m query --windows) || exit 0

# Update all BSP spaces so moving a window also fixes its previous space.
# Compare existing values to avoid resize signals causing an update loop.
printf '%s\n' "$spaces" | jq -r --argjson windows "$windows" '
  .[] | select(.type == "bsp") |
  . as $space |
  ([$windows[] | select(.space == $space.index and
    .["is-floating"] == false and .["is-minimized"] == false and
    .["is-hidden"] == false and .["is-native-fullscreen"] == false)] | length) as $count |
  "\($space.index) \(if $count > 1 then 5 else 0 end)"
' | while read -r space gap; do
    for setting in top_padding bottom_padding left_padding right_padding window_gap; do
        current=$(yabai -m config --space "$space" "$setting") || continue
        [ "$current" = "$gap" ] || yabai -m config --space "$space" "$setting" "$gap"
    done
done
