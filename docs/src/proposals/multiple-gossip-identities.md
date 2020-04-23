# Multiple Gossip Identities

The current gossip design requires that a single validator ID is run by a
single node. If more than one machine runs at the same time they will overwrite
each others CRDS values. Ideally, a validator might want to be able to run
multiple nodes in gossip that represent the same ID.

Some use cases:

- Enables H/A with multiple nodes to be run in parallel.
- Autoscaling by running low/high power nodes at the same time.
- Prioritize Turbine traffic to nodes shared by a single validator.

By allowing this however, changes to how the validator runs when it is a staked
node are required. Much of Solana's codebase makes decisions based on whether
it is currently an active leader or not, so simply enabling the gossip changes
is not enough for a node to exist passively on the network.


# Proposal


### Gossip Changes

Extend gossip with another public key, known as a "node" key. This key only
identifies an individual machine in gossip, and is not associated with any on
chain state. `CrdsValue` entries are then associated with the node key instead
of the current ID key, while the ID key remains the key used in on-chai state
and consensus.

Internal to the validator itself, lookups for the a validator ID can now treat
gossip as "groups" of nodes. Looking up some validator ID in gossip can return
a list of `ContactInfo`, which can be used to make decisions in Turbine or any
other code that cares about the physical layout of validator machines.

To make this transition easier, this node state should be kept completely
separate from the chain itself, as the main purpose of adding this key is to
allow some flexibility in how a validator runs its setup. If we were to try and
embed this key on-chain, it would defeat the reliability of H/A-like software
which aims to manage these nodes precisely when the chain might fail.

### Active State Changes

In order to allow the validator to act passively on the network, it extends the
current --no-voting mode to also prevent the node from trying to broadcast slot
data. If two nodes with the same data believe they are both currently leader
but one isn't voting, then both nodes will broadcast conflicting shreds for a
particular slot.

### Validator State Changes

The ability to toggle voting on/off is an important requirement for H/A
software that might run multiple nodes, but is also useful for standard 
supervisor software to take advantage of.

In particular, allowing toggling of states makes it possible for administrators
to take their node "offline" without having to shut down the validator itself.
This might be useful in cases where a validator owner might want to make
changes to their setup and disable voting, but keep the process alive so that
it can keep reporting metrics, or be re-enabled quickly.

The initial suggestion would be an a new RPC call that switches the validator's
`--no-voting` state. This currently works by clearing out the known signing
keys so to change this, flags would need to be propagated through the codebase.
As much of the codebase already relies on `CLusterInfo` to make decisions about
behaviour it could be a good place to store this information.



# Impact

1. Initially, this will mean turbine will begin sending data to multiple nodes
   owned by a single validator, which opens up a potential attack vector. To
   resolve this, Turbine should take into account validator groups and only
   send data to a single node. That node can then within its own turbine
   prioritize forwarding traffic to nodes in its own group. With this change
   this actually increases turbines throughput.

2. Internally, services should continue to use the ID key for packet and shred
   data, but many services rely on `cluster_info` and gossip to find this key.
   This change requires a careful check to make sure the correct keys are being
   used within the various stages.



# Alternatives

One alternative that was suggested initially, is to allow VoteState to contain
multiple ID keys. It already allows multiple voting keys, though currently only
one is possibly active as the codebase currently handles it.

With this change, two `ID` keys in Gossip can be owned by the same VoteState. There
are a few problems with this model though:

1. Changes to VoteState keys currently don't take effect until epoch
   boundaries. Validators who are changing their internal machine setups likely
   want to make these changes without having to reflect them on-chain.

2. The changes to implement this model are more complex, as a lot of the
   codebase will have to deal with the possibility of receiving consensus data
   signed by multiple different keys. A node key keeps the current one job one
   key status.
