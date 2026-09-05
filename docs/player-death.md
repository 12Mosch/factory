# Player death and respawn

Player damage resolves after world automation and manual crafting in each fixed
simulation tick. Simultaneous attacks are accumulated before damage is applied.
The first transition to zero health records the death tick and increments the
lifetime player-death statistic exactly once. Further damage ignores the dead
player.

Click **Respawn** on the death panel to request recovery. If the game is paused,
resume it first. Requests are consumed at the start of the next simulation tick.
All remaining gameplay commands in the request's batch still see the dead state
and are rejected; they are never deferred until after respawn. Enemy settings,
saving, loading, and presentation controls remain available. World automation
continues while the player is dead.

Recovery chooses a generated, walkable tile without a placed entity, nearest to
the world origin by square-ring distance, with ties ordered by y then x. This is
also the new-player spawn rule. It never clears buildings or rewrites terrain.
If no valid tile exists, the request remains pending until one becomes available.
Recovery restores maximum health, preserving armor resistances and world state.

The initial penalty retains all items: inventory, equipped armor and modules,
opened ammunition, and partially used repair packs. Stored armor battery, shield,
and personal-roboport energy (including fractional energy) are lost. Cooldowns
are retained. Manual crafting pauses with its reserved ingredients and progress;
there is no death-time refund that could overflow a full inventory. Personal
robots pause in place with their cargo, payloads and charging ownership intact;
no new personal jobs dispatch while dead. World changes may invalidate their
jobs through the normal reconciliation rules. After respawn robots resume and
use the existing return/recovery rules, including waiting when inventory is full.
Stationary robot networks continue to operate. No corpse or recovery container
is created under this policy.

Death tick, pending recovery, death statistics, inventory, crafting and equipment
state participate in save/load and deterministic hashing. Save format 53 adds
these fields; older versions are rejected through the existing version check.
