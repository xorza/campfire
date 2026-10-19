# Gather loop

What the gather loop costs, and how Zero Hour finds a supply source, recorded 2026-10-18.

## Measurement

- **Machine:** Intel Core i9-13980HX, Linux, x86-64-v3, with other work running.
- **A run:** `./drive.py bench -- integration/gather` at `12a98cb63`, criterion's defaults: two fields of 8 nodes each, 200 workers in their loop, a match of production and navigation.
- **Result:** one tick costs 20.5 µs, 9.7 million workers a second.

## Zero Hour

- `ResourceGatheringManager` keeps each player's supply warehouses in a list, which `addSupplyWarehouse` and `removeSupplyWarehouse` keep, and `findBestSupplyWarehouse` scans the list for the best one; it queries no spatial partition (GeneralsX, `GameEngine/Source/Common/RTS/ResourceGatheringManager.cpp`).
