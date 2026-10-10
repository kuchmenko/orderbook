use derive_more::Display;
use uuid::Uuid;

use crate::book::Book;

#[derive(
    Debug,
    Copy,
    Clone,
    PartialEq,
    PartialOrd,
    Eq,
    Hash,
    Display,
    serde::Deserialize,
    serde::Serialize,
)]
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
