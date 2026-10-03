use std::{
    cmp::min,
    collections::{BTreeMap, HashMap, VecDeque},
    fmt::Display,
};

use uuid::Uuid;

#[derive(Debug, Default)]
struct Level {
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

type OrderId = uuid::Uuid;
type Price = u32;

#[derive(Debug, Clone)]
enum Side {
    Buy,
    Sell,
}

#[derive(Debug, Clone)]
struct Order {
    id: OrderId,
    side: Side,
    price: Price,
    amount: u32,
}

#[derive(Debug)]
struct OrderIntent {
    side: Side,
    price: Price,
    amount: u32,
}

#[derive(Debug)]
struct Fill {
    order_id: OrderId,
    price: u32,
    total_amount: u32,
    amount: u32,
}

#[derive(Debug)]
struct PlacementResult {
    order: Order,
    fills: Option<Vec<Order>>,
}

#[derive(Debug)]
enum MatchResult {
    Matched(Vec<Fill>),
    None,
}

#[derive(Debug)]
struct Book {
    orders: BTreeMap<OrderId, Order>,

    buys: BTreeMap<Price, Level>,
    sells: BTreeMap<Price, Level>,

    best_sell: u32,
    best_buy: u32,
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
    fn new() -> Self {
        Book {
            orders: BTreeMap::new(),
            buys: BTreeMap::new(),
            sells: BTreeMap::new(),
            best_sell: u32::MAX,
            best_buy: 0,
        }
    }

    fn place(&mut self, intent: OrderIntent) -> PlacementResult {
        match intent.side {
            Side::Buy => self.buy(intent),
            Side::Sell => self.sell(intent),
        }
    }

    fn buy(&mut self, intent: OrderIntent) -> PlacementResult {
        let mut amount = intent.amount;
        let mut filled_places = vec![];
        let iter = self.sells.iter_mut();

        for (_, level) in iter {
            if level.price <= intent.price {
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
                                filled_places.push(order);
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

        let order = Order {
            id: Uuid::now_v7(),
            side: intent.side,
            amount,
            price: intent.price,
        };

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

    fn sell(&mut self, intent: OrderIntent) -> PlacementResult {
        let mut amount = intent.amount;
        let mut filled_places = vec![];
        let iter = self.buys.iter_mut().rev();

        for (_, level) in iter {
            if level.price >= intent.price {
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
                                filled_places.push(order);
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

        let order = Order {
            id: Uuid::now_v7(),
            side: intent.side,
            amount,
            price: intent.price,
        };

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

    fn cancel(&mut self, order_id: &OrderId) -> Option<Order> {
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

fn main() {
    let mut market = Book::new();
    // let mut places = vec![];

    // market.sell(5, 11);
    market.place(OrderIntent {
        side: Side::Sell,
        price: 11,
        amount: 5,
    });

    // market.sell(13, 10);
    market.place(OrderIntent {
        side: Side::Sell,
        price: 10,
        amount: 13,
    });
    // market.sell(2, 7);
    market.place(OrderIntent {
        side: Side::Sell,
        price: 7,
        amount: 2,
    });
    // market.sell(10, 6);
    market.place(OrderIntent {
        side: Side::Sell,
        price: 6,
        amount: 10,
    });
    //
    //
    println!("market {}", market);
    //
    // market.buy(13, 9);
    market.place(OrderIntent {
        side: Side::Buy,
        price: 9,
        amount: 13,
    });
    // market.buy(53, 4);
    market.place(OrderIntent {
        side: Side::Buy,
        price: 4,
        amount: 53,
    });
    // market.buy(100, 2);
    market.place(OrderIntent {
        side: Side::Buy,
        price: 2,
        amount: 100,
    });
    //
    println!("market {}", market);
    // // market.sell(63, 3);
    market.place(OrderIntent {
        side: Side::Sell,
        price: 3,
        amount: 63,
    }); // rest 9 at price 3
    println!("market {}", market);
    // // // // market.buy(9, 10);
    market.place(OrderIntent {
        side: Side::Buy,
        price: 10,
        amount: 10,
    }); //rest 1 at price 10
    println!("market {}", market);
    // market.buy(12, 9);
    market.place(OrderIntent {
        side: Side::Buy,
        price: 9,
        amount: 12,
    }); // rest 12 at price 9
    println!("market {}", market);
    // // // // // market.sell(12, 5);
    market.place(OrderIntent {
        side: Side::Sell,
        price: 5,
        amount: 12,
    }); // no rest, leave  1 at 9
    println!("market {}", market);
    // // // // market.sell(100, 1);
    market.place(OrderIntent {
        side: Side::Sell,
        price: 1,
        amount: 100,
    }); // buy 1 at 9, no rest
    println!("market {}", market);
    // // market.cancel(&first.order.id);
    // // println!("market {}", market);
    // //
    // // market.cancel(&second.order.id);
    // // println!("market {}", market);
    //
    // market.sell(12, 3); // 12 at 3
    market.place(OrderIntent {
        side: Side::Sell,
        price: 3,
        amount: 12,
    });
    // market.sell(15, 5); // 15 at 5
    market.place(OrderIntent {
        side: Side::Sell,
        price: 5,
        amount: 15,
    });
    //
    println!("market {}", market);
    //
    // market.buy(100, 11); //  10 buy at 11
    market.place(OrderIntent {
        side: Side::Buy,
        price: 11,
        amount: 100,
    });
    println!("market {}", market);
    // // market.buy(352, 1); //  352 buy at 1
    market.place(OrderIntent {
        side: Side::Buy,
        price: 1,
        amount: 352,
    });
    //
    println!("market {}", market);
    // //
    // // market.sell(173, 5); // 8 - 15 = -7 = 7 sell at 5
    market.place(OrderIntent {
        side: Side::Sell,
        price: 5,
        amount: 173,
    });
    //
    println!("market 173 {}", market);
    // //
    // // market.buy(400, 15); //  352 buy at 1
    // market.place(PlaceIntent {
    //     side: Side::Buy,
    //     price: 15,
    //     amount: 400,
    // });
    // println!("market {}", market);
    println!("Hello, world!");
}
