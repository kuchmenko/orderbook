use std::{fmt::Display, ops::Add};

use derive_more::Display;

#[derive(Debug, Copy, Clone, Eq, PartialEq, PartialOrd, Ord, Hash, Display)]
#[display("{epoch}:{sequence}")]
pub struct EpochSequenceId {
    pub epoch: u32,
    pub sequence: u64,
}

impl EpochSequenceId {
    pub fn new(epoch: u32, sequence: u64) -> Self {
        Self { epoch, sequence }
    }
}

pub trait IdGenerator<Id> {
    fn next(&mut self) -> Id;
}

pub struct EpochSequenceIdGenerator {
    pub epoch: u32,
    pub sequence: u64,
}

impl EpochSequenceIdGenerator {
    pub fn new() -> Self {
        Self {
            epoch: 0,
            sequence: 0,
        }
    }

    pub fn restore(epoch: u32, sequence: u64) -> Self {
        Self { epoch, sequence }
    }
}

impl IdGenerator<EpochSequenceId> for EpochSequenceIdGenerator {
    fn next(&mut self) -> EpochSequenceId {
        let next_sequence = self.sequence.checked_add(1).unwrap_or(0);

        if next_sequence < self.sequence && next_sequence == 0 {
            self.epoch = self.epoch.add(1)
        }

        self.sequence = next_sequence;

        EpochSequenceId {
            epoch: self.epoch,
            sequence: self.sequence,
        }
    }
}
