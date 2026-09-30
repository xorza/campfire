# Issues

- An ability whose `targeting` names a tag, such as `enemies:hero`, fails to load, though design 08 allows a filter there.
- One player's many calls can spend the whole `input` pool in a tick, since mode inputs and casts share it: another player's cast then fails, which design 02 says no other script can make happen.
