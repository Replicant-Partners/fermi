# Location Noise: Gameplay and Supply Chain Scenarios

## The Mechanic

A tethered creature's GPS defines a rabble's published location. But the tether status is private — other participants see the rabble's position, not which creature is the anchor. A player can place multiple creatures in different configurations: one tethered to their real GPS, another at a simulated location, a third in a rabble anchored by someone else. Each creature reports a different position. The system doesn't distinguish real from simulated — it just records transitions.

This is not a bug or a hack. It's structural. The append-only model treats every location as equally valid. There's no "true position" in the system — only the most recent log entry for each creature.

---

## Gameplay Scenarios

### The Decoy Flock

A player tethers Creature A to their phone — real GPS, walking through a park. They place Creature B at a simulated location across town, anchoring a public rabble. Other players join Creature B's rabble, seeing it as a gathering point. Meanwhile the player's actual position (Creature A) is elsewhere entirely.

Why do this? Maybe Creature A is a rare dragonfly exploring a sensitive habitat. The player doesn't want followers. The decoy flock draws attention away from the real exploration.

### The Shell Game

Three rabbles, three anchor creatures, one player. Each rabble publishes a different location. Other players can join any of them. The player rotates which anchor is tethered to their real GPS — sometimes it's rabble 1, sometimes rabble 3. The other two freeze at their last known position.

Observers see three active rabbles. They can't tell which one is currently live without joining and watching the movement pattern. Even then, the player can untether and re-tether to a different anchor at any time.

### The Convoy

A group of friends each tether a creature to their own phone. All creatures join the same rabble. The rabble's official position is the anchor creature's GPS, but each tethered creature has its own independent track. The flock dynamics now show real spatial relationships — who's ahead, who's drifting, how the group moves through space.

If the anchor hands off (transfers anchor to another creature), the rabble's center jumps to the new anchor's position. The convoy reshuffles around a new leader without anyone leaving the rabble.

### The Honeypot

A player creates a rabble with a small radius (10m) at a popular location — a café, a park bench. They set walk-in price to 0 (free). The anchor creature is tethered to their phone sitting on the table. Other creatures join, clustering in the small radius.

Now the player picks up their phone and walks away. The anchor moves. The rabble moves with it. Every creature in the rabble is now "at" wherever the anchor goes — even though the other players might be across town. The location on their creature's log shows the café, then suddenly a park, then a bus stop.

The other players' creatures were never physically at those places. But the log says they were.

### The Stationary Blind

Opposite of the honeypot. A player untethers their anchor creature, freezing the rabble at a fixed point. New creatures join at that frozen location. The rabble becomes a permanent observation post — a "bird blind" that persists at a GPS coordinate regardless of where any player actually is.

Creatures accumulate at the blind over time. The flock dynamics operate within the radius. The original player doesn't even need to be online — the blind exists as long as the rabble is active.

---

## Supply Chain Application

The same mechanics that let a teenager play shell games with butterfly creatures can protect a logistics network.

### The Problem

A supply chain has nodes: warehouses, trucks, distribution centers, retail points. Each node has a real physical location that, if known to an adversary, reveals the structure of the entire network. Traditional approaches hide this with access control — only authorized people see the map. But access control fails at the edges: a driver's phone leaks GPS, a warehouse address appears on a shipping label, a delivery photo reveals a loading dock.

### Location Noise as Defense

Replace "creature" with "asset tracker." Replace "rabble" with "logistics cluster." The mechanics are identical:

**Decoy clusters.** Real trucks carry tethered trackers. Simulated trackers anchor decoy clusters at plausible but fake warehouse locations. An adversary monitoring the public-facing position data sees five warehouses. Only two are real. They can't tell which without physically visiting each one.

**Convoy obfuscation.** A fleet of trucks joins a single logistics cluster. The cluster's published position is the lead truck (anchor). Individual truck positions are private (tether status is per-device). An adversary sees one moving cluster, not twelve individual routes. They know roughly where the fleet is going but not the distribution of vehicles within it.

**Anchor rotation.** The logistics cluster periodically transfers its anchor to a different truck. The published position jumps. The actual trucks haven't changed course — but the observable trajectory is now a zigzag between anchor handoffs. Route prediction breaks.

**Stationary blinds as dead drops.** A logistics cluster is frozen at a GPS coordinate — a parking lot, a rest stop. Trucks "join" the cluster when they arrive and "leave" when they depart. The cluster's existence at that point is public, but which trucks are currently there is private. It's a dead drop that exists in the location layer without revealing its current inventory.

**Radius as operational security.** A small radius (10m) means the cluster is precise — useful for a specific loading dock. A large radius (1km) means the cluster covers an area — useful for obscuring which building in an industrial park is the actual warehouse. The radius itself is a tunable noise parameter.

### What Makes This Different

This isn't encryption. The data is all public. Any creature can see any rabble's published location. The defense comes from **the gap between what's published and what's real** — and the system's structural inability to distinguish between them. There's no "decrypt" operation that reveals the true positions, because the system genuinely doesn't know. The append-only log records what it's told. The tether pushes what the GPS says. The simulation places what the user chooses. All entries are first-class.

An adversary with full read access to the database sees every rabble, every creature, every location. They still can't tell which locations are real GPS and which are simulated, because the data model doesn't record that distinction at the rabble level. The `data_source` field on individual flights says "device" vs "synthetic" — but that's per-creature, and creature-level data is owner-private.

The noise is not added on top of the signal. The noise IS the signal. The system works the same way whether you're playing with butterflies or protecting a cold chain.
