- [Features](#features)
- [How to use](#how-to-use)
  - [Download](#download)
  - [Map](#map)
  - [Configuration](#configuration)
  - [Action](#action)
  - [Condition](#condition)
  - [Linked Key & Linked Action](#linked-key--linked-action)
  - [Rotation Modes](#rotation-modes)
  - [Platforms Pathing](#platforms-pathing)
  - [Capture Modes](#capture-modes)
  - [Remote Control](#remote-control)
- [Troubleshooting](#troubleshooting)
  - [Wrong map detection](#wrong-map-detection)
  - [Actions contention](#actions-contention-)
  - [Default Ratio game resolution](#default-ratio-game-resolution)
  - [Preventing double jump(s)](#preventing-double-jumps)
  - [Mage up jump](#mage-up-jump)
  - [Installation](#installation)
- [Contact](#contact)
- [Video guides](#video-guides)
- [Showcase](#showcase)
  - [Rotation](#rotation)
  - [Auto Mobbing & Platforms Pathing](#auto-mobbing-%26-platforms-pathing)
  - [Rune Solving](#rune-solving)

## Features
- Rotation maker by through creating preset and actions
- Support for combo-oriented classes
- Auto-mobbing mode for hitting random mobs
- Auto-buff for common farming-related buffs
- Auto-HP potion
- Solve rune (from v0.8 spinning rune can now be solved)
- Platforms pathing for auto-mobbing and rune solving
- Usable UI
- Work by taking an image and send key inputs
- Not a feature but currently work best in GMS:
  - Updating PNGs are required to support non-English regions
  - Work with KMS/CMS/TMS/MSEA
  - Solving rune in non-GMS region works fine but the bot cannot solve other anti-bot detections

## How to use

#### Download
- Head to the [Github Release](https://github.com/sasanquaa/maple-bot/releases)
- Download the `app.zip` and extract it
- Run the exe file

#### Map
- Map is automatically detected but must be created manually by providing a name
- The created map is saved and can be selected again later
- Any actions preset created in the detected map is saved to that map only
- **Map detection can be wrong**, see [Troubleshooting](#troubleshooting)

The arcs are only for visual and do not represent the actual moving path. However, it does represent
the order of one action to another depending on rotation mode.

![Map](https://github.com/sasanquaa/maple-bot/blob/master/.github/images/map.png?raw=true)

#### Configuration
- Configuration is used to change key bindings, set up buffs,...
- Configuration can be created for use with different character(s) through preset
- Configuration is saved globally and not affected by the detected map

For supported buffs in the configuration, the bot relies on detecting buffs on the top-right corner. And the bot
movement depends heavily on the skill `Rope Lift` to move around platforms, so make sure you set a key for it.

![Rope Lift](https://github.com/sasanquaa/maple-bot/blob/master/.github/images/ropelift.png?raw=true)

![Configuration](https://github.com/sasanquaa/maple-bot/blob/master/.github/images/configuration.png?raw=true)

![Buffs](https://github.com/sasanquaa/maple-bot/blob/master/.github/images/buffs.png?raw=true)

#### Action
There are two types of action:
- `Move` - Moves to a location on the map
- `Key` - Uses a key with or without location

An action is further categorized into two:
- A normal action is an action with condition set to `Any`
- A priority action is any `ErdaShowerOffCooldown`/`EveryMillis` action

A priority action can override a normal action and force the player to perform the former. The
normal action is not completely overriden and is only delayed until the priority action is complete.

Action `Move` configurations:
- `Type`: `Move`
- `Position`: The required position to move to 
- `Adjust position`: Whether the actual position should be as close as possible to the specified position 
- `Condition`: See [below](#condition)
- `Wait after move`: The milliseconds to wait after moving (e.g. for looting)

Action `Key` configurations:
- `Type`: `Key`
- `Position`: Optionally add a position to use the key 
- `Count`: Number of times to use the key
- `Key`: The key to use
- `Has link key`: Optionally enable link key (useful for [combo classes](#linked-key--linked-action))
- `Condition`: See [below](#condition)
- `Queue to front`:
  - Applicable only to `EveryMillis` and `ErdaShowerOffCooldown` conditions
  - When set, this action can override other non-`Queue to front` priority action
  - The overriden priority action is not lost but delayed like normal action
  - Useful for action such as `press attack after x milliseconds even while moving`
  - Cannot override linked action
- `Direction`: The direction to use the key
- `With`:
  - `Stationary` - Performs an action only when standing on ground (for buffs)
  - `DoubleJump` - Performs an action with double jump
- `Wait before action`/`Wait after action`:
  - Wait for the specified amount of millseconds after/before using the key
  - Waiting is applied on each repeat of `Count`
- `Wait before random range`/`Wait after random range`: Applies randomization to the delay in the range `delay - range` to `delay + range`

Actions added in the list below can be dragged/dropped/reordered.

![Actions](https://github.com/sasanquaa/maple-bot/blob/master/.github/images/actions.png?raw=true)

#### Condition
There are four types of condition:
- `Any` - Does not do anything special and affected by rotation mode 
- `ErdaShowerOffCooldown` - Runs an action only when Erda Shower is off-cooldown
- `EveryMillis` - Runs an action every `x` milliseconds
- `Linked` - Runs an action chained to the previous action (e.g. like a combo) 

For `ErdaShowerOffCooldown` condition to work, the skill Erda Shower must be assigned to
the quick slots, with Action Customization toggled on and **visible** on screen. The skill
should also be casted when using this condition or the actions will be re-run.

![Erda Shower](https://github.com/sasanquaa/maple-bot/blob/master/.github/images/erda.png?raw=true)

#### Linked Key & Linked Action
Linked key and linked action are useful for combo-oriented class such as Blaster, Cadena, Ark, Mercedes,...
Animation cancel timing is specific to each class. As such, the timing is approximated and provided in the configuration, so make sure you select the appropriate one.

For linked key, there are four link types:
- `Before` - Uses the link key before the actual key (e.g. for Cadena, Chain Arts: Thrash is the link key)
- `AtTheSame` - Uses the link key at the same time as the actual key (probably only Blaster skating needs this)
- `After` - Uses the link key after the actual key (e.g. for Blaster, Weaving/Bobbing is the link key)
- `Along` - Uses the link key along with the actual key while the link key is being held down (e.g. for in-game Combo key)

Note that even though `AtTheSame` would send two keys simultaneously, *the link key will be send first*. When the configured
class is set to Blaster, the performing action has `After` link type and the link key is not `Jump Key`, an extra `Jump Key` will be sent for cancelling Bobbing/Weaving. The same effect can also be achieved through linked action.

As for `Along` link type, the timing is fixed and does not affected by class type.

Linked action is for linking action(s) into a chain. Linked action can be created by adding a `Linked` condition action below any `Any`/`ErdaShowerOffCooldown`/`EveryMillis`/`Linked` action. The first non-`Linked` action is the start of the actions chain:

```
Any Linked Linked Linked   EveryMillis Linked Linked
 ▲                    ▲     ▲                    ▲  
 │                    │     │                    │  
 │                    │     │                    │  
 └────────────────────┘     └────────────────────┘  
          Chain                      Chain          
```

Linked action cannot be overriden by any other type of actions once it has started executing regardless of whether the action is a normal or priority action.

(This feature is quite niche though...)

#### Rotation Modes
Rotation mode specifies how to run the actions and affects **only** `Any` condition actions. There are three modes:
- `StartToEnd` - Runs actions from start to end in the order added and repeats
- `StartToEndThenReverse` - Runs actions from start to end in the order added and reverses (end to start)
- `AutoMobbing` - All added actions are ignored and, instead, detects a random mob within bounds to hit

For other conditions actions:
- `EveryMillis` actions run out of order
- `ErdaShowerOffCooldown` actions run in the order added same as `StartToEnd`

When `AutoMobbing` is used:
- Setting the bounds to inside the minimap is required so that the bot will not wrongly detect out of bounds mobs
- The bounds should be the rectangle where you can move around (two edges of the map)
- While this mode ignores all `Any` condition actions, it is still possible to use other conditions
- For platforms pathing, see [Platforms Pathing](#platforms-pathing)
- From v0.8.0, `AutoMobbing` behavior has been improved and will now try to utilize platforms as pathing points if provided
  - Pathing point is to help `AutoMobbing` moves to area with more mobs to detect

![Auto Mobbing](https://github.com/sasanquaa/maple-bot/blob/master/.github/images/automobbing.png?raw=true)

#### Platforms Pathing
Platforms pathing is currently only supported for Auto Mobbing and Rune Solving. This feature exists to help
pathing around platforms with or without `Rope Lift` skill. To use this feature, add all the map's platforms starting
from the ground level.

Without this feature, the bot movement is quite simple. It just moves horizontally first so the `x` matches the destination
and then try to up jump, rope lift or drop down as appropriate to match the `y`.

Hot keys can be used to add platforms more quickly.

![Platforms](https://github.com/sasanquaa/maple-bot/blob/master/.github/images/platforms.png?raw=true)

#### Capture Modes
There are three capture modes, the first two are similar to what you see in OBS:
- `BitBlt` - The default capture mode that works for GMS
- `Windows Graphics Capture` - The alternative capture mode for Windows 10 that works for TMS/MSEA
- `BitBltArea` - Captures a fixed area on the screen
  - This capture mode is useful if you are running the game inside something else or want to use fixed capture area (e.g. a VM, capture card (?) or Sunshine/Moonlight)
  - The capture area can stay behind the game but it cannot be minimized
  - As of v0.6.0, limited support for remote control has been added see [Remote Control](#remote-control)
  - **When the game resizes (e.g. going to cash shop), the capture area must still contain the game**
  - **When using this capture mode, key inputs will also be affected:**
    - **Make sure the window on top of the capture area is focused by clicking it for key inputs to work**
    - For example, if you have Notepad on top of the game and focused, it will send input to the Notepad instead of the game

If you want to customize input method, the currently supported method is by using gRPC:
  - Use the language of your choice to write, host it and provide the server URL to the bot
  - Check this [example](https://github.com/sasanquaa/maple-bot/tree/master/examples/python)

#### Remote Control
v0.6.0 add some supports for remote control. It is currently tested against Sunshine/Moonlight with the following setup:
1. Both host (Sunshine) and client (Moonlight) native resolutions are 1980x1080
2. Client connects in borderless windowed, 1080p, 10Mbps bitrate and 30 FPS
3. Host matches client resolution upon connection using https://github.com/Nonary/ResolutionAutomation
4. Use BitBltArea and drag over to the client game
5. Tested against some dark/bright maps and using skills with fancy effects

Another Sunshine/Moonlight setup tested using Windowed mode:
1. Have the Moonlight settings to 1366x768, FPS of your choice but increase the Video bitrate around 20Mbps so the images can be clearer
2. When connecting to the Sunshine host, make sure you don't resize the Moonlight window but only drag it around
3. Set your MapleStory game in Sunshine host to 1366x768 full screen

There has been confirmation about working well when GeForce NOW.

## Troubleshooting
#### Wrong map detection
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

#### Actions contention (?)
Action with `EveryMillis` can lead to contention if you do not space them out properly. For example, if there are two `EveryMillis` actions executed every 2 seconds, wait 1 second afterwards and one normal action, it is likely the normal action will never
get the chance to run to completion.

That said, it is quite rare.

#### Default Ratio game resolution
Currently, the bot does not support `Default Ratio` game resolution because most detection resources are
in `Ideal Ratio` (1920x1080 with `Ideal Ratio` or 1376x768 below). `Default Ratio` currently only takes effect
when play in `1920x1080` or above, making the UI blurry.

#### Preventing double jump(s)
**This is subject to change** but if you want to the bot to only walk between points then the two
points `x` distance should be less than `25`.

#### Mage up jump
If your mage class does not have an up jump through jump key but only through teleport, you need to set 
the up jump key the same as the teleport key.

#### Installation
If you use the bot on a newly installed Windows, make sure [Visual C++ Redistributable 2015-2022](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist#visual-studio-2015-2017-2019-and-2022) is installed.

## Contact
- You can reach me by creating an issue on Github or by joining the [Discord](https://discord.gg/ReTp6MHgF6)
- The Discord contains other guide resources

## Video guides
1. [Basic operations](https://youtu.be/8X2CKS7bnHY?si=3yPmVPaMsFEyDD8c)
2. [Auto-mobbing and platforms pathing](https://youtu.be/8r2duEz6278?si=HTHb8WXh6L7ulCoE)
3. Rotation modes, linked key and linked actions - TODO

## Showcase
#### Rotation
https://github.com/user-attachments/assets/3c66dcb9-7196-4245-a7ea-4253f214bba6

(This Blaster rotation was before Link Key & Link Action were added)

https://github.com/user-attachments/assets/463b9844-0950-4371-9644-14fad5e1fab9

#### Auto Mobbing & Platforms Pathing
https://github.com/user-attachments/assets/3f087f83-f956-4ee1-84b0-1a31286413ef

#### Rune Solving
https://github.com/user-attachments/assets/e9ebfc60-42bc-49ef-a367-3c20a1cd00e0
