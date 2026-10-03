use derive_more::Display;
use tracing_subscriber::field::display;
use uuid::Uuid;

use crate::{book::Book, id::EpochSequenceId};

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Hash, Display)]
#[display("{_0}")]
pub struct MarketId(pub Uuid);

#[derive(Debug)]
pub struct Market {
    pub id: MarketId,
    pub book: Book,
}

impl Market {
    pub fn new(id: MarketId) -> Self {
        Self {
            id,
            book: Book::new(),
        }
    }
}
