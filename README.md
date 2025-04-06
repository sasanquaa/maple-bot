- [Features](#features)
- [How to use](#how-to-use)
- [Troubleshooting](#troubleshooting)
- [Showcase](#showcase)
  - [Rotation](#rotation)
  - [Auto Mobbing & Platforms Pathing](#auto-mobbing-%26-platforms-pathing)
  - [Rune Solving](#rune-solving)

## Features
- More like a rotation maker than a bot?
- Run around and hit random mobs mode (literally a bot)
- Auto buffs for common farming-related buffs
- Auto potion (periodically or somewhat functional Auto HP-like pet skill)
- Solve rune (for spinning rune, it uses the lost ancient technique of AFK-ing in cash shop for 10 seconds)
- Platforms pathing (find a path to reach a platform)
- ~~Barely maintainable~~ UI ~~(please help)~~
- Work by taking an image and send key inputs (no memory hacking)
- Not a feature but currently only work in GMS (haven't tested MSEA but it is in English...)
- I hate this game

## How to use
#### Map
- Map is automatically detected, saved and restored anytime you go to the map
- Any actions preset created in the detected map is saved to that map only
- **Map detection can be wrong**, see [Troubleshooting](#troubleshooting)

The arcs are only for visual and do not represent the actual moving path. However, it does represent
the order of one action to another depending on rotation mode.

![Map](https://github.com/sasanquaa/maple-bot/blob/master/.github/images/map.png?raw=true)

#### Configuration
- Configuration is used to change key bindings, set up buffs,...
- Configuration can be created for use with different character(s) through preset
- Configuration is saved globally and not affected by the detected map

![Configuration](https://github.com/sasanquaa/maple-bot/blob/master/.github/images/configuration.png?raw=true)

#### Action
There are two types of action:
- `Move` - Moves to a location on the map
- `Key` - Uses a key with or without location

An action is further categorized into two:
- A normal action is an action with condition set to `Any`
- A priority action is any `ErdaShowerOffCooldown`/`EveryMillis` action

A priority action can override a normal action and force the player to perform the former. The
normal action is not completely overriden and is only delayed until the priority action is complete.

Action properties:
- `Position`: Optionally add a position to use the key
- `Count`: Number of times to use the key
- `Key`: The key to use
- `Has link key`: Optionally enable link key (useful for [combo classes](linked-key--linked-action))
- `Queue to front`:
  - Applicable only to `EveryMillis` and `ErdaShowerOffCooldown` conditions
  - When set, this action can override other non-`Queue to front` priority action
  - The overriden priority action is not lost but delayed like normal action
  - Cannot override linked action
- `Direction`: The direction to use the key
- `With`:
  - `Stationary` - Performs an action only when standing on ground (for buffs)
  - `DoubleJump` - Performs an action with double jump
- `Wait before action`/`Wait after action`:
  - Wait for the specified amount of millseconds after/before using the key
  - Waiting is applied on each repeat of `Count`

![Actions](https://github.com/sasanquaa/maple-bot/blob/master/.github/images/actions.png?raw=true)

#### Condition
There are four types of condition:
- `Any` - Does not do anything special and affected by rotation mode 
- `ErdaShowerOffCooldown` - Runs an action only when Erda Shower is off-cooldown
- `EveryMillis` - Runs an action every `x` milliseconds
- `Linked` - Runs an action chained to the previous action (e.g. like a combo) 

For `ErdaShowerOffCooldown` condition to work, the skill Erda Shower must be assigned to
the quick slots, with Action Customization toggled on and **visible** on screen.

TODO Add image

#### Rotation Modes
Rotation mode specifies how to run the actions and affects **only** `Any` condition actions. There are three modes:
- `StartToEnd` - Runs actions from start to end in the order added and repeats
- `StartToEndThenReverse` - Runs actions from start to end in the order added and reverses (end to start)
- `AutoMobbing` - All added actions are ignored and, instead, detects a random mob within bounds to hit

For other conditions actions:
- `EveryMillis` actions run out of order
- `ErdaShowerOffCooldown` actions run in the order added same as `StartToEnd`

#### Linked Key & Linked Action
Linked key and linked action are useful for combo-oriented class such as Blaster, Cadena, Ark, Mercedes,...
Animation cancel timing is specific to each class. As such, the timing is approximated and provided in the configuration, so make sure you select the appropriate one.

For linked key, there are three link types:
- `Before` - Uses the link key before the actual key (e.g. for Cadena, Chain Arts: Thrash is the link key)
- `AtTheSame` - Uses the link key at the same time as the actual key (probably only Blaster skating needs this)
- `After` - Uses the link key after the actual key (e.g. for Blaster, Weaving/Bobbing is the link key)

Note that even though `AtTheSame` would send two keys simultaneously, *the link key will be send first*. When the configured
class is set to Blaster, the performing action has `After` link type and the link key is not `Space`, an extra `Space` key will be sent for cancelling Bobbing/Weaving. The same effect can also be achieved through linked action.

Linked action is for linking action(s) into a chain. Linked action is straightforward and can be created by adding a `Linked` condition action below any `Any`/`ErdaShowerOffCooldown`/`EveryMillis`/`Linked` action. The first non-`Linked` action is the start of the actions chain:

```
Any Linked Linked Linked   EveryMillis Linked Linked
 ▲                     ▲    ▲                    ▲  
 │                     │    │                    │  
 │                     │    │                    │  
 └─────────────────────┘    └────────────────────┘  
          Chain                      Chain          
```
Linked action cannot be overriden by any other type of actions once it has started executing regardless of whether the action is a normal or priority action.

TODO Add image

## Troubleshooting
#### Wrong map detection

## Showcase
#### Rotation
https://github.com/user-attachments/assets/3c66dcb9-7196-4245-a7ea-4253f214bba6

https://github.com/user-attachments/assets/463b9844-0950-4371-9644-14fad5e1fab9
#### Auto Mobbing & Platforms Pathing
https://github.com/user-attachments/assets/3f087f83-f956-4ee1-84b0-1a31286413ef
#### Rune Solving
https://github.com/user-attachments/assets/e9ebfc60-42bc-49ef-a367-3c20a1cd00e0

