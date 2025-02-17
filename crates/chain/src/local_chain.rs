use core::convert::Infallible;

use alloc::collections::{BTreeMap, BTreeSet};
use bitcoin::BlockHash;

use crate::{BlockId, ChainOracle};

/// This is a local implementation of [`ChainOracle`].
///
/// TODO: We need a cache/snapshot thing for chain oracle.
/// * Minimize calls to remotes.
/// * Can we cache it forever? Should we drop stuff?
/// * Assume anything deeper than (i.e. 10) blocks won't be reorged.
/// * Is this a cache on txs or block? or both?
/// TODO: Parents of children are confirmed if children are confirmed.
#[derive(Debug, Default, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LocalChain {
    blocks: BTreeMap<u32, BlockHash>,
}

impl ChainOracle for LocalChain {
    type Error = Infallible;

    fn is_block_in_chain(
        &self,
        block: BlockId,
        static_block: BlockId,
    ) -> Result<Option<bool>, Self::Error> {
        if block.height > static_block.height {
            return Ok(None);
        }
        Ok(
            match (
                self.blocks.get(&block.height),
                self.blocks.get(&static_block.height),
            ) {
                (Some(&hash), Some(&static_hash)) => {
                    Some(hash == block.hash && static_hash == static_block.hash)
                }
                _ => None,
            },
        )
    }
}

impl AsRef<BTreeMap<u32, BlockHash>> for LocalChain {
    fn as_ref(&self) -> &BTreeMap<u32, BlockHash> {
        &self.blocks
    }
}

impl From<LocalChain> for BTreeMap<u32, BlockHash> {
    fn from(value: LocalChain) -> Self {
        value.blocks
    }
}

impl From<BTreeMap<u32, BlockHash>> for LocalChain {
    fn from(value: BTreeMap<u32, BlockHash>) -> Self {
        Self { blocks: value }
    }
}

impl LocalChain {
    pub fn from_blocks<B>(blocks: B) -> Self
    where
        B: IntoIterator<Item = BlockId>,
    {
        Self {
            blocks: blocks.into_iter().map(|b| (b.height, b.hash)).collect(),
        }
    }

    pub fn tip(&self) -> Option<BlockId> {
        self.blocks
            .iter()
            .last()
            .map(|(&height, &hash)| BlockId { height, hash })
    }

    /// Get a block at the given height.
    pub fn get_block(&self, height: u32) -> Option<BlockId> {
        self.blocks
            .get(&height)
            .map(|&hash| BlockId { height, hash })
    }

    /// This is like the sparsechain's logic, expect we must guarantee that all invalidated heights
    /// are to be re-filled.
    pub fn determine_changeset(&self, update: &Self) -> Result<ChangeSet, UpdateNotConnectedError> {
        let update = update.as_ref();
        let update_tip = match update.keys().last().cloned() {
            Some(tip) => tip,
            None => return Ok(ChangeSet::default()),
        };

        // this is the latest height where both the update and local chain has the same block hash
        let agreement_height = update
            .iter()
            .rev()
            .find(|&(u_height, u_hash)| self.blocks.get(u_height) == Some(u_hash))
            .map(|(&height, _)| height);

        // the lower bound of the range to invalidate
        let invalidate_lb = match agreement_height {
            Some(height) if height == update_tip => u32::MAX,
            Some(height) => height + 1,
            None => 0,
        };

        // the first block's height to invalidate in the local chain
        let invalidate_from_height = self.blocks.range(invalidate_lb..).next().map(|(&h, _)| h);

        // the first block of height to invalidate (if any) should be represented in the update
        if let Some(first_invalid_height) = invalidate_from_height {
            if !update.contains_key(&first_invalid_height) {
                return Err(UpdateNotConnectedError(first_invalid_height));
            }
        }

        let mut changeset: BTreeMap<u32, Option<BlockHash>> = match invalidate_from_height {
            Some(first_invalid_height) => {
                // the first block of height to invalidate should be represented in the update
                if !update.contains_key(&first_invalid_height) {
                    return Err(UpdateNotConnectedError(first_invalid_height));
                }
                self.blocks
                    .range(first_invalid_height..)
                    .map(|(height, _)| (*height, None))
                    .collect()
            }
            None => BTreeMap::new(),
        };
        for (height, update_hash) in update {
            let original_hash = self.blocks.get(height);
            if Some(update_hash) != original_hash {
                changeset.insert(*height, Some(*update_hash));
            }
        }

        Ok(changeset)
    }

    /// Applies the given `changeset`.
    pub fn apply_changeset(&mut self, changeset: ChangeSet) {
        for (height, blockhash) in changeset {
            match blockhash {
                Some(blockhash) => self.blocks.insert(height, blockhash),
                None => self.blocks.remove(&height),
            };
        }
    }

    /// Updates [`LocalChain`] with an update [`LocalChain`].
    ///
    /// This is equivalent to calling [`determine_changeset`] and [`apply_changeset`] in sequence.
    ///
    /// [`determine_changeset`]: Self::determine_changeset
    /// [`apply_changeset`]: Self::apply_changeset
    pub fn apply_update(&mut self, update: Self) -> Result<ChangeSet, UpdateNotConnectedError> {
        let changeset = self.determine_changeset(&update)?;
        self.apply_changeset(changeset.clone());
        Ok(changeset)
    }

    pub fn initial_changeset(&self) -> ChangeSet {
        self.blocks
            .iter()
            .map(|(&height, &hash)| (height, Some(hash)))
            .collect()
    }

    pub fn heights(&self) -> BTreeSet<u32> {
        self.blocks.keys().cloned().collect()
    }
}

/// This is the return value of [`determine_changeset`] and represents changes to [`LocalChain`].
///
/// [`determine_changeset`]: LocalChain::determine_changeset
pub type ChangeSet = BTreeMap<u32, Option<BlockHash>>;

/// Represents an update failure of [`LocalChain`] due to the update not connecting to the original
/// chain.
///
/// The update cannot be applied to the chain because the chain suffix it represents did not
/// connect to the existing chain. This error case contains the checkpoint height to include so
/// that the chains can connect.
#[derive(Clone, Debug, PartialEq)]
pub struct UpdateNotConnectedError(pub u32);

impl core::fmt::Display for UpdateNotConnectedError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "the update cannot connect with the chain, try include block at height {}",
            self.0
        )
    }
}

#[cfg(feature = "std")]
impl std::error::Error for UpdateNotConnectedError {}
