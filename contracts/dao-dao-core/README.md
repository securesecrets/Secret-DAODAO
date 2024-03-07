# dao-dao-core

This contract is the core module for all DAO DAO DAOs. It handles
management of voting power and proposal modules, executes messages,
and holds the DAO's treasury.

For more information about how these modules fit together see
[this wiki page](https://github.com/DA0-DA0/dao-contracts/wiki/DAO-DAO-Contracts-Design).

In addition to the wiki spec this contract may also pause. To do so a
`Pause` message must be executed by a proposal module. Pausing the
core module will stop all actions on the module for the duration of
the pause.

## Developing
Core messages and interfaces are defined in the [dao-interfaces](../../packages/dao-interface) package. If you are building new modules or a contract that interacts with a DAO, use `dao-interface`.

## Treasury management

For management of non-native assets this contract maintains a list of
[snip20](https://github.com/scrtlabs/snip20-reference-impl)
and
[snip721](https://github.com/baedrik/snip721-reference-impl)
tokens who's balances the DAO would like to track. This allows
frontends to list these tokens in the DAO's treasury. Note that in Secret DAO DAO, all 
user balances are private, and therefore their voting power as well.

For native tokens we do not need this additional tracking step, as
native token balances are stored in the [bank
module](https://github.com/cosmos/cosmos-sdk/tree/main/x/bank). Thus,
for those tokens frontends can query the chain directly to discover
which tokens the DAO owns.

### Managing the treasury

There are two ways that a non-native token may be added to the DAO
treasury.

If `automatically_add_[snip20s|snip721s]` is set to true in the [DAO's
config](https://github.com/DA0-DA0/dao-contracts/blob/74bd3881fdd86829e5e8b132b9952dd64f2d0737/contracts/dao-dao/src/state.rs#L16-L21),
the DAO will add the token to the treasury upon receiving the token
via snip20's `Send` method and snip721's `SendNft` method.

The DAO may always add or remove non-native tokens via the
`UpdateSnip20List` and `UpdateSnip721List` methods.