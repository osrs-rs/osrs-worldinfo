# worldinfo

Implementation for PlayerInfo and NpcInfo for Oldschool Runescape.

[![Build](https://github.com/runecore/worldinfo/workflows/build/badge.svg)](https://github.com/runecore/worldinfo)
[![API](https://docs.rs/worldinfo/badge.svg)](https://docs.rs/worldinfo)
[![Crate](https://img.shields.io/crates/v/worldinfo)](https://crates.io/crates/worldinfo)
[![dependency status](https://deps.rs/repo/github/runecore/worldinfo/status.svg)](https://deps.rs/repo/github/runecore/worldinfo)
[![Discord](https://img.shields.io/discord/926860365873184768?color=5865F2)](https://discord.gg/CcTa7TZfSc)

## Usage

TODO

## License

This project is licensed under the [MIT license](license-mit).

## Contributing

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in `worldinfo` by you, shall be licensed as MIT, without any additional terms or conditions.

## TODO's

Rewrite to the following implementation:

```txt
1. first run to do the bitbuffer stuff, and mark down what masks u need from each player.
2. second run to build all the masks that are requested(e.g. if player X needs my appearance mask, I build appearance mask right now) - note, hits mask is observer-dependent and cannot be cached, the rest can
3. third run to append the masks of everyone to the end of your buffer.

all three of the steps can be done asynchronously, that is to say you can build the info for N players concurrently(e.g. thread 1 on player A, thread 2 on player B etc, i dont mean that u split the inner tasks into chunks cus that is borderline impossible)

all in all, this way u remove one of the biggest bottlenecks that u might otherwise come across in the worst edge case scenario - that is putting 2000 players in a small box.
each of the 2000 players will observe everyone else, resulting in 2000^2 masks being built, while - if we exclude hit mask entirely, only 2000 would be built otherwise.
```
