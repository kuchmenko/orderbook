use std::{
    cmp::min,
    collections::{BTreeMap, VecDeque, btree_map::IterMut},
    fmt::Display,
};

use derive_more::Display;

use crate::id::EpochSequenceId;
#[derive(Debug, Default)]
pub struct Level {
    price: Price,
    total_amount: u32,
    orders: VecDeque<OrderId>,
}

impl Level {
    fn matching(&mut self, orders: &mut BTreeMap<OrderId, Order>, amount: &mut u32) -> MatchResult {
        if self.orders.is_empty() {
            return MatchResult::None;
        }

        let mut matched = vec![];

        while *amount > 0 {
            let Some(order_id) = self.orders.pop_front() else {
                break;
            };

            let Some(order) = orders.get_mut(&order_id) else {
                break;
            };

            let taker_amount = min(order.amount, *amount);
            let total_amount = order.amount;

            order.amount -= taker_amount;
            *amount -= taker_amount;
            self.total_amount -= taker_amount;

            println!("matched order {:?}, taker_amount: {}", order, taker_amount);

            if order.amount > 0 {
                self.orders.push_front(order_id);
            }

            matched.push(Fill {
                order_id: order.id,
                price: self.price,
                total_amount,
                amount: taker_amount,
            });
        }

        if matched.is_empty() {
            return MatchResult::None;
        }

        MatchResult::Matched(matched)
    }
}

#[derive(Debug, Copy, Clone, Ord, PartialEq, PartialOrd, Eq, Hash, Display, serde::Deserialize)]
#[display("OrderId[{_0}]")]
pub struct OrderId(pub EpochSequenceId);

type Price = u32;

#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize)]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Order {
    pub id: OrderId,
    pub side: Side,
    pub price: Price,
    pub amount: u32,
}

#[derive(Debug, PartialEq, serde::Deserialize)]
pub struct OrderIntent {
    pub side: Side,
    pub price: Price,
    pub amount: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Fill {
    pub order_id: OrderId,
    pub price: u32,
    pub total_amount: u32,
    pub amount: u32,
}

impl Fill {
    pub fn is_full(&self) -> bool {
        self.total_amount == self.amount
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrderFill {
    pub order: Order,
    pub fill: Fill,
}

#[derive(Debug)]
pub struct PlacementResult {
    pub order: Order,
    pub fills: Option<Vec<OrderFill>>,
}

#[derive(Debug)]
enum MatchResult {
    Matched(Vec<Fill>),
    None,
}

#[derive(Debug)]
pub struct Book {
    pub orders: BTreeMap<OrderId, Order>,

    pub buys: BTreeMap<Price, Level>,
    pub sells: BTreeMap<Price, Level>,

    pub best_sell: Price,
    pub best_buy: Price,
}

impl Book {
    pub fn new() -> Self {
        Book {
            orders: BTreeMap::new(),
            buys: BTreeMap::new(),
            sells: BTreeMap::new(),
            best_sell: u32::MAX,
            best_buy: 0,
        }
    }

    pub fn place(&mut self, order: Order) -> PlacementResult {
        match order.side {
            Side::Buy => self.buy(order),
            Side::Sell => self.sell(order),
        }
    }

    fn process_order<'a>(
        side: Side,
        makers: impl Iterator<Item = (&'a Price, &'a mut Level)>,
        takers: &mut BTreeMap<Price, Level>,
        orders: &mut BTreeMap<OrderId, Order>,
        best_ask: &mut Price,
        best_bid: &mut Price,
        order: Order,
    ) -> PlacementResult {
        let mut order = order;
        let mut amount = order.amount;
        let mut filled_places = vec![];
        let mut is_empty = false;

        for (_, level) in makers {
            *best_ask = level.price;
            if match side {
                Side::Buy => order.price >= level.price,
                Side::Sell => order.price <= level.price,
            } {
                let result = level.matching(orders, &mut amount);

                match result {
                    MatchResult::Matched(fills) => {
                        for fill in fills {
                            let order = if fill.is_full() {
                                orders.remove(&fill.order_id)
                            } else {
                                orders.get(&fill.order_id).cloned()
                            };

                            if let Some(order) = order {
                                filled_places.push(OrderFill { order, fill });
                            }
                        }
                    }
                    MatchResult::None => {}
                }
            }

            is_empty = level.orders.is_empty();

            if amount == 0 && !is_empty {
                break;
            }
        }

        if is_empty {
            *best_ask = match side {
                Side::Buy => u32::MAX,
                Side::Sell => 0,
            }
        }

        if amount > 0 {
            match side {
                Side::Buy => {
                    if order.price > *best_bid {
                        *best_bid = order.price;
                    }
                }
                Side::Sell => {
                    if order.price < *best_bid {
                        *best_bid = order.price;
                    }
                }
            }

            order.amount = amount;

            let level = takers.entry(order.price).or_insert(Level {
                price: order.price,
                total_amount: 0,
                orders: VecDeque::with_capacity(10),
            });
            level.orders.push_back(order.id);
            level.total_amount += amount;
            orders.entry(order.id).or_insert(order.clone());
        }

        PlacementResult {
            order,
            fills: if !filled_places.is_empty() {
                Some(filled_places)
            } else {
                None
            },
        }
    }

    fn buy(&mut self, order: Order) -> PlacementResult {
        let makers = self.sells.iter_mut();
        let takers = &mut self.buys;
        let orders = &mut self.orders;
        let best_ask = &mut self.best_sell;
        let best_bid = &mut self.best_buy;

        Self::process_order(
            order.side, makers, takers, orders, best_ask, best_bid, order,
        )
    }

    fn sell(&mut self, order: Order) -> PlacementResult {
        let makers = self.buys.iter_mut().rev();
        let takers = &mut self.sells;
        let orders = &mut self.orders;
        let best_ask = &mut self.best_buy;
        let best_bid = &mut self.best_sell;

        Self::process_order(
            order.side, makers, takers, orders, best_ask, best_bid, order,
        )
    }

    pub fn cancel(&mut self, order_id: &OrderId) -> Option<Order> {
        let order = self.orders.remove(order_id)?;

        let Some(level) = (match order.side {
            Side::Buy => self.buys.get_mut(&order.price),
            Side::Sell => self.sells.get_mut(&order.price),
        }) else {
            return Some(order);
        };

        let Some(idx) = level.orders.iter().position(|x| x == order_id) else {
            return Some(order);
        };

        level.orders.remove(idx);
        level.total_amount -= order.amount;

        let mut iter = match order.side {
            Side::Buy => self.buys.iter(),
            Side::Sell => self.sells.iter(),
        };

        loop {
            let next = match order.side {
                Side::Buy => iter.next_back(),
                Side::Sell => iter.next(),
            };
            let Some((_, level)) = next else {
                break;
            };

            if !level.orders.is_empty() {
                match order.side {
                    Side::Buy => self.best_buy = level.price,
                    Side::Sell => self.best_sell = level.price,
                };
                return Some(order);
            }
        }

        match order.side {
            Side::Buy => self.best_buy = 0,
            Side::Sell => self.best_sell = u32::MAX,
        };
        Some(order)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Side::{Buy, Sell};

    fn order(id: u64, side: Side, price: u32, amount: u32) -> Order {
        Order {
            id: OrderId(EpochSequenceId::new(0, id)),
            side,
            price,
            amount,
        }
    }

    fn assert_fills(label: &str, result: &PlacementResult, expected: &[(u64, u32, u32, u32)]) {
        let actual: Vec<_> = result
            .fills
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|matched| {
                assert_eq!(matched.order.id, matched.fill.order_id, "{label}: maker ID");
                assert_eq!(
                    matched.order.price, matched.fill.price,
                    "{label}: maker price"
                );
                assert_ne!(
                    matched.order.side, result.order.side,
                    "{label}: opposite sides"
                );
                (
                    matched.fill.order_id.0.sequence,
                    matched.fill.price,
                    matched.fill.total_amount,
                    matched.fill.amount,
                )
            })
            .collect();
        assert_eq!(actual, expected, "{label}: fills for {:?}", result.order);
        if expected.is_empty() {
            assert!(result.fills.is_none(), "{label}: expected no fills");
        }
    }

    fn assert_book(label: &str, book: &Book, expected: &[Order]) {
        let orders: BTreeMap<_, _> = expected
            .iter()
            .map(|order| (order.id, order.clone()))
            .collect();
        assert_eq!(book.orders, orders, "{label}: remaining orders");

        for (side, levels) in [(Buy, &book.buys), (Sell, &book.sells)] {
            for (price, level) in levels {
                assert_eq!(level.price, *price, "{label}: level price");
                let at_price: Vec<_> = expected
                    .iter()
                    .filter(|order| order.side == side && order.price == *price)
                    .collect();
                let ids: VecDeque<_> = at_price.iter().map(|order| order.id).collect();
                assert_eq!(
                    level.orders, ids,
                    "{label}: order queue at {side:?} {price}"
                );
                assert_eq!(
                    level.total_amount,
                    at_price.iter().map(|order| order.amount).sum::<u32>(),
                    "{label}: total amount at {side:?} {price}"
                );
            }
            for order in expected.iter().filter(|order| order.side == side) {
                assert!(
                    levels.contains_key(&order.price),
                    "{label}: missing {side:?} level at {}",
                    order.price
                );
            }
        }

        assert_eq!(
            book.best_buy,
            expected
                .iter()
                .filter(|order| order.side == Buy)
                .map(|order| order.price)
                .max()
                .unwrap_or(0),
            "{label}: best buy price"
        );
        assert_eq!(
            book.best_sell,
            expected
                .iter()
                .filter(|order| order.side == Sell)
                .map(|order| order.price)
                .min()
                .unwrap_or(u32::MAX),
            "{label}: best sell price"
        );
    }

    #[test]
    fn empty_book() {
        let book = Book::new();
        assert!(book.buys.is_empty());
        assert!(book.sells.is_empty());
        assert_book("New book has no orders", &book, &[]);
    }

    #[test]
    fn places_buy_without_matches() {
        let mut book = Book::new();
        let maker = order(1, Buy, 10, 5);
        let result = book.place(maker.clone());
        assert_eq!(result.order, maker);
        assert_fills("Buy has no sell to match", &result, &[]);
        assert_book("Buy 5 at 10 remains in the book", &book, &[maker]);
    }

    #[test]
    fn places_sell_without_matches() {
        let mut book = Book::new();
        let maker = order(1, Sell, 10, 5);
        let result = book.place(maker.clone());
        assert_eq!(result.order, maker);
        assert_fills("Sell has no buy to match", &result, &[]);
        assert_book("Sell 5 at 10 remains in the book", &book, &[maker]);
    }

    #[test]
    fn buy_below_sell_remains_in_book() {
        let mut book = Book::new();
        book.place(order(1, Sell, 10, 7));

        let result = book.place(order(2, Buy, 9, 5));

        assert_fills("Buy at 9 cannot match sell at 10", &result, &[]);
        assert_book(
            "Non-crossing buy and sell both remain",
            &book,
            &[order(1, Sell, 10, 7), order(2, Buy, 9, 5)],
        );
    }

    #[test]
    fn sell_above_buy_remains_in_book() {
        let mut book = Book::new();
        book.place(order(1, Buy, 9, 5));

        let result = book.place(order(2, Sell, 10, 7));

        assert_fills("Sell at 10 cannot match buy at 9", &result, &[]);
        assert_book(
            "Non-crossing sell and buy both remain",
            &book,
            &[order(1, Buy, 9, 5), order(2, Sell, 10, 7)],
        );
    }

    #[test]
    fn buy_fully_matches_at_sell_price() {
        let mut book = Book::new();
        book.place(order(1, Sell, 10, 5));
        let taker = order(2, Buy, 11, 5);
        let result = book.place(taker.clone());
        assert_eq!(result.order, taker);
        assert_fills(
            "Buy fills all 5 at the maker sell price of 10",
            &result,
            &[(1, 10, 5, 5)],
        );
        assert_book("Fully matched buy and sell leave an empty book", &book, &[]);
    }

    #[test]
    fn sell_fully_matches_at_buy_price() {
        let mut book = Book::new();
        book.place(order(1, Buy, 10, 5));
        let taker = order(2, Sell, 9, 5);
        let result = book.place(taker.clone());
        assert_eq!(result.order, taker);
        assert_fills(
            "Sell fills all 5 at the maker buy price of 10",
            &result,
            &[(1, 10, 5, 5)],
        );
        assert_book("Fully matched sell and buy leave an empty book", &book, &[]);
    }

    #[test]
    fn buy_partially_matches_sell_and_then_matches_remainder() {
        let mut book = Book::new();
        book.place(order(1, Sell, 10, 10));
        assert_fills(
            "First buy fills 4 of the sell order",
            &book.place(order(2, Buy, 10, 4)),
            &[(1, 10, 10, 4)],
        );
        assert_book(
            "Sell remainder of 6 stays available at 10",
            &book,
            &[order(1, Sell, 10, 6)],
        );
        assert_fills(
            "Second buy fills the remaining 6",
            &book.place(order(3, Buy, 10, 6)),
            &[(1, 10, 6, 6)],
        );
        assert_book("Second buy leaves an empty book", &book, &[]);
    }

    #[test]
    fn sell_partially_matches_buy_and_then_matches_remainder() {
        let mut book = Book::new();
        book.place(order(1, Buy, 10, 10));
        assert_fills(
            "First sell fills 4 of the buy order",
            &book.place(order(2, Sell, 10, 4)),
            &[(1, 10, 10, 4)],
        );
        assert_book(
            "Buy remainder of 6 stays available at 10",
            &book,
            &[order(1, Buy, 10, 6)],
        );
        assert_fills(
            "Second sell fills the remaining 6",
            &book.place(order(3, Sell, 10, 6)),
            &[(1, 10, 6, 6)],
        );
        assert_book("Second sell leaves an empty book", &book, &[]);
    }

    #[test]
    fn stores_only_buy_remainder_at_its_limit_price() {
        let mut book = Book::new();
        book.place(order(1, Sell, 10, 4));
        assert_fills(
            "Buy fills the sell quantity of 4 at 10",
            &book.place(order(2, Buy, 11, 10)),
            &[(1, 10, 4, 4)],
        );
        assert_book(
            "Only buy remainder of 6 rests at its limit of 11",
            &book,
            &[order(2, Buy, 11, 6)],
        );
    }

    #[test]
    fn stores_only_sell_remainder_at_its_limit_price() {
        let mut book = Book::new();
        book.place(order(1, Buy, 10, 4));
        assert_fills(
            "Sell fills the buy quantity of 4 at 10",
            &book.place(order(2, Sell, 9, 10)),
            &[(1, 10, 4, 4)],
        );
        assert_book(
            "Only sell remainder of 6 rests at its limit of 9",
            &book,
            &[order(2, Sell, 9, 6)],
        );
    }

    #[test]
    fn buy_matches_cheapest_sells_first_and_stops_at_limit() {
        let mut book = Book::new();
        book.place(order(1, Sell, 11, 2));
        book.place(order(2, Sell, 9, 2));
        book.place(order(3, Sell, 10, 2));

        let result = book.place(order(4, Buy, 10, 5));

        assert_fills(
            "Buy fills sells at 9 then 10, skipping sell at 11",
            &result,
            &[(2, 9, 2, 2), (3, 10, 2, 2)],
        );
        assert_book(
            "Sell at 11 remains alongside buy remainder of 1 at 10",
            &book,
            &[order(1, Sell, 11, 2), order(4, Buy, 10, 1)],
        );
    }

    #[test]
    fn sell_matches_highest_buys_first_and_stops_at_limit() {
        let mut book = Book::new();
        book.place(order(1, Buy, 9, 2));
        book.place(order(2, Buy, 11, 2));
        book.place(order(3, Buy, 10, 2));

        let result = book.place(order(4, Sell, 10, 5));

        assert_fills(
            "Sell fills buys at 11 then 10, skipping buy at 9",
            &result,
            &[(2, 11, 2, 2), (3, 10, 2, 2)],
        );
        assert_book(
            "Buy at 9 remains alongside sell remainder of 1 at 10",
            &book,
            &[order(1, Buy, 9, 2), order(4, Sell, 10, 1)],
        );
    }

    #[test]
    fn buy_matches_same_price_sells_in_placement_order() {
        let mut book = Book::new();
        // IDs deliberately differ from placement order.
        book.place(order(3, Sell, 10, 2));
        book.place(order(1, Sell, 10, 2));
        book.place(order(2, Sell, 10, 2));
        assert_fills(
            "Buy fills sell IDs 3 then 1 in placement order",
            &book.place(order(4, Buy, 10, 4)),
            &[(3, 10, 2, 2), (1, 10, 2, 2)],
        );
        assert_book(
            "Only sell ID 2 remains available",
            &book,
            &[order(2, Sell, 10, 2)],
        );
        assert_fills(
            "Next buy fills the last sell, ID 2",
            &book.place(order(5, Buy, 10, 2)),
            &[(2, 10, 2, 2)],
        );
        assert_book("All same-price sells have been filled", &book, &[]);
    }

    #[test]
    fn sell_matches_same_price_buys_in_placement_order() {
        let mut book = Book::new();
        // IDs deliberately differ from placement order.
        book.place(order(3, Buy, 10, 2));
        book.place(order(1, Buy, 10, 2));
        book.place(order(2, Buy, 10, 2));
        assert_fills(
            "Sell fills buy IDs 3 then 1 in placement order",
            &book.place(order(4, Sell, 10, 4)),
            &[(3, 10, 2, 2), (1, 10, 2, 2)],
        );
        assert_book(
            "Only buy ID 2 remains available",
            &book,
            &[order(2, Buy, 10, 2)],
        );
        assert_fills(
            "Next sell fills the last buy, ID 2",
            &book.place(order(5, Sell, 10, 2)),
            &[(2, 10, 2, 2)],
        );
        assert_book("All same-price buys have been filled", &book, &[]);
    }

    #[test]
    fn cancels_sell_and_keeps_other_sell_matchable() {
        let mut book = Book::new();
        let first = order(1, Sell, 10, 2);
        let second = order(2, Sell, 10, 3);
        book.place(first.clone());
        book.place(second.clone());
        assert_eq!(book.cancel(&first.id), Some(first));
        assert_book(
            "Only the second sell remains after cancellation",
            &book,
            &[second],
        );
        assert_fills(
            "Buy still fills the remaining sell after cancellation",
            &book.place(order(3, Buy, 10, 3)),
            &[(2, 10, 3, 3)],
        );
        assert_book("Remaining sell has been fully filled", &book, &[]);
    }

    #[test]
    fn cancels_buy_and_keeps_other_buy_matchable() {
        let mut book = Book::new();
        let first = order(1, Buy, 10, 2);
        let second = order(2, Buy, 10, 3);
        book.place(first.clone());
        book.place(second.clone());
        assert_eq!(book.cancel(&first.id), Some(first));
        assert_book(
            "Only the second buy remains after cancellation",
            &book,
            &[second],
        );
        assert_fills(
            "Sell still fills the remaining buy after cancellation",
            &book.place(order(3, Sell, 10, 3)),
            &[(2, 10, 3, 3)],
        );
        assert_book("Remaining buy has been fully filled", &book, &[]);
    }

    #[test]
    fn unknown_and_repeated_buy_cancellation_leave_book_unchanged() {
        let mut book = Book::new();
        let maker = order(1, Buy, 10, 5);
        book.place(maker.clone());
        assert_eq!(book.cancel(&order(99, Buy, 10, 5).id), None);
        assert_book(
            "Unknown ID leaves the buy unchanged",
            &book,
            &[maker.clone()],
        );
        assert_eq!(book.cancel(&maker.id), Some(maker.clone()));
        assert_book("Cancelling the buy leaves an empty book", &book, &[]);
        assert_eq!(book.cancel(&maker.id), None);
        assert_book("Repeated cancellation keeps the book empty", &book, &[]);
    }

    #[test]
    fn unknown_and_repeated_sell_cancellation_leave_book_unchanged() {
        let mut book = Book::new();
        let maker = order(1, Sell, 10, 5);
        book.place(maker.clone());
        assert_eq!(book.cancel(&order(99, Sell, 10, 5).id), None);
        assert_book(
            "Unknown ID leaves the sell unchanged",
            &book,
            &[maker.clone()],
        );
        assert_eq!(book.cancel(&maker.id), Some(maker.clone()));
        assert_book("Cancelling the sell leaves an empty book", &book, &[]);
        assert_eq!(book.cancel(&maker.id), None);
        assert_book("Repeated cancellation keeps the book empty", &book, &[]);
    }

    #[test]
    fn cancels_partially_filled_sell() {
        let mut book = Book::new();
        book.place(order(1, Sell, 10, 10));
        book.place(order(2, Buy, 10, 4));
        let remainder = order(1, Sell, 10, 6);
        assert_eq!(book.cancel(&remainder.id), Some(remainder));
        assert_book(
            "Cancelling the remaining sell quantity of 6 leaves an empty book",
            &book,
            &[],
        );
    }

    #[test]
    fn cancels_partially_filled_buy() {
        let mut book = Book::new();
        book.place(order(1, Buy, 10, 10));
        book.place(order(2, Sell, 10, 4));
        let remainder = order(1, Buy, 10, 6);
        assert_eq!(book.cancel(&remainder.id), Some(remainder));
        assert_book(
            "Cancelling the remaining buy quantity of 6 leaves an empty book",
            &book,
            &[],
        );
    }

    #[test]
    fn updates_best_sell_after_matching_best_sell_level() {
        let mut book = Book::new();
        book.place(order(1, Sell, 10, 2));
        book.place(order(2, Sell, 9, 2));
        assert_book(
            "Best sell is 9 before matching",
            &book,
            &[order(1, Sell, 10, 2), order(2, Sell, 9, 2)],
        );
        assert_fills(
            "Buy fills the best sell at 9",
            &book.place(order(3, Buy, 9, 2)),
            &[(2, 9, 2, 2)],
        );
        assert_book(
            "Remaining sell at 10 becomes the best sell",
            &book,
            &[order(1, Sell, 10, 2)],
        );
    }

    #[test]
    fn updates_best_buy_after_matching_best_buy_level() {
        let mut book = Book::new();
        book.place(order(1, Buy, 10, 2));
        book.place(order(2, Buy, 11, 2));
        assert_book(
            "Best buy is 11 before matching",
            &book,
            &[order(1, Buy, 10, 2), order(2, Buy, 11, 2)],
        );
        assert_fills(
            "Sell fills the best buy at 11",
            &book.place(order(3, Sell, 11, 2)),
            &[(2, 11, 2, 2)],
        );
        assert_book(
            "Remaining buy at 10 becomes the best buy",
            &book,
            &[order(1, Buy, 10, 2)],
        );
    }

    #[test]
    fn updates_best_sell_after_cancellation() {
        let mut book = Book::new();
        let worse = order(1, Sell, 10, 2);
        let better = order(2, Sell, 9, 3);
        book.place(worse.clone());
        book.place(better.clone());
        assert_book(
            "Best sell is 9 before cancellation",
            &book,
            &[worse.clone(), better.clone()],
        );
        assert_eq!(book.cancel(&better.id), Some(better));
        assert_book(
            "Cancelling sell at 9 makes 10 the best sell",
            &book,
            &[worse.clone()],
        );
        assert_eq!(book.cancel(&worse.id), Some(worse));
        assert_book(
            "Cancelling the last sell resets the best sell price",
            &book,
            &[],
        );
    }

    #[test]
    fn updates_best_buy_after_cancellation() {
        let mut book = Book::new();
        let worse = order(1, Buy, 10, 2);
        let better = order(2, Buy, 11, 3);
        book.place(worse.clone());
        book.place(better.clone());
        assert_book(
            "Best buy is 11 before cancellation",
            &book,
            &[worse.clone(), better.clone()],
        );
        assert_eq!(book.cancel(&better.id), Some(better));
        assert_book(
            "Cancelling buy at 11 makes 10 the best buy",
            &book,
            &[worse.clone()],
        );
        assert_eq!(book.cancel(&worse.id), Some(worse));
        assert_book(
            "Cancelling the last buy resets the best buy price",
            &book,
            &[],
        );
    }
}
