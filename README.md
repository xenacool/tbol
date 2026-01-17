# The Beam of Light

The Beam of Light (TBOL) is a sandboxed networked 3d application. The system is designed for an RPG I'm writing, The Beam of Light Campaign. Since this content contains assets and intellectual property under a different license, it is hosted privately elsewhere. This code is the underlying platform is intended to be used for turn-based games, action adventures, social experiences, etc..


This is ALPHA and not ready for public use.

## Infrastructure Rough plan

The logical/datastructures start with networking. Each island is made up of 1 or more rooms. Rooms are rectangular. Everything happens in a room, whether it's dialogue, indoors or outdoors. 

veilnet for networking with unique addresses given to each island.
Each room will have a bidirectional, deterministic log that materializes the current state and can be snapshotted.

Each island will have a log of vector clocks representing transaction epochs across rooms. The turn-counter for each room can be advanced in parallel.

mcts for the game ai, but if that seems difficult for I may revert to a utility-based approach.

yarnspinner's storylets will be the main entry point into dialogue trees.

luau via mlua for scripting of character classes, campaign events, dialogue encounters, placement of data in the UI, snapshotting of savefiles, and unit and property testing.

Rendering is done in godot with the primary coding done in rust via gdext.

The memory model in Rust adopt's godot's system but an arena or ecs might be more appropriate later.

## Implementation Plan: MDA Framework

### Milestone 1: Mechanics (Core Systems)
**Goal**: Single-player tactical combat prototype testable in unit tests (no Godot required). 3-5 test encounters with proptest validation for balance. Working lobby exists but full networking deferred to M2.

**1.1-1.2: Foundation (COMPLETE)**
- Room system with adjacency
- Automatic room loading via BFS
- RON serialization
- Path sandboxing
- Networking lobby UI (partial - exists but not integrated)
- Tokio runtime
- GDext library structure
