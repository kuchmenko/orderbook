use std::{
    cmp::min,
    collections::{BTreeMap, VecDeque},
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
        println!("matching level: {:?}", self);
        if self.orders.is_empty() {
            return MatchResult::None;
        }

        let mut matched = vec![];
        let mut matched_ids = vec![];

        for (id, order_id) in self.orders.iter().enumerate() {
            if *amount == 0 {
                break;
            }

            if let Some(order) = orders.get_mut(&order_id) {
                let taker_amount = min(order.amount, *amount);
                let total_amount = order.amount;

                order.amount -= taker_amount;
                *amount -= taker_amount;
                self.total_amount -= taker_amount;

                println!("matched order {:?}, taker_amount: {}", order, taker_amount);

                matched.push(Fill {
                    order_id: order.id,
                    price: self.price,
                    total_amount,
                    amount: taker_amount,
                });
                matched_ids.push(id);
            } else {
                println!("not found: {}", order_id);
            }
        }

        for id in matched_ids {
            self.orders.remove(id);
        }

        if matched.is_empty() {
            return MatchResult::None;
        }

        MatchResult::Matched(matched)
    }
}

#[derive(Debug, Copy, Clone, Ord, PartialEq, PartialOrd, Eq, Hash, Display)]
#[display("OrderId[{_0}]")]
pub struct OrderId(pub EpochSequenceId);

type Price = u32;

#[derive(Debug, Clone, Copy, PartialEq)]
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

#[derive(Debug, PartialEq)]
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

    pub best_sell: u32,
    pub best_buy: u32,
}

impl Display for Book {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("\n");
        f.write_str(&format!("sells-- best:{}\n", self.best_sell));
        let mut sells = self.sells.keys().clone().collect::<Vec<&u32>>();
        sells.sort();
        sells.reverse();

        for key in &sells {
            if let Some(value) = self.sells.get(key) {
                if value.total_amount == 0 {
                    continue;
                }

                let formatted = format!("---> {}  at  {}\n", value.total_amount, value.price);
                f.write_str(&formatted)?;
            }
        }

        // f.write_str(&format!(
        //     "\n\n SPREAD: {} \n\n",
        //     self.best_buy - self.best_sell,
        // ))?;

        let mut buys = self.buys.keys().clone().collect::<Vec<&u32>>();
        buys.sort();
        buys.reverse();

        f.write_str(&format!("buys-- best:{}\n", self.best_buy));
        for key in buys {
            if let Some(value) = self.buys.get(key) {
                if value.total_amount == 0 {
                    continue;
                }
                let formatted = format!("---> {} at {}\n", value.total_amount, value.price);
                f.write_str(&formatted)?;
            }
        }

        Ok(())
    }
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

    fn buy(&mut self, order: Order) -> PlacementResult {
        let mut amount = order.amount;
        let mut filled_places = vec![];
        let iter = self.sells.iter_mut();

        for (_, level) in iter {
            if level.price <= order.price {
                println!("level {}, matching {}", level.price, amount);
                let orders = &mut self.orders;
                let result = level.matching(orders, &mut amount);

                match result {
                    MatchResult::Matched(fills) => {
                        for fill in fills {
                            if fill.total_amount != fill.amount {
                                continue;
                            }
                            println!("fill: {:?}", fill);
                            if let Some(order) = self.orders.remove(&fill.order_id) {
                                filled_places.push(OrderFill { order, fill });
                            }
                        }
                    }
                    MatchResult::None => {}
                }
            }

            if amount == 0 && !level.orders.is_empty() {
                self.best_buy = level.price;
                break;
            }
        }

        if amount > 0 {
            if order.price > self.best_buy {
                self.best_buy = order.price;
            }

            let level = self.buys.entry(order.price).or_insert(Level {
                price: order.price,
                total_amount: 0,
                orders: VecDeque::with_capacity(10),
            });
            level.orders.push_front(order.id);
            level.total_amount += amount;

            self.orders.entry(order.id).or_insert(order.clone());
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

    fn sell(&mut self, order: Order) -> PlacementResult {
        let mut amount = order.amount;
        let mut filled_places = vec![];
        let iter = self.buys.iter_mut().rev();

        for (_, level) in iter {
            if level.price >= order.price {
                let orders = &mut self.orders;
                let result = level.matching(orders, &mut amount);

                match result {
                    MatchResult::Matched(fills) => {
                        for fill in fills {
                            if fill.total_amount != fill.amount {
                                continue;
                            }
                            println!("fill: {:?}", fill);
                            if let Some(order) = self.orders.remove(&fill.order_id) {
                                filled_places.push(OrderFill { order, fill });
                            }
                        }
                    }
                    MatchResult::None => {}
                }
            }

            if amount == 0 && !level.orders.is_empty() {
                self.best_buy = level.price;
                break;
            }
        }

        if amount > 0 {
            if order.price < self.best_sell {
                self.best_sell = order.price;
            }

            let level = self.sells.entry(order.price).or_insert(Level {
                price: order.price,
                total_amount: 0,
                orders: VecDeque::with_capacity(10),
            });
            level.orders.push_front(order.id);
            level.total_amount += amount;
            self.orders.entry(order.id).or_insert(order.clone());
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

    pub fn cancel(&mut self, order_id: &OrderId) -> Option<Order> {
        if let Some(order) = self.orders.remove(order_id) {
            let Some(level) = (match order.side {
                Side::Buy => self.buys.get_mut(&order.price),
                Side::Sell => self.sells.get_mut(&order.price),
            }) else {
                return Some(order);
            };

            if let Some(idx) = level.orders.iter().position(|x| x == order_id) {
                level.orders.remove(idx);
                level.total_amount -= order.amount;

                return Some(order);
            }
        }
        None
    }
}
