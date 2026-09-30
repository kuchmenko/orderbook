use std::{
    cmp::{self, max, min},
    collections::{HashMap, VecDeque},
    fmt::Display,
    u32,
};

use uuid::Uuid;

#[derive(Debug)]
struct Level {
    price: Price,
}

type OrderId = uuid::Uuid;
type Price = u32;

#[derive(Debug)]
enum Side {
    Buy,
    Sell,
}

#[derive(Debug)]
struct Order {
    id: OrderId,
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
enum MatchResult {
    Matched(Vec<OrderId>),
    None,
}

#[derive(Debug)]
struct Market {
    orders_ids: HashMap<OrderId, Order>,

    buys: HashMap<Price, VecDeque<OrderId>>,
    sells: HashMap<Price, VecDeque<OrderId>>,

    best_sell: u32,
    best_buy: u32,
}

impl Display for Market {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("\n");
        f.write_str(&format!("sells-- best:{}\n", self.best_sell));
        for (key, value) in &self.sells {
            if value.len() == 0 {
                continue;
            }
            let formatted = format!(
                "---> {}  at  {}\n",
                value.iter().sum::<u32>(),
                key.to_string(),
            );
            f.write_str(&formatted)?;
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
                if value.len() == 0 {
                    continue;
                }
                let formatted = format!(
                    "---> {} at {}\n",
                    value.iter().sum::<u32>(),
                    key.to_string(),
                );
                f.write_str(&formatted)?;
            }
        }

        Ok(())
    }
}

impl Market {
    fn new() -> Self {
        Market {
            orders_ids: HashMap::new(),
            buys: HashMap::new(),
            sells: HashMap::new(),
            best_sell: u32::MAX,
            best_buy: 0,
        }
    }

    fn order(&mut self, intent: OrderIntent) -> Order {
        let id = 1;

        match intent.side {
            Side::Buy => todo!(),
            Side::Sell => todo!(),
        }
    }

    fn buy(&mut self, intent: OrderIntent) -> Option<&Order> {
        let mut amount = intent.amount;
        let mut price_level = self.best_sell;

        while price_level < intent.price {
            if let Some(level) = self.sells.get_mut(&price_level) {
                let result = self.match_level(level, &mut amount);

                match result {
                    MatchResult::Matched(orders) => {
                        for order in orders {
                            self.orders_ids.remove(&order);
                        }
                    }
                    MatchResult::None => continue,
                }
            }
            if amount == 0 {
                break;
            }

            price_level += 1
        }
        let mut keys: Vec<u32> = self.sells.keys().copied().collect();
        keys.sort_unstable();

        for key in &keys {
            if key < &price_level {
                continue;
            }

            let Some(level) = self.sells.get(&key) else {
                continue;
            };
            if !level.is_empty() {
                self.best_sell = *key;
                return None;
            }
        }

        if amount > 0 {
            let order_id = Uuid::now_v7();
            let order = Order {
                id: order_id,
                amount: intent.amount,
                price: intent.price,
            };

            self.buys
                .entry(intent.price)
                .or_default()
                .push_front(order_id);
            self.orders_ids.entry(order_id).or_insert(order);
            if intent.price > self.best_buy {
                self.best_buy = intent.price;
            }

            return self.orders_ids.get(&order_id);
        }

        self.best_sell = 0;
        None
    }

    fn sell(&mut self, intent: OrderIntent) -> Option<&Order> {
        let mut amount = intent.amount;
        let mut price_level = self.best_buy;

        while price_level >= intent.price {
            if let Some(level) = self.buys.get_mut(&price_level) {
                let result = self.match_level(level, &mut amount);

                match result {
                    MatchResult::Matched(orders) => {
                        for order in orders {
                            self.orders_ids.remove(&order);
                        }
                    }
                    MatchResult::None => continue,
                }
            }

            if amount == 0 {
                break;
            }
            price_level -= 1;
        }

        let mut keys: Vec<u32> = self.buys.keys().copied().collect();
        keys.sort_unstable();

        for key in keys.into_iter().rev() {
            if key > price_level {
                continue;
            }

            let Some(level) = self.buys.get(&key) else {
                continue;
            };
            if !level.is_empty() {
                self.best_buy = key;
            }
        }

        if amount > 0 {
            let order_id = Uuid::now_v7();
            let order = Order {
                id: order_id,
                amount: intent.amount,
                price: intent.price,
            };

            self.sells
                .entry(intent.price)
                .or_default()
                .push_front(order_id);
            self.orders_ids.entry(order.id).or_insert(order);
            if intent.price < self.best_sell {
                self.best_sell = intent.price;
            }

            return self.orders_ids.get(&order_id);
        }

        self.best_buy = 0;

        None
    }

    fn match_level(&mut self, orders: &mut VecDeque<OrderId>, amount: &mut u32) -> MatchResult {
        if orders.len() == 0 {
            return MatchResult::None;
        }

        let mut i = 0;
        let mut matched = vec![];

        while i < orders.len() {
            if *amount == 0 {
                break;
            }

            if let Some(order) = self.orders_ids.get_mut(&orders[i]) {
                let taker_amount = min(order.amount, *amount);

                order.amount -= taker_amount;
                *amount -= taker_amount;

                if order.amount == 0 {
                    if let Some(order) = orders.remove(i) {
                        matched.push(order);
                    }
                }
            }

            i += 1
        }

        if matched.len() == 0 {
            return MatchResult::None;
        }

        return MatchResult::Matched(matched);
    }
}

// #[test]
// fn test_should_update_best_buy() {
//     let mut market = Market::new();
//
//     market.buy(12, 5);
//
//     assert_eq!(market.best_buy, 5);
//
//     market.buy(20, 4);
//
//     assert_eq!(market.best_buy, 5);
//
//     market.sell(15, 4);
//
//     assert_eq!(market.buys.get(&5).unwrap().len(), 0);
//     assert_eq!(market.buys.get(&4).unwrap()[0], 17);
//     assert_eq!(market.best_buy, 4);
//
//     market.sell(2, 4);
//     market.sell(17, 4);
//
//     assert_eq!(market.buys.get(&4).unwrap().len(), 0);
//     assert_eq!(market.sells.get(&4).unwrap()[0], 2);
//     println!("mm: {}", market);
// }

fn main() {
    let mut market = Market::new();
    //
    // market.sell(5, 11);
    // market.sell(13, 10);
    // market.sell(2, 7);
    // market.sell(10, 6);
    //
    // println!("market {}", market);
    //
    // market.buy(13, 9);
    // market.buy(53, 4);
    // market.buy(100, 2);
    //
    // println!("market {}", market);
    // market.sell(63, 3);
    // println!("market {}", market);
    // market.buy(9, 10);
    // market.buy(12, 9);
    // println!("market {}", market);
    // market.sell(12, 5);
    // println!("market {}", market);
    // market.sell(100, 1);
    // println!("market {}", market);
    // market.sell(12, 3); // 12 at 3
    // market.sell(15, 5); // 15 at 5
    //
    // println!("market {}", market);
    //
    // market.buy(100, 11); //  10 buy at 11
    // market.buy(352, 1); //  352 buy at 1
    //
    // println!("market {}", market);
    //
    // market.sell(173, 5); // 8 - 15 = -7 = 7 sell at 5
    //
    // println!("market 173 {}", market);
    //
    // market.buy(400, 15); //  352 buy at 1
    // println!("market {}", market);
    println!("Hello, world!");
}
