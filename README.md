![Emo Cat](https://github.com/sasanquaa/maple-bot/blob/master/.github/images/emo-cat.gif)

- [Features](#features)
- [How to use](#how-to-use)
  - [Map](#map)
  - [Configuration](#configuration)
  - [Action](#action)
  - [Condition](#condition)
  - [Linked Key & Linked Action](#linked-key--linked-action)
  - [Rotation Modes](#rotation-modes)
  - [Platforms Pathing](#platforms-pathing)
- [Troubleshooting](#troubleshooting)
  - [Wrong map detection](#wrong-map-detection)
  - [Actions contention](#actions-contention-)
  - [Default Ratio game resolution](#default-ratio-game-resolution)
- [Showcase](#showcase)
  - [Rotation](#rotation)
  - [Auto Mobbing & Platforms Pathing](#auto-mobbing-%26-platforms-pathing)
  - [Rune Solving](#rune-solving)

## Features
- More like a rotation maker than a bot? (cope)
- Run around and hit random mobs mode (literally a bot)
- Auto buffs for common farming-related buffs
- Auto potion (periodically or somewhat functional Auto HP-like pet skill)
- Solve rune (for spinning rune, it uses the lost ancient technique of AFK-ing in cash shop for 10 seconds)
- Platforms pathing (find a path to reach a platform)
- ~~Barely maintainable~~ UI ~~(please help)~~
- Work by taking an image and send key inputs (no memory hacking)
- Not a feature but currently only work in GMS (haven't tested MSEA but it is in English...)
  - From v0.2.0, because of `Manual map`, map's name should be language-agnostic but there are still other detection resources in English... (just PNGs)
- I hate this game 
- Why do Jett gets free BOD just to be deleted?

## How to use
#### Map
- Map is automatically detected, saved and restored anytime you go to the map
- Any actions preset created in the detected map is saved to that map only
- **Map detection can be wrong**, see [Troubleshooting](#troubleshooting)
- From v0.2.0, `Manual map` can now be selected:
  - This allows opting out of automatic map's name detection
  - You can manually add a new map with custom name based on current map detection
  - Maybe removing automatic map detection altogether?

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

For linked key, there are three link types:
- `Before` - Uses the link key before the actual key (e.g. for Cadena, Chain Arts: Thrash is the link key)
- `AtTheSame` - Uses the link key at the same time as the actual key (probably only Blaster skating needs this)
- `After` - Uses the link key after the actual key (e.g. for Blaster, Weaving/Bobbing is the link key)

Note that even though `AtTheSame` would send two keys simultaneously, *the link key will be send first*. When the configured
class is set to Blaster, the performing action has `After` link type and the link key is not `Space`, an extra `Space` key will be sent for cancelling Bobbing/Weaving. The same effect can also be achieved through linked action.

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

![Auto Mobbing](https://github.com/sasanquaa/maple-bot/blob/master/.github/images/automobbing.png?raw=true)

#### Platforms Pathing
Platforms pathing is currently only supported for Auto Mobbing and Rune Solving. This feature exists to help
pathing around platforms with or without `Rope Lift` skill. To use this feature, add all the map's platforms starting
from the ground level.

Without this feature, the bot movement is quite simple. It just moves horizontally first so the `x` matches the destination
and then try to up jump, rope lift or drop down as appropriate to match the `y`.

![Platforms](https://github.com/sasanquaa/maple-bot/blob/master/.github/images/platforms.png?raw=true)

## Troubleshooting
#### Wrong map detection
Wrong map detection can happen when:
- The detected map's name is too different from existing map's name and see it as a new map
- Some maps can have different name detected at different position
- Other UIs overlapping

Rule of thumb is: Make sure the map's name is detected persistently each time you enter the map. As long as 
the detected name is similar, the map data will be restored. **Note that the detected name may not be accurate
but as long as the same name (or similar name) is detected each time, it should be fine**.

Fix methods:
- Below the map are three buttons, two of which can be used to help troubleshooting:
    - `Re-detect map`: Use this button to re-detect the map
    - `Delete map`: Use this to **permanently delete** the map, only use when you know the map's name is wrong
- Move the map UI around
- Move around the map (I encountered this in some Sellas maps)
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

## Showcase
#### Rotation
https://github.com/user-attachments/assets/3c66dcb9-7196-4245-a7ea-4253f214bba6

(This Blaster rotation was before Link Key & Link Action were added)

https://github.com/user-attachments/assets/463b9844-0950-4371-9644-14fad5e1fab9

#### Auto Mobbing & Platforms Pathing
https://github.com/user-attachments/assets/3f087f83-f956-4ee1-84b0-1a31286413ef

#### Rune Solving
https://github.com/user-attachments/assets/e9ebfc60-42bc-49ef-a367-3c20a1cd00e0

