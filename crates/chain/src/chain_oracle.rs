use crate::BlockId;

/// Represents a service that tracks the blockchain.
///
/// The main method is [`is_block_in_chain`] which determines whether a given block of [`BlockId`]
/// is an ancestor of another "static block".
///
/// [`is_block_in_chain`]: Self::is_block_in_chain
pub trait ChainOracle {
    /// Error type.
    type Error: core::fmt::Debug;

    /// Determines whether `block` of [`BlockId`] exists as an ancestor of `static_block`.
    ///
    /// If `None` is returned, it means the implementation cannot determine whether `block` exists.
    fn is_block_in_chain(
        &self,
        block: BlockId,
        static_block: BlockId,
    ) -> Result<Option<bool>, Self::Error>;
}
