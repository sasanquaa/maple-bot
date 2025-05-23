- [Wrong map detection](#wrong-map-detection)
- [Actions contention](#actions-contention-)
- [Default Ratio game resolution](#default-ratio-game-resolution)
- [Preventing double jump(s)](#preventing-double-jumps)
- [Mage up jump](#mage-up-jump)
- [Installation](#installation)

## Wrong map detection
Wrong map detection can happen when:
- Moving quickly between different maps
- Other UIs overlapping
- The map is not expanded fully

Rule of thumb is:  Map detection will always try to crop the white border and make it is as tight as possbile. So before
creating a new map, double-check to see if the map being displayed has white border cropped and looks similar to the
one in the game.

Fix methods:
- Below the map are three buttons, two of which can be used to help troubleshooting:
    - `Re-detect map`: Use this button to re-detect the map
    - `Delete map`: Use this to **permanently delete** the map
- Move the map UI around
- When moving around different maps, it may detect previous map due to delay. Just use `Re-detect map` 
button for this case.

## Actions contention (?)
Action with `EveryMillis` can lead to contention if you do not space them out properly. For example, if there are two `EveryMillis` actions executed every 2 seconds, wait 1 second afterwards and one normal action, it is likely the normal action will never
get the chance to run to completion.

That said, it is quite rare.

## Default Ratio game resolution
Currently, the bot does not support `Default Ratio` game resolution because most detection resources are
in `Ideal Ratio` (1920x1080 with `Ideal Ratio` or 1376x768 below). `Default Ratio` currently only takes effect
when play in `1920x1080` or above, making the UI blurry.

## Preventing double jump(s)
**This is subject to change** but if you want to the bot to only walk between points then the two
points `x` distance should be less than `25`.

## Up jump key
- If you are a mage class:
  - If you have up jump, which most mage classes now have, you don't need to set this key
  - If you use teleport as up jump, set this key to same key as `Teleport key` 
- If you are Demon Slayer, set this key to up arrow
- If you are any other class with up jump skill such as Explorer Warriors, Blaster,... set this key to that skill

## Missing installation
If you use the bot on a newly installed Windows, make sure [Visual C++ Redistributable 2015-2022](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist#visual-studio-2015-2017-2019-and-2022) is installed.
